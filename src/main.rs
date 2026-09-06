//! Dynamic (subprocess) entrypoint for the ntfy plugin.
//!
//! ntfy is a HYBRID plugin: it exposes its own `ntfy.*` `#[orca_tool]` surface
//! AND serves one notification endpoint per enabled db row. The unified
//! [`Plugin`](plugin_toolkit::plugin::Plugin) builder advertises both facets
//! from one binary — the `.tools(["ntfy."])` inventory plus the typed
//! `.notify()` facet, which advertises one def per endpoint and routes
//! `notify.__backend.<endpoint>.emit` to the resolved typed
//! [`NtfyBackend`](ntfy::backend::NtfyBackend). The plugin hand-writes no
//! op-string dispatch — the builder emits all wire glue.

plugin_toolkit::instrument::bootstrap!();

use plugin_toolkit::plugin::Plugin;

// Force-link the `ntfy.` #[orca_tool] surface (a separate module from the
// provider referenced below) so its inventory isn't dead-stripped at link time
// (crate-level ref; a submodule `use` trips unused-import under -D warnings).
use ntfy as _;

fn main() -> plugin_toolkit::anyhow::Result<()> {
    Plugin::named("ntfy")
        .version(env!("CARGO_PKG_VERSION"))
        .tools(["ntfy."])
        .notify(ntfy::NtfyProvider)
        .serve()
}
