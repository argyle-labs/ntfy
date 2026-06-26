//! ntfy server deployment lifecycle tool surface.
//!
//! Net-new over the notification-send surface: these `#[orca_tool]`s own the
//! full deploy lifecycle of a self-hosted ntfy server — provision, version
//! bump, and config backup/restore — driving the host's Docker/Compose runtime
//! and `tar` for the `/etc/ntfy` + `/var/cache/ntfy` volumes through
//! `tokio::process::Command`. There is no parallel shell glue: the bootstrap
//! scripts in `scripts/` are the curl-bootstrap payload these tools
//! orchestrate, and every capability is reachable as an orca tool.
//!
//! ntfy is deployed as the official `binwiederhier/ntfy` Docker image. There is
//! no LXC-native package path the way a media server has, so the runtime here is
//! Docker/Compose only — kept honest rather than fabricating an `apt` path.
//!
//! Imports flow through `plugin_toolkit::prelude::*` only — the toolkit is the
//! single gateway. Process exec uses the toolkit's re-exported `tokio`.
#![allow(clippy::disallowed_types)]

use std::path::Path;
use std::process::Output;

use plugin_toolkit::prelude::*;
use plugin_toolkit::tokio::process::Command;

/// Release channel for `ntfy.upgrade`. Maps to an image tag on
/// `binwiederhier/ntfy`.
#[derive(
    Clone,
    Copy,
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
    plugin_toolkit::clap::ValueEnum,
    Default,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Newest published ntfy release.
    #[default]
    Latest,
    /// Pinned stable line (ntfy publishes `stable` as an alias for latest GA).
    Stable,
}

impl Channel {
    /// The container image tag this channel resolves to.
    fn image_tag(self) -> &'static str {
        match self {
            Channel::Latest => "latest",
            Channel::Stable => "stable",
        }
    }
}

/// Run a command, capturing output, and map a non-zero exit to an error that
/// carries stderr — the lifecycle tools surface the runtime's own message
/// rather than a bare exit code.
async fn run(cmd: &mut Command) -> Result<Output> {
    let output = cmd
        .output()
        .await
        .with_context(|| "failed to spawn command".to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("command failed ({}): {}", output.status, stderr.trim());
    }
    Ok(output)
}

// ═══════════════════════════════════════════════════════════════════════════
// ntfy.install — provision a Compose deployment
// ═══════════════════════════════════════════════════════════════════════════

#[derive(
    plugin_toolkit::clap::Args,
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
pub struct NtfyInstallArgs {
    /// Compose file to bring up. Defaults to the repo-relative `compose.yml`.
    #[arg(long, default_value = "compose.yml")]
    #[serde(default = "default_compose")]
    pub compose_file: String,
}

#[derive(
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct NtfyInstallOutput {
    /// True when the provisioning command completed successfully.
    pub provisioned: bool,
    /// Combined stdout from the provisioning step.
    pub log: String,
}

