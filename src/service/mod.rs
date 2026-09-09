//! Install `rdc serve` as a per-user background service.
//!
//! macOS: LaunchAgent (must run in the user's GUI session, so never a LaunchDaemon).
//! Linux: systemd --user unit. Windows: Task Scheduler logon task in the interactive session.

#[cfg(target_os = "macos")]
mod launchd;
#[cfg(target_os = "linux")]
mod systemd;
#[cfg(target_os = "windows")]
mod windows;

use anyhow::Result;
use std::path::PathBuf;

pub const LABEL: &str = "dev.rdc.daemon";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Install,
    Uninstall,
    Status,
}

/// Path of the executable the service should run. On macOS prefer the .app bundle copy if
/// this binary lives inside one, so TCC grants are keyed to the bundle.
pub fn service_binary() -> Result<PathBuf> {
    Ok(std::env::current_exe()?.canonicalize()?)
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn log_dir() -> PathBuf {
    crate::config::state_dir()
}

pub fn run(op: Op) -> Result<()> {
    #[cfg(target_os = "macos")]
    return launchd::run(op);
    #[cfg(target_os = "linux")]
    return systemd::run(op);
    #[cfg(target_os = "windows")]
    return windows::run(op);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    anyhow::bail!("service management is not implemented on this platform yet (op {op:?})")
}
