#![allow(clippy::disallowed_types)]

use plugin_toolkit::prelude::*;
use plugin_toolkit::process::{Command, Output};

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
async fn run(cmd: Command) -> Result<Output> {
    let output = cmd
        .output()
        .await
        .with_context(|| "failed to spawn command".to_string())?;
    if !output.status.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output
            .status
            .code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        bail!("command failed (exit {}): {}", code, stderr.trim());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_maps_to_image_tag() {
        assert_eq!(Channel::Latest.image_tag(), "latest");
        assert_eq!(Channel::Stable.image_tag(), "stable");
    }
}
