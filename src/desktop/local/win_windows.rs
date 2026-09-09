//! Windows-specific pieces: absolute pointer positioning across the whole virtual desktop and
//! window focus. enigo's absolute move normalises against the primary monitor only, so on a
//! multi-monitor desktop it cannot reach secondary displays; we send the input ourselves.

use crate::proto::*;
use std::ffi::c_void;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, IsIconic, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

/// The virtual desktop rectangle in physical pixels (all monitors), as Windows reports it.
pub fn virtual_screen() -> Rect {
    // SAFETY: GetSystemMetrics has no preconditions.
    unsafe {
        Rect {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            w: GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) as u32,
            h: GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) as u32,
        }
    }
}

/// Move the pointer to an absolute position in virtual-desktop pixels.
///
/// With `MOUSEEVENTF_VIRTUALDESK`, `dx`/`dy` are normalised so that 0 and 65535 map to the
/// edges of the whole virtual screen, not the primary monitor. See the `mouse_event` remarks in
/// the Win32 docs for the rounding used here.
pub fn move_absolute(x: i32, y: i32) -> Result<()> {
    let vs = virtual_screen();
    let norm = |v: i32, origin: i32, extent: u32| -> i32 {
        let span = (extent as i64 - 1).max(1);
        let rel = (v as i64 - origin as i64).clamp(0, span);
        ((rel * 65535 + span / 2) / span) as i32
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: norm(x, vs.x, vs.w),
                dy: norm(y, vs.y, vs.h),
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: `input` is a fully initialised INPUT and the size matches.
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err(RdcError::Backend(format!(
            "SendInput moved 0 of 1 events (blocked by UIPI or another session?): {}",
            windows::core::Error::from_thread()
        )))
    }
}

fn hwnd(id: u64) -> HWND {
    HWND(id as usize as *mut c_void)
}

/// Bring a window to the foreground.
///
/// Windows only lets the process that currently owns the foreground (or one it has handed the
/// right to) call `SetForegroundWindow` successfully. Attaching our thread's input queue to the
/// current foreground window's thread is the long-standing way to be allowed.
pub fn focus(w: &Window) -> Result<()> {
    let target = hwnd(w.id);
    // SAFETY: plain Win32 calls on a window handle we got from enumeration; all of them tolerate
    // a stale handle by failing.
    unsafe {
        if IsIconic(target).as_bool() {
            let _ = ShowWindow(target, SW_RESTORE);
        }
        let fg = GetForegroundWindow();
        let mut fg_pid = 0u32;
        let fg_thread = if fg.0.is_null() { 0 } else { GetWindowThreadProcessId(fg, Some(&mut fg_pid)) };
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(target);
        let mut ok = SetForegroundWindow(target).as_bool();
        if !ok {
            // Windows grants foreground rights to the process that most recently sent input.
            // A bare Alt press and release is the long-standing, harmless way to earn them.
            tap_alt();
            let _ = BringWindowToTop(target);
            ok = SetForegroundWindow(target).as_bool();
        }
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
        if ok || GetForegroundWindow() == target {
            Ok(())
        } else {
            Err(RdcError::Backend(format!("Windows refused to bring {} ({}) to the foreground", w.app, w.title)))
        }
    }
}

fn key_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_MENU, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

/// Press and release Alt without anything else, which does not trigger a menu.
fn tap_alt() {
    let events = [key_input(Default::default()), key_input(KEYEVENTF_KEYUP)];
    // SAFETY: fully initialised INPUT array, correct size.
    unsafe {
        SendInput(&events, std::mem::size_of::<INPUT>() as i32);
    }
}
