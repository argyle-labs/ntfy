// The tool surface crosses this FFI boundary as opaque JSON — the designated
// JSON dispatch seam, identical to orca's `plugin-loader` and
// `dispatch::ErasedTool::run_json`. The payload type is aliased (`sj`) at this
// one seam, exactly as the loader aliases it, and the workspace
// disallowed-types lint is suppressed for this file only.
#![allow(clippy::disallowed_types)]

//! ABI-stable cdylib export.
//!
//! Builds and exports the single [`PluginModRef`] root module orca's
//! `plugin-loader` `dlopen`s. The accessor fns carry the version header the
//! loader reads before invoking anything; `manifest`/`invoke` wrap this crate's
//! own statically-linked tool inventory (`ntfy.send` + the `endpoint_resource!`
//! CRUD `ntfy.{list,detail,create,update,delete}` + the lifecycle tools
//! `ntfy.{install,upgrade,backup,restore}`) through the toolkit's re-exported
//! dispatch surface. `backends` additionally advertises one `notifications`
//! domain backend per enabled endpoint row, which the host's `NotifyProxy`
//! drives back through `invoke` under `notify.__backend.<endpoint>.emit`.
//!
//! Only the entrypoint + metadata cross as `StableAbi` types; the tool surface
//! itself crosses as JSON (manifest array + invoke args/result strings),
//! exactly as the toolkit `abi` contract specifies.

use std::sync::Arc;
use std::sync::OnceLock;

// The `#[export_root_module]` attribute expands to bare `::abi_stable` paths in
// this crate's root, so `abi_stable` must be a direct dependency — it is a
// genuinely-external (non-orca) crate, exactly like the progenitor/serde deps
// the generated client carries. Pinned to the toolkit's version so the layout
// hash the loader checks matches.
use abi_stable::export_root_module;
use abi_stable::prefix_type::PrefixTypeTrait;
use abi_stable::std_types::{RErr, ROk, RResult, RStr, RString};
use plugin_toolkit::abi::{BackendDef, PluginMod, PluginModRef, ToolDef};
use plugin_toolkit::contract::config::{Config, Model, Ports};
use plugin_toolkit::contract::ToolCtx;
use plugin_toolkit::dispatch::{dispatch, tool_manifest_json};
// The notification-domain types this plugin's backend seam crosses: `emit`
// deserializes an `Event` and returns a `MessageRef` (via the `Backend` trait).
use plugin_toolkit::notify::{Backend, Event};
// The JSON dispatch payload type, named once here at the designated opaque seam.
use plugin_toolkit::serde_json as sj;
use plugin_toolkit::tokio::runtime::{Builder, Runtime};

use crate::backend::NtfyBackend;
use crate::tools::endpoint_db;

extern "C" fn plugin_semver() -> RString {
    RString::from(env!("CARGO_PKG_VERSION"))
}

extern "C" fn target_software() -> RString {
    RString::from("ntfy")
}

extern "C" fn target_compat() -> RString {
    RString::from("")
}

extern "C" fn orca_compat() -> RString {
    RString::from(">=0.0.8, <0.1.0")
}

/// Tool-name prefix this plugin owns. The cdylib statically links the toolkit's
/// domain crates (containers / notifications / …), each of which carries its
/// own `#[orca_tool]` inventory entries, so the raw `tool_manifest_json()` walk
/// returns those host-owned tools alongside the plugin's. The plugin exposes
/// only its own `ntfy.*` namespace across the ABI; the host already owns the
/// domain tools and would otherwise reject the manifest as colliding built-ins.
const TOOL_PREFIX: &str = "ntfy.";

/// Reserved tool-name family the notifications `NotifyProxy` calls back through
/// for each registered endpoint: `notify.__backend.<endpoint>.emit`. Mirrors
/// storage's `storage.__backend.<name>.*` seam. Each enabled endpoint row
/// advertises an invoke prefix of `{BACKEND_PREFIX}{name}` in [`backends`].
const BACKEND_PREFIX: &str = "notify.__backend.";

/// The plugin's own tool surface: `tool_manifest_json()` filtered to the
/// `ntfy.*` namespace. Shared by `manifest()` (serialized back out) and
/// `invoke()` (admission check) so both agree on exactly which tools cross.
fn own_tools() -> Vec<ToolDef> {
    let all: Vec<ToolDef> = sj::from_str(&tool_manifest_json()).unwrap_or_default();
    all.into_iter()
        .filter(|d| d.name.starts_with(TOOL_PREFIX))
        .collect()
}

extern "C" fn manifest() -> RString {
    let defs = own_tools();
    RString::from(sj::to_string(&defs).unwrap_or_else(|_| "[]".to_string()))
}

/// Shared multi-thread runtime driving the async tool bodies behind the
/// synchronous FFI `invoke`. Built once on first call and kept for the process
/// lifetime so repeated invocations don't spin a fresh runtime each time.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build plugin tokio runtime")
    })
}

/// A minimal `ToolCtx` for in-cdylib dispatch. The tool surface this plugin
/// exposes (HTTP-only endpoint CRUD + diagnosis) needs no host-injected
/// services, so an empty service registry over a placeholder config suffices;
/// any tool reaching for a service errors cleanly rather than panicking.
fn minimal_ctx() -> ToolCtx {
    let config = Config {
        anthropic_api_key: None,
        lmstudio_url: String::new(),
        ollama_url: String::new(),
        default_model: Model::LMStudio {
            id: String::new(),
            url: String::new(),
        },
        app_dir: std::env::temp_dir(),
        memory_root: std::env::temp_dir(),
        db_path: std::env::temp_dir().join("orca-plugin.db"),
        ports: Ports::default(),
    };
    ToolCtx::new(Arc::new(config))
}

