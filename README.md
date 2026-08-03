<p align="center">
  <img src="assets/icon-256.png" width="120" alt="ntfy" />
</p>

# ntfy

ntfy is a simple HTTP-based pub-sub notification service you can self-host.

A first-party [orca](https://github.com/argyle-labs/orca) plugin (service-backend).

This repo **ships a `compose.yml`** — run ntfy **by hand, without orca** straight from it:

---

## Run it without orca

### Docker Compose

```sh
docker compose up -d
```

See [`compose.yml`](compose.yml) for the image, ports, volumes, and hardware/device mappings and `scripts/` for provisioning helpers.

### Other runtimes

**Podman**: `podman compose -f compose.yml up -d`. **LXC / VM**: run the same image via Docker/Podman on the guest, or install upstream. **Unraid**: *Docker → Add Container* with the image/ports/volumes from `compose.yml`. Upstream: <https://ntfy.sh/>.


### Backup & restore

Back up the config/data volume(s) above — that's the whole service state (stop the container first for a clean copy). Restore by putting them back and starting it.

> With orca this is **`service.backup` / `service.restore`** — location-agnostic (docker / podman / lxc / vm), one command regardless of where ntfy runs. No per-service backup script.

## With orca

orca drives this plugin through the single generic `service.*` surface — no per-plugin tools:

```sh
orca service.deploy ntfy      # render + launch on any supported runtime
orca service.status ntfy      # health + rich diagnostics (typed payload)
orca service.backup ntfy      # location-agnostic backup (tar; PBS on Proxmox)
orca service.configure ntfy   # apply config via the upstream API
```

## Layout

- `src/backend.rs` — the notifications backend: renders/deploys the compose stack and reports typed `status` diagnostics.
- `src/lifecycle.rs` — install / upgrade / backup / restore lifecycle for the ntfy container and its `./state` tree.
- `src/tools.rs` — the plugin's tool surface for registering ntfy endpoints and sending raw messages through them.
- `src/lib.rs` / `src/main.rs` — plugin registration and out-of-process entrypoint.
- `compose.yml` — standalone deployment.
- `scripts/` — provisioning / lifecycle helpers.
- `assets/` — plugin icon.
