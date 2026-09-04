//! Platform permission checks and one-time prompts (macOS TCC).

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    pub fn screen_recording() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    /// Triggers the system prompt (or a Settings deep link) if not yet granted.
    pub fn request_screen_recording() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }
    pub fn accessibility() -> bool {
        unsafe { AXIsProcessTrusted() }
    }
    /// enigo can pop the Accessibility prompt for us.
    pub fn request_accessibility() -> bool {
        let settings = enigo::Settings { open_prompt_to_get_permissions: true, ..Default::default() };
        enigo::Enigo::new(&settings).is_ok() && accessibility()
    }
}

/// (name, granted) pairs relevant on this platform.
pub fn check() -> Vec<(&'static str, bool)> {
    #[cfg(target_os = "macos")]
    {
        vec![("screen recording", mac::screen_recording()), ("accessibility", mac::accessibility())]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![]
    }
}

/// Ask the OS for anything missing. Returns the post-request state.
pub fn request() -> Vec<(&'static str, bool)> {
    #[cfg(target_os = "macos")]
    {
        let sr = mac::screen_recording() || mac::request_screen_recording();
        let ax = mac::accessibility() || mac::request_accessibility();
        vec![("screen recording", sr), ("accessibility", ax)]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![]
    }
}
