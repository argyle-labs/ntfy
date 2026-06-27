# ntfy

An [orca](https://github.com/argyle-labs/orca) plugin for [ntfy](https://ntfy.sh)
— the self-hostable, send-an-HTTP-POST push-notification service.

This repo is two halves of one thing: the **orca plugin** (an ABI-stable
cdylib orca loads at runtime) and the **deploy lifecycle** (Dockerfile, Compose,
and `scripts/`) for running your own ntfy server.

## What the plugin owns

- A small HTTP client (`Client` + `Message`) that POSTs to an ntfy topic over
  the toolkit's HTTP gateway.
- A `notifications::Backend` impl so the orca notifications domain can route
  operator events (heartbeats, drift, alerts, approvals) to ntfy.
- An endpoint registry: `ntfy.{list,detail,create,update,delete}` — generated
  by `endpoint_resource!`, persisting `(name, base_url, topic, token, enabled)`.
- `ntfy.send` — fire a raw message through a registered endpoint (smoke test).
- Deploy lifecycle tools: `ntfy.{install,upgrade,backup,restore}`.

`notifications` knows nothing about ntfy. Adding email/Slack/etc. follows the
same shape: own crate, own table, own backend impl, own bootstrap.

## Tool surface

| Tool | What it does |
|---|---|
| `ntfy.create` / `ntfy.update` / `ntfy.delete` | Manage registered endpoints. |
| `ntfy.list` / `ntfy.detail` | Inspect registered endpoints. |
| `ntfy.send` | POST a raw message to a registered endpoint. |
| `ntfy.install` | `docker compose up -d` the bundled stack. |
| `ntfy.upgrade` | Pull the channel image tag and recreate the container. |
| `ntfy.backup` | Tar the state tree (`/etc/ntfy` + message db) to a `.tar.gz`. |
| `ntfy.restore` | Extract a backup tarball over the state tree. |

> The lifecycle verb is `ntfy.upgrade` (not `ntfy.update`) because the endpoint
> registry already owns `ntfy.update` for editing a registered endpoint row.

## Deploy your own ntfy server

ntfy ships an official image (`binwiederhier/ntfy`); this repo wraps it with the
backup/restore helpers and a known-good Compose file.

```sh
# bring it up (persistent state under ./state)
docker compose -f compose.yml up -d

# or via the orca tool
orca tool ntfy.install --compose-file compose.yml
```

Then register the endpoint with orca so notifications can route to it:

```sh
orca tool ntfy.create \
  --name home \
  --base-url http://127.0.0.1:80 \
  --topic homelab \
  --enabled true
# add --token <tok> for an access-controlled server
```

Compose examples live in [`examples/`](examples): `docker-compose.basic.yml`
(anonymous) and `docker-compose.auth.yml` (deny-by-default + token auth).

## Podman

ntfy has no special host requirements, so the same [`compose.yml`](compose.yml)
runs under rootless Podman:

```sh
podman compose -f compose.yml up -d   # or: podman-compose -f compose.yml up -d
```

## Proxmox LXC

There are no native-LXC assets for ntfy — it ships as the official upstream
image. To run it on Proxmox, create an unprivileged Debian/Ubuntu LXC, install
Docker or Podman inside it, and use the Docker / Compose or Podman path above
(Docker-in-LXC). Persistent state stays under `./state` as documented above.

## Backup & Restore

Two equivalent paths — the orca tools and the shell scripts archive the same
state tree (config + message/auth db) as a single `.tar.gz`.

```sh
# orca tools
orca tool ntfy.backup  --state-path /opt/ntfy --destination /opt/backups
orca tool ntfy.restore --from /opt/backups/ntfy-state-20260101-000000.tar.gz --state-path /opt/ntfy

# shell scripts
./scripts/backup.sh  /opt/ntfy /opt/backups
./scripts/restore.sh /opt/backups/ntfy-state-20260101-000000.tar.gz /opt/ntfy
```

---

## orca plugin

### Build

```sh
# With an orca checkout at ../orca, the committed .cargo/config.toml patch
# resolves plugin-toolkit locally; otherwise it resolves from the pinned rc tag.
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

### The two-dependency rule

A compliant orca plugin's `[dependencies]` is **exactly two crates**:

| Dep | Why it is allowed |
|---|---|
| `plugin-toolkit` | The single orca gateway. Every other crate the plugin would reach for — serde, serde_json, schemars, clap, thiserror, chrono, uuid, reqwest, anyhow, async_trait, tokio, tracing, **urlencoding** — is re-exported through `plugin_toolkit::*` / its prelude, or injected by the `#[plugin_struct]` / `#[orca_tool]` / `endpoint_resource!` macros. The plugin source names **no** external crate through this dep. |
| `abi_stable` | **The one genuine non-toolkit dependency, and it cannot be removed.** See below. |

Everything else (the in-crate tests' deps — `tokio` / `wiremock` / `tempfile`)
lives under `[dev-dependencies]` and is outside the rule: dev-deps never ship in
the cdylib.

This plugin is fully hand-written (no codegen): there is no `build.rs`, no
`specs/`, and no `plugin-toolkit-build` dependency. URL-encoding of the topic
goes through `plugin_toolkit::urlencoding`, not a direct `urlencoding` dep.

### Why `abi_stable` is the unavoidable exception

orca loads external plugins as **cdylibs it `dlopen`s at runtime** — not as
statically linked crates. That crossing is a C-ABI FFI boundary, and the data
that crosses it (the root module, the version header, the layout hashes the
loader checks before it trusts the `.so`) must have a **guaranteed, stable memory
layout**. Rust's native `repr(Rust)` gives no such guarantee across independent
compilations, so the boundary types come from `abi_stable` (`RString`, `RStr`,
`RResult`, `PrefixTypeTrait`, …).

The decisive detail: `#[export_root_module]` — the attribute that emits the
single symbol orca's loader looks up — **expands to bare `::abi_stable::*` paths
in this crate's own root.** There is no source path for the toolkit to redirect
and no `crate =` attribute to retarget; the macro hard-codes the crate name into
generated code that lives *in the plugin*. So unlike serde/reqwest (whose paths
route through `::plugin_toolkit::*`), `abi_stable` genuinely must be a direct dep.

It is pinned to **the same `abi_stable` version the toolkit uses** (`0.11`) so the
layout hash baked into the cdylib matches what orca's `plugin-loader` validates at
load time. A version skew here is not a compile error — it is a load-time
rejection. Keep it in lockstep with the toolkit.

The whole abi boundary is isolated to one file,
[`src/abi_export.rs`](src/abi_export.rs): the only place `abi_stable` is named,
the only place the JSON dispatch payload type is aliased, and the only place the
`disallowed_types` lint is suppressed.

### Authoring a fresh plugin from this template

This repo is the canonical template for a **hand-written (non-codegen)** orca
plugin. To start a new `<name>` plugin:

1. **Scaffold the crate.** Copy this repo's skeleton: `Cargo.toml`,
   `.cargo/config.toml`, `src/abi_export.rs`, and a `src/` tree (`lib.rs`,
   `tools.rs`, plus `backend.rs` / `lifecycle.rs` as the surface needs). Keep
   `[lib] crate-type = ["cdylib", "rlib"]` — `cdylib` is the artifact orca
   loads; `rlib` keeps the in-crate test harness.

2. **Set `[dependencies]` to the two allowed crates** — `plugin-toolkit` (git
   dep on the orca rc tag) and `abi_stable = "0.11"` — nothing else. Put test
   tooling under `[dev-dependencies]`.

3. **Write the surface against the toolkit only.** `use plugin_toolkit::prelude::*;`
   for the common surface; reach `plugin_toolkit::http`, `plugin_toolkit::chrono`,
   `plugin_toolkit::urlencoding`, `plugin_toolkit::serde_json`, etc. explicitly
   where the prelude doesn't cover it. Derive on hand-written types via
   `#[plugin_struct]`. **Do not** add a `thiserror` dep — hand-roll `Display` +
   `std::error::Error` + `From` as `NtfyError` does in [`src/lib.rs`](src/lib.rs);
   `?` conversion rides anyhow's blanket `From`.

4. **Update `abi_export.rs` metadata** — change `target_software`,
   `target_compat`, `orca_compat`, and `TOOL_PREFIX` to your `<name>.`
   namespace. Leave the rest of the FFI plumbing as-is.

5. **Prove the rule holds** before committing:
   ```sh
   cargo build && cargo clippy --all-targets -- -D warnings && cargo test
   cargo tree -e normal --depth 1   # MUST show only plugin-toolkit + abi_stable
   ```
   Any third crate under `[dependencies]` is a toolkit gap — file it against
   `plugin-toolkit` and route through it rather than adding the dep here.

## Tags

The wrapper image is published to `ghcr.io/scottdkey/ntfy:latest`. The plugin's
`plugin-toolkit` git dep is pinned to an orca rc tag in `Cargo.toml`; bump it
when a newer toolkit surface is required.
