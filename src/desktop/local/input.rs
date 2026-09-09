//! Input and clipboard run on dedicated OS threads (enigo and arboard handles are not happily
//! shared across threads on every platform). Input has its own thread so a stalled clipboard
//! transfer can never block mouse and keyboard.

use crate::keys::{Chord, KeyName, Modifier, parse_chord};
use crate::proto::*;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tokio::sync::oneshot;

/// Largest scroll magnitude accepted per request, in wheel steps.
pub const MAX_SCROLL_STEPS: i32 = 100;
/// How long a clipboard operation may take before the caller gives up on it.
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(5);

struct Job<Req, Resp> {
    req: Req,
    reply: oneshot::Sender<Resp>,
}

enum InputReq {
    /// Action plus the logical displays (for validation and coordinate mapping).
    Act(InputAction, Vec<Display>),
}

enum ClipReq {
    Get,
    Set(String),
}

#[derive(Clone)]
pub struct InputWorker {
    input: mpsc::Sender<Job<InputReq, Result<()>>>,
    clip: mpsc::Sender<Job<ClipReq, Result<String>>>,
}

impl InputWorker {
    pub fn start() -> Self {
        let (itx, irx) = mpsc::channel();
        thread::Builder::new().name("rdc-input".into()).spawn(move || input_thread(irx)).expect("spawn input thread");
        let (ctx, crx) = mpsc::channel();
        thread::Builder::new()
            .name("rdc-clipboard".into())
            .spawn(move || clipboard_thread(crx))
            .expect("spawn clipboard thread");
        Self { input: itx, clip: ctx }
    }

    pub async fn input(&self, a: InputAction, displays: Vec<Display>) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        if self.input.send(Job { req: InputReq::Act(a, displays), reply }).is_err() {
            return Err(RdcError::Backend("input thread gone".into()));
        }
        rx.await.unwrap_or_else(|_| Err(RdcError::Backend("input thread dropped reply".into())))
    }

    async fn clip(&self, req: ClipReq) -> Result<String> {
        let (reply, rx) = oneshot::channel();
        if self.clip.send(Job { req, reply }).is_err() {
            return Err(RdcError::Backend("clipboard thread gone".into()));
        }
        match tokio::time::timeout(CLIPBOARD_TIMEOUT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(RdcError::Backend("clipboard thread dropped reply".into())),
            Err(_) => Err(RdcError::Backend(format!(
                "clipboard operation timed out after {}s (is the clipboard owner hung?)",
                CLIPBOARD_TIMEOUT.as_secs()
            ))),
        }
    }

    pub async fn clipboard_get(&self) -> Result<String> {
        self.clip(ClipReq::Get).await
    }

    pub async fn clipboard_set(&self, t: String) -> Result<()> {
        self.clip(ClipReq::Set(t)).await.map(|_| ())
    }
}

fn clipboard_thread(rx: mpsc::Receiver<Job<ClipReq, Result<String>>>) {
    let mut clip: Option<arboard::Clipboard> = None;
    for job in rx {
        let c = match &mut clip {
            Some(c) => Ok(c),
            None => match arboard::Clipboard::new() {
                Ok(c) => Ok(clip.insert(c)),
                Err(e) => Err(clip_err(e)),
            },
        };
        let resp = c.and_then(|c| match job.req {
            ClipReq::Get => c.get_text().map_err(clip_err),
            ClipReq::Set(t) => c.set_text(t).map(|_| String::new()).map_err(clip_err),
        });
        let _ = job.reply.send(resp);
    }
}

fn clip_err(e: arboard::Error) -> RdcError {
    RdcError::Backend(format!("clipboard: {e}"))
}

fn input_thread(rx: mpsc::Receiver<Job<InputReq, Result<()>>>) {
    let mut enigo: Option<Enigo> = None;
    for job in rx {
        let InputReq::Act(action, displays) = job.req;
        let resp = (|| {
            let e = match &mut enigo {
                Some(e) => e,
                None => enigo.insert(new_enigo()?),
            };
            let map = CoordMap::new(e, &displays);
            perform(e, &map, action)
        })();
        let _ = job.reply.send(resp);
    }
}

