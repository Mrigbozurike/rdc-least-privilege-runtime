//! Install `rdc serve` as a per-user background service.
//!
//! macOS: LaunchAgent (must run in the user's GUI session, so never a LaunchDaemon).
//! Linux: systemd --user unit. Windows: phase 4.

#[cfg(target_os = "macos")]
mod launchd;
#[cfg(target_os = "linux")]
mod systemd;

use anyhow::Result;
use std::path::PathBuf;

#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub const LABEL: &str = "dev.bscott.rdc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Install,
    Uninstall,
    Status,
}

/// Path of the executable the service should run. On macOS prefer the .app bundle copy if
/// this binary lives inside one, so TCC grants are keyed to the bundle.
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub fn service_binary() -> Result<PathBuf> {
    Ok(std::env::current_exe()?.canonicalize()?)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn log_dir() -> PathBuf {
    dirs::state_dir().or_else(dirs::data_local_dir).unwrap_or_else(|| PathBuf::from(".")).join("rdc")
}

pub fn run(op: Op) -> Result<()> {
    #[cfg(target_os = "macos")]
    return launchd::run(op);
    #[cfg(target_os = "linux")]
    return systemd::run(op);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    anyhow::bail!("service management is not implemented on this platform yet (op {op:?})")
}