/// **Provision a self-hosted ntfy server** by bringing up the Compose stack.
/// The bundled `compose.yml` mounts `/etc/ntfy` (config) + `/var/cache/ntfy`
/// (message db) as persistent volumes and exposes port 80.
#[orca_tool(domain = "ntfy", verb = "install")]
async fn ntfy_install(args: NtfyInstallArgs, _ctx: &ToolCtx) -> Result<NtfyInstallOutput> {
    let output = run(Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(&args.compose_file)
        .arg("up")
        .arg("-d"))
    .await?;
    Ok(NtfyInstallOutput {
        provisioned: true,
        log: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

fn default_compose() -> String {
    "compose.yml".to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// ntfy.update — channel-aware image bump
// ═══════════════════════════════════════════════════════════════════════════

#[derive(
    plugin_toolkit::clap::Args,
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
pub struct NtfyUpdateArgs {
    /// Release channel to move to. `latest` / `stable`.
    #[arg(long, value_enum, default_value_t = Channel::Latest)]
    #[serde(default)]
    pub channel: Channel,
    /// Compose file to recreate the container from.
    #[arg(long, default_value = "compose.yml")]
    #[serde(default = "default_compose")]
    pub compose_file: String,
}

#[derive(
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct NtfyUpdateOutput {
    /// True when the update command completed.
    pub updated: bool,
    /// Image tag the channel resolved to.
    pub image_tag: String,
    /// Combined stdout from the update step.
    pub log: String,
}

/// **Update a self-hosted ntfy server** to the head of a release channel:
/// re-pulls the channel image tag and recreates the container from Compose.
#[orca_tool(domain = "ntfy", verb = "upgrade")]
async fn ntfy_update(args: NtfyUpdateArgs, _ctx: &ToolCtx) -> Result<NtfyUpdateOutput> {
    let tag = args.channel.image_tag();
    let image = format!("binwiederhier/ntfy:{tag}");
    run(Command::new("docker").arg("pull").arg(&image)).await?;
    let output = run(Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(&args.compose_file)
        .arg("up")
        .arg("-d"))
    .await?;
    Ok(NtfyUpdateOutput {
        updated: true,
        image_tag: tag.to_string(),
        log: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// ntfy.backup — tar the config + message db to a destination
// ═══════════════════════════════════════════════════════════════════════════

#[derive(
    plugin_toolkit::clap::Args,
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
pub struct NtfyBackupArgs {
    /// Host path of the ntfy config + db tree to archive (the volume mounted at
    /// `/var/cache/ntfy` + `/etc/ntfy` on the container).
    #[arg(long, default_value = "/opt/ntfy")]
    #[serde(default = "default_state_path")]
    pub state_path: String,
    /// Directory to write the `.tar.gz` into. Created if missing.
    #[arg(long)]
    pub destination: String,
}

fn default_state_path() -> String {
    "/opt/ntfy".to_string()
}

#[derive(
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct NtfyBackupOutput {
    /// Absolute path of the archive written.
    pub archive: String,
}

/// **Back up the ntfy server state** (config + message db) to a `.tar.gz` in
/// the destination directory.
#[orca_tool(domain = "ntfy", verb = "backup")]
async fn ntfy_backup(args: NtfyBackupArgs, _ctx: &ToolCtx) -> Result<NtfyBackupOutput> {
    backup_state(&args).await
}

/// Archive logic, independent of the tool context so it is directly testable.
async fn backup_state(args: &NtfyBackupArgs) -> Result<NtfyBackupOutput> {
    let state = Path::new(&args.state_path);
    if !state.is_dir() {
        bail!("state path '{}' is not a directory", args.state_path);
    }
    run(Command::new("mkdir").arg("-p").arg(&args.destination)).await?;

    let stamp = now_stamp();
    let archive = format!(
        "{}/ntfy-state-{}.tar.gz",
        args.destination.trim_end_matches('/'),
        stamp
    );

    run(Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&args.state_path)
        .arg("."))
    .await?;

    Ok(NtfyBackupOutput { archive })
}

// ═══════════════════════════════════════════════════════════════════════════
// ntfy.restore — restore the state tree from a tarball
// ═══════════════════════════════════════════════════════════════════════════

#[derive(
    plugin_toolkit::clap::Args,
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
pub struct NtfyRestoreArgs {
    /// The backup tarball to restore from.
    #[arg(long = "from")]
    pub from: String,
    /// Host path of the state tree to restore into. Created if missing.
    #[arg(long, default_value = "/opt/ntfy")]
    #[serde(default = "default_state_path")]
    pub state_path: String,
}

#[derive(
    plugin_toolkit::serde::Serialize,
    plugin_toolkit::serde::Deserialize,
    plugin_toolkit::schemars::JsonSchema,
)]
#[serde(crate = "plugin_toolkit::serde")]
#[schemars(crate = "plugin_toolkit::schemars")]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct NtfyRestoreOutput {
    /// True when extraction completed.
    pub restored: bool,
    /// Where the state was restored to.
    pub state_path: String,
}

/// **Restore the ntfy server state** from a `.tar.gz` produced by
/// `ntfy.backup`. Stop the container before restoring; this tool only extracts
/// the archive over the state directory.
#[orca_tool(domain = "ntfy", verb = "restore")]
async fn ntfy_restore(args: NtfyRestoreArgs, _ctx: &ToolCtx) -> Result<NtfyRestoreOutput> {
    restore_state(args).await
}

/// Extraction logic, independent of the tool context so it is directly testable.
async fn restore_state(args: NtfyRestoreArgs) -> Result<NtfyRestoreOutput> {
    if !Path::new(&args.from).is_file() {
        bail!("backup tarball '{}' not found", args.from);
    }
    run(Command::new("mkdir").arg("-p").arg(&args.state_path)).await?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&args.from)
        .arg("-C")
        .arg(&args.state_path))
    .await?;
    Ok(NtfyRestoreOutput {
        restored: true,
        state_path: args.state_path,
    })
}

/// UTC timestamp `YYYYMMDD-HHMMSS` for archive names. chrono is reached through
/// the toolkit re-export so the plugin carries no direct chrono dep.
fn now_stamp() -> String {
    plugin_toolkit::chrono::Utc::now()
        .format("%Y%m%d-%H%M%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_maps_to_image_tag() {
        assert_eq!(Channel::Latest.image_tag(), "latest");
        assert_eq!(Channel::Stable.image_tag(), "stable");
    }

    #[tokio::test]
    async fn backup_rejects_missing_state_dir() {
        let args = NtfyBackupArgs {
            state_path: "/nonexistent/ntfy/state/path".to_string(),
            destination: "/tmp/ntfy-test-dest".to_string(),
        };
        let err = backup_state(&args).await.unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    #[tokio::test]
    async fn restore_rejects_missing_tarball() {
        let args = NtfyRestoreArgs {
            from: "/nonexistent/backup.tar.gz".to_string(),
            state_path: "/tmp/ntfy-test-restore".to_string(),
        };
        let err = restore_state(args).await.unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn backup_then_restore_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        std::fs::create_dir_all(state.join("etc")).unwrap();
        std::fs::write(state.join("etc").join("server.yml"), b"base-url: x").unwrap();
        std::fs::write(state.join("cache.db"), b"sqlite").unwrap();

        let dest = tmp.path().join("backups");
        let out = backup_state(&NtfyBackupArgs {
            state_path: state.to_string_lossy().into_owned(),
            destination: dest.to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
        assert!(Path::new(&out.archive).is_file());

        let restore_target = tmp.path().join("restored");
        restore_state(NtfyRestoreArgs {
            from: out.archive.clone(),
            state_path: restore_target.to_string_lossy().into_owned(),
        })
        .await
        .unwrap();

        assert!(restore_target.join("etc").join("server.yml").is_file());
        assert!(restore_target.join("cache.db").is_file());
    }
}
