//! Dynamic (subprocess) entrypoint for the ntfy plugin.
//!
//! The toolkit's `serve_tool_plugin!` (hybrid arm) emits `fn main`, serving this
//! plugin over the orca socket. Dynamic replacement for the retired cdylib
//! `export_tool_plugin!` FFI export — the plugin is a `[[bin]]`, owns no
//! runtime, and reaches orca only through the socket.
//!
//! ntfy is a HYBRID plugin: it exposes its own `ntfy.*` tool surface AND
//! registers one notification backend per enabled endpoint row. The tool
//! surface is derived from the linked inventory by the macro; the two
//! notification hooks below reproduce, verbatim in behaviour, what the cdylib's
//! `backends()` / `invoke_backend()` exports used to cross the FFI boundary:
//!
//!   - [`ntfy_backends_json`] advertises one `notifications` backend per enabled
//!     endpoint, each with the `notify.__backend.<name>` invoke prefix.
//!   - [`ntfy_backend_dispatch`] handles the `notify.__backend.<name>.emit`
//!     callbacks the host's `NotifyProxy` drives, rebuilding the endpoint's
//!     `NtfyBackend` from its db row and driving `emit`. Returns `None` for
//!     anything outside the backend prefix so tool dispatch handles it.
#![allow(clippy::disallowed_types)]

use plugin_toolkit::notify::{Backend, Event};
use plugin_toolkit::serde_json as sj;

use ntfy::backend::NtfyBackend;
use ntfy::tools::endpoint_db;

/// Reserved tool-name family the notifications `NotifyProxy` calls back through
/// for each registered endpoint: `notify.__backend.<endpoint>.emit`. Mirrors
/// storage's `storage.__backend.<name>.*` seam. Each enabled endpoint row
/// advertises an invoke prefix of `{BACKEND_PREFIX}{name}` in
/// [`ntfy_backends_json`].
const BACKEND_PREFIX: &str = "notify.__backend.";

/// The notification backends this plugin contributes, serialized as the JSON
/// array the macro hands to orca: one per enabled ntfy endpoint row, named
/// after the endpoint so the global dispatcher routes `send = ["<endpoint>"]`
/// to it. Enumerated from the db at load time; the contract is unchanged from
/// the static plugin's `bootstrap` — endpoint add/remove applies on next daemon
/// restart. Non-fatal on db error: an empty array degrades to "no backends
/// registered" rather than failing the whole plugin.
fn ntfy_backends_json() -> String {
    let defs: Vec<sj::Value> = enabled_endpoints()
        .into_iter()
        .map(|row| {
            sj::json!({
                "domain": "notifications",
                "name": row.name,
                "kind": "",
                "endpoint": row.base_url,
                "capabilities": ["emit"],
                "invoke_prefix": format!("{BACKEND_PREFIX}{}", row.name),
            })
        })
        .collect();
    sj::to_string(&defs).unwrap_or_else(|_| "[]".to_string())
}

/// Enabled endpoint rows, or empty on any db error (plugin load must not fail
/// because the notification table is momentarily unreadable).
fn enabled_endpoints() -> Vec<ntfy::tools::EndpointRow> {
    match endpoint_db::list() {
        Ok(rows) => rows.into_iter().filter(|r| r.enabled).collect(),
        Err(_) => Vec::new(),
    }
}

/// Backend-dispatch hook for the `notify.__backend.<endpoint>.emit` callbacks.
/// Returns `None` for any tool outside the backend prefix so the macro's tool
/// dispatch handles it. Stateless across calls, so it rebuilds the endpoint's
/// [`NtfyBackend`] from its db row and drives `emit`.
fn ntfy_backend_dispatch(tool: &str, args_json: &str) -> Option<Result<String, String>> {
    let rest = tool.strip_prefix(BACKEND_PREFIX)?;
    Some(invoke_backend(rest, args_json))
}

/// Route one notification-backend proxy op. `rest` is `"<endpoint>.<op>"`; the
/// only op is `emit`.
fn invoke_backend(rest: &str, args_json: &str) -> Result<String, String> {
    let Some((endpoint, op)) = rest.rsplit_once('.') else {
        return Err(format!("malformed backend invoke '{BACKEND_PREFIX}{rest}'"));
    };
    if op != "emit" {
        return Err(format!("ntfy notification backend has no operation '{op}'"));
    }
    let event: Event = sj::from_str(args_json).map_err(|e| format!("invalid emit args: {e}"))?;
    let backend = backend_for(endpoint)?;
    match plugin_toolkit::reactor::block_on(backend.emit(&event)) {
        Ok(msg) => sj::to_string(&msg).map_err(|e| format!("failed to encode result: {e}")),
        Err(e) => Err(format!("{e}")),
    }
}

/// Build the [`NtfyBackend`] for endpoint `name` from its db row. Errors as a
/// plain string (the backend-dispatch boundary carries no typed error).
fn backend_for(name: &str) -> Result<NtfyBackend, String> {
    let row = endpoint_db::get(name)
        .map_err(|e| format!("load endpoint '{name}': {e}"))?
        .ok_or_else(|| format!("ntfy endpoint '{name}' not registered"))?;
    let mut cfg = ntfy::Config::new(row.base_url, row.topic);
    if let Some(t) = row.token {
        cfg = cfg.with_token(t);
    }
    Ok(NtfyBackend::new(row.name, ntfy::Client::new(cfg)))
}

plugin_toolkit::serve_tool_plugin! {
    name: "ntfy",
    target_compat: "",
    backends: ntfy_backends_json(),
    backend_dispatch: ntfy_backend_dispatch,
}