fn new_enigo() -> Result<Enigo> {
    let mut settings = Settings {
        // Never pop a permissions dialog from a headless daemon; `doctor` reports instead.
        open_prompt_to_get_permissions: false,
        release_keys_when_dropped: true,
        ..Settings::default()
    };
    // enigo opens every compiled-in Linux backend it can and broadcasts each event to all of
    // them. Under Wayland that would also drive Xwayland with the wrong coordinate space, so
    // pin exactly one backend by pointing the other at a display that cannot exist.
    if cfg!(target_os = "linux") {
        if on_wayland() {
            settings.x11_display = Some(":9999".into());
        } else {
            settings.wayland_display = Some("rdc-disabled".into());
        }
    }
    Enigo::new(&settings).map_err(|e| match e {
        enigo::NewConError::NoPermission => RdcError::Permission("Accessibility not granted".into()),
        other => RdcError::Backend(format!("enigo init: {other}")),
    })
}

fn on_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn ie(e: enigo::InputError) -> RdcError {
    RdcError::Backend(format!("input: {e}"))
}

fn btn(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

/// Translates validated logical desktop points into whatever enigo's absolute move expects.
///
/// - Wayland: enigo sends `motion_absolute(x, y, extent_w, extent_h)` with the extent being the
///   first output's physical mode; the compositor maps that fraction over the whole layout. We
///   express the point as a fraction of the logical desktop bounds.
/// - X11: xcap reports geometry divided by the Xft.dpi scale, but XTEST wants root pixels, so
///   multiply back by the scale.
/// - macOS and Windows: enigo already speaks logical desktop points.
struct CoordMap {
    bounds: Rect,
    wayland_extent: Option<(i64, i64)>,
    x11_scale: f64,
}

impl CoordMap {
    fn new(e: &Enigo, displays: &[Display]) -> Self {
        let bounds = union_bounds(displays);
        let wayland_extent = if on_wayland() {
            e.main_display().ok().filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| (w as i64, h as i64))
        } else {
            None
        };
        let x11_scale = if cfg!(target_os = "linux") && !on_wayland() {
            displays
                .iter()
                .find(|d| d.primary)
                .or(displays.first())
                .map(|d| d.scale as f64)
                .filter(|s| *s > 0.0)
                .unwrap_or(1.0)
        } else {
            1.0
        };
        Self { bounds, wayland_extent, x11_scale }
    }

    /// Reject points outside the desktop instead of letting a backend guess.
    fn check(&self, x: i64, y: i64) -> Result<()> {
        let b = self.bounds;
        let (x0, y0) = (b.x as i64, b.y as i64);
        let (x1, y1) = (x0 + b.w as i64, y0 + b.h as i64);
        if b.w == 0 || b.h == 0 {
            return Err(RdcError::Backend("no displays; cannot map coordinates".into()));
        }
        if x < x0 || y < y0 || x > x1 || y > y1 {
            return Err(RdcError::BadRequest(format!(
                "point ({x}, {y}) is outside the desktop ({x0}..={x1}, {y0}..={y1})"
            )));
        }
        Ok(())
    }

    fn map(&self, x: i64, y: i64) -> Result<(i32, i32)> {
        self.check(x, y)?;
        let (mx, my) = match self.wayland_extent {
            Some((ew, eh)) => {
                let fx = (x - self.bounds.x as i64) as f64 / self.bounds.w as f64;
                let fy = (y - self.bounds.y as i64) as f64 / self.bounds.h as f64;
                ((fx * ew as f64).round(), (fy * eh as f64).round())
            }
            None => (x as f64 * self.x11_scale, y as f64 * self.x11_scale),
        };
        Ok((mx.round() as i32, my.round() as i32))
    }
}

