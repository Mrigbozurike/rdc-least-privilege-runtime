//! Windows window focus. Phase 4 implements SetForegroundWindow.
use crate::proto::*;

pub fn focus(_w: &Window) -> Result<()> {
    Err(RdcError::Unsupported("window focus on Windows (phase 4)".into()))
}