/// The notification backends this plugin contributes: one per enabled ntfy
/// endpoint row, named after the endpoint so the global dispatcher routes
/// `send = ["<endpoint>"]` to it. Enumerated from the db at load time — the
/// static plugin's `bootstrap` read the same table once at startup, and the
/// contract is unchanged: endpoint add/remove applies on next daemon restart.
/// Non-fatal on db error: an empty array degrades to "no backends registered"
/// rather than failing the whole plugin load.
extern "C" fn backends() -> RString {
    let defs: Vec<BackendDef> = enabled_endpoints()
        .into_iter()
        .map(|row| BackendDef {
            domain: "notifications".to_string(),
            name: row.name.clone(),
            kind: String::new(),
            endpoint: row.base_url.clone(),
            capabilities: vec!["emit".to_string()],
            invoke_prefix: format!("{BACKEND_PREFIX}{}", row.name),
        })
        .collect();
    RString::from(sj::to_string(&defs).unwrap_or_else(|_| "[]".to_string()))
}

/// Enabled endpoint rows, or empty on any db error (plugin load must not fail
/// because the notification table is momentarily unreadable).
fn enabled_endpoints() -> Vec<crate::tools::EndpointRow> {
    let Ok(conn) = plugin_toolkit::db::open_default() else {
        return Vec::new();
    };
    match endpoint_db::list(&conn) {
        Ok(rows) => rows.into_iter().filter(|r| r.enabled).collect(),
        Err(_) => Vec::new(),
    }
}

extern "C" fn invoke(name: RStr<'_>, args_json: RStr<'_>) -> RResult<RString, RString> {
    let n = name.as_str();
    // Notification-backend proxy ops: `notify.__backend.<endpoint>.emit`.
    if let Some(rest) = n.strip_prefix(BACKEND_PREFIX) {
        return invoke_backend(rest, args_json.as_str());
    }
    // The plugin's own tool surface (`ntfy.*`).
    if !n.starts_with(TOOL_PREFIX) {
        return RErr(RString::from(format!(
            "tool '{n}' is not in this plugin's '{TOOL_PREFIX}' namespace"
        )));
    }
    let args: sj::Value = match sj::from_str(args_json.as_str()) {
        Ok(v) => v,
        Err(e) => return RErr(RString::from(format!("invalid args JSON: {e}"))),
    };
    let ctx = minimal_ctx();
    let result = runtime().block_on(dispatch(n, args, &ctx));
    match result {
        Ok(value) => match sj::to_string(&value) {
            Ok(s) => ROk(RString::from(s)),
            Err(e) => RErr(RString::from(format!("failed to encode result: {e}"))),
        },
        Err(e) => RErr(RString::from(format!("{e:#}"))),
    }
}

/// Route one notification-backend proxy op. `rest` is `"<endpoint>.<op>"`; the
/// only op is `emit`. The proxy is stateless across the FFI seam, so this
/// rebuilds the endpoint's [`NtfyBackend`] from its db row and drives `emit`.
fn invoke_backend(rest: &str, args_json: &str) -> RResult<RString, RString> {
    let Some((endpoint, op)) = rest.rsplit_once('.') else {
        return RErr(RString::from(format!(
            "malformed backend invoke '{BACKEND_PREFIX}{rest}'"
        )));
    };
    if op != "emit" {
        return RErr(RString::from(format!(
            "ntfy notification backend has no operation '{op}'"
        )));
    }
    let event: Event = match sj::from_str(args_json) {
        Ok(e) => e,
        Err(e) => return RErr(RString::from(format!("invalid emit args: {e}"))),
    };
    let backend = match backend_for(endpoint) {
        Ok(b) => b,
        Err(e) => return RErr(RString::from(e)),
    };
    match runtime().block_on(backend.emit(&event)) {
        Ok(msg) => match sj::to_string(&msg) {
            Ok(s) => ROk(RString::from(s)),
            Err(e) => RErr(RString::from(format!("failed to encode result: {e}"))),
        },
        Err(e) => RErr(RString::from(format!("{e}"))),
    }
}

/// Build the [`NtfyBackend`] for endpoint `name` from its db row. Errors as a
/// plain string (the FFI boundary carries no typed error).
fn backend_for(name: &str) -> Result<NtfyBackend, String> {
    let conn = plugin_toolkit::db::open_default().map_err(|e| format!("db open failed: {e}"))?;
    let row = endpoint_db::get(&conn, name)
        .map_err(|e| format!("load endpoint '{name}': {e}"))?
        .ok_or_else(|| format!("ntfy endpoint '{name}' not registered"))?;
    let mut cfg = crate::Config::new(row.base_url, row.topic);
    if let Some(t) = row.token {
        cfg = cfg.with_token(t);
    }
    Ok(NtfyBackend::new(row.name, crate::Client::new(cfg)))
}

#[export_root_module]
fn export() -> PluginModRef {
    PluginMod {
        plugin_semver,
        target_software,
        target_compat,
        orca_compat,
        manifest,
        invoke,
        backends,
    }
    .leak_into_prefix()
}