fn union_bounds(displays: &[Display]) -> Rect {
    let mut it = displays.iter();
    let first = match it.next() {
        Some(d) => d.rect,
        None => return Rect { x: 0, y: 0, w: 0, h: 0 },
    };
    it.fold(first, |acc, d| acc.union(&d.rect))
}

fn move_abs(e: &mut Enigo, m: &CoordMap, x: i64, y: i64) -> Result<()> {
    let (mx, my) = m.map(x, y)?;
    e.move_mouse(mx, my, Coordinate::Abs).map_err(ie)
}

fn perform(e: &mut Enigo, m: &CoordMap, a: InputAction) -> Result<()> {
    match a {
        InputAction::MouseMove { x, y } => move_abs(e, m, x as i64, y as i64),
        InputAction::Click { x, y, button, count } => {
            move_abs(e, m, x as i64, y as i64)?;
            thread::sleep(Duration::from_millis(20));
            for i in 0..count.clamp(1, 3) {
                if i > 0 {
                    thread::sleep(Duration::from_millis(60));
                }
                e.button(btn(button), Direction::Click).map_err(ie)?;
            }
            Ok(())
        }
        InputAction::Button { button, down } => {
            e.button(btn(button), if down { Direction::Press } else { Direction::Release }).map_err(ie)
        }
        InputAction::Drag { from, to, button } => {
            let (fx, fy) = (from.0 as i64, from.1 as i64);
            let (tx, ty) = (to.0 as i64, to.1 as i64);
            // Validate both ends before touching anything.
            m.check(fx, fy)?;
            m.check(tx, ty)?;
            move_abs(e, m, fx, fy)?;
            thread::sleep(Duration::from_millis(30));
            e.button(btn(button), Direction::Press).map_err(ie)?;
            let steps = 12;
            let mut result = Ok(());
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                let x = fx as f64 + (tx - fx) as f64 * t;
                let y = fy as f64 + (ty - fy) as f64 * t;
                if let Err(err) = move_abs(e, m, x.round() as i64, y.round() as i64) {
                    result = Err(err);
                    break;
                }
                thread::sleep(Duration::from_millis(12));
            }
            thread::sleep(Duration::from_millis(30));
            // Always release, even if a move failed, so the desktop is never left mid-drag.
            let released = e.button(btn(button), Direction::Release).map_err(ie);
            result.and(released)
        }
        InputAction::Scroll { at, dx, dy } => {
            if dx.abs() > MAX_SCROLL_STEPS || dy.abs() > MAX_SCROLL_STEPS {
                return Err(RdcError::BadRequest(format!(
                    "scroll steps must be within ±{MAX_SCROLL_STEPS} (got dx={dx}, dy={dy})"
                )));
            }
            if let Some((x, y)) = at {
                move_abs(e, m, x as i64, y as i64)?;
                thread::sleep(Duration::from_millis(20));
            }
            if dy != 0 {
                e.scroll(dy, Axis::Vertical).map_err(ie)?;
            }
            if dx != 0 {
                e.scroll(dx, Axis::Horizontal).map_err(ie)?;
            }
            Ok(())
        }
        InputAction::Type { text } => e.text(&text).map_err(ie),
        InputAction::Key { chord } => press_chord(e, &parse_chord(&chord)?),
    }
}

fn modifier_key(m: Modifier) -> Key {
    match m {
        Modifier::Meta => Key::Meta,
        Modifier::Control => Key::Control,
        Modifier::Alt => Key::Alt,
        Modifier::Shift => Key::Shift,
    }
}

