//! macOS window focus. Phase 3 replaces this with NSRunningApplication + AXRaise.
use crate::proto::*;

pub fn focus(w: &Window) -> Result<()> {
    // Cheap, dependency-free path for now: activate the owning app by pid.
    let status = std::process::Command::new("osascript")
        .args(["-e", &format!("tell application \"System Events\" to set frontmost of (first process whose unix id is {}) to true", w.pid)])
        .status()
        .map_err(|e| RdcError::Backend(format!("osascript: {e}")))?;
    if status.success() { Ok(()) } else { Err(RdcError::Backend("osascript activate failed".into())) }
}
