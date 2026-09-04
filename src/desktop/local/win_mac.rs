//! macOS window focus: activate the owning application via AppKit. Raising one specific
//! window of a multi-window app would need the Accessibility API (AXRaise); activating the
//! app brings its key window forward, which covers the dialog-clicking use case.
use crate::proto::*;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

pub fn focus(w: &Window) -> Result<()> {
    let pid = w.pid as i32;
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .ok_or_else(|| RdcError::NotFound(format!("no running application with pid {pid}")))?;
    #[allow(deprecated)]
    let ok = app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    if ok { Ok(()) } else { Err(RdcError::Backend(format!("macOS refused to activate pid {pid}"))) }
}