fn key_for(k: &KeyName) -> Result<Key> {
    Ok(match k {
        KeyName::Return => Key::Return,
        KeyName::Escape => Key::Escape,
        KeyName::Tab => Key::Tab,
        KeyName::Space => Key::Space,
        KeyName::Backspace => Key::Backspace,
        KeyName::Delete => Key::Delete,
        #[cfg(not(target_os = "macos"))]
        KeyName::Insert => Key::Insert,
        // Macs have no Insert key; kVK_Help (0x72) is the closest physical equivalent.
        #[cfg(target_os = "macos")]
        KeyName::Insert => Key::Other(0x72),
        KeyName::Home => Key::Home,
        KeyName::End => Key::End,
        KeyName::PageUp => Key::PageUp,
        KeyName::PageDown => Key::PageDown,
        KeyName::Up => Key::UpArrow,
        KeyName::Down => Key::DownArrow,
        KeyName::Left => Key::LeftArrow,
        KeyName::Right => Key::RightArrow,
        KeyName::CapsLock => Key::CapsLock,
        KeyName::F(n) => function_key(*n)?,
        KeyName::Mod(m) => modifier_key(*m),
        KeyName::Char(c) => Key::Unicode(*c),
    })
}

fn function_key(n: u8) -> Result<Key> {
    Ok(match n {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        #[cfg(not(target_os = "macos"))]
        21 => Key::F21,
        #[cfg(not(target_os = "macos"))]
        22 => Key::F22,
        #[cfg(not(target_os = "macos"))]
        23 => Key::F23,
        #[cfg(not(target_os = "macos"))]
        24 => Key::F24,
        // macOS defines no virtual keycodes above F20; refuse rather than press a different key.
        other => return Err(RdcError::Unsupported(format!("F{other} is not available on this platform"))),
    })
}

fn press_chord(e: &mut Enigo, c: &Chord) -> Result<()> {
    // Resolve the key first so an unsupported key never leaves modifiers half-pressed.
    let key = key_for(&c.key)?;
    for m in &c.mods {
        e.key(modifier_key(*m), Direction::Press).map_err(ie)?;
    }
    let r = e.key(key, Direction::Click).map_err(ie);
    for m in c.mods.iter().rev() {
        // Always release, even if the key press failed.
        let _ = e.key(modifier_key(*m), Direction::Release);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn displays() -> Vec<Display> {
        vec![
            Display { id: 0, name: "a".into(), rect: Rect { x: 0, y: 0, w: 1440, h: 900 }, scale: 2.0, primary: true },
            Display {
                id: 1,
                name: "b".into(),
                rect: Rect { x: -1920, y: 0, w: 1920, h: 1080 },
                scale: 1.0,
                primary: false,
            },
        ]
    }

    #[test]
    fn union_spans_negative_origin() {
        let b = union_bounds(&displays());
        assert_eq!(b, Rect { x: -1920, y: 0, w: 3360, h: 1080 });
    }

    #[test]
    fn check_rejects_outside_and_accepts_edges() {
        let m = CoordMap { bounds: union_bounds(&displays()), wayland_extent: None, x11_scale: 1.0 };
        assert!(m.check(-1920, 0).is_ok());
        assert!(m.check(1440, 1080).is_ok());
        assert!(m.check(1441, 0).is_err());
        assert!(m.check(0, -1).is_err());
        assert!(m.check(i32::MIN as i64, 0).is_err());
        assert!(m.check(i32::MAX as i64, i32::MAX as i64).is_err());
    }

    #[test]
    fn wayland_maps_to_extent_fraction() {
        let m = CoordMap {
            bounds: Rect { x: 0, y: 0, w: 1440, h: 960 },
            wayland_extent: Some((2880, 1920)),
            x11_scale: 1.0,
        };
        assert_eq!(m.map(720, 480).unwrap(), (1440, 960));
        assert_eq!(m.map(0, 0).unwrap(), (0, 0));
    }

    #[test]
    fn x11_multiplies_by_scale() {
        let m = CoordMap { bounds: Rect { x: 0, y: 0, w: 1920, h: 1080 }, wayland_extent: None, x11_scale: 2.0 };
        assert_eq!(m.map(960, 540).unwrap(), (1920, 1080));
    }
}
