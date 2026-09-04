//! A dedicated OS thread owns the enigo and arboard handles (neither is happily shared
//! across threads on every platform) and serves requests over a channel.

use crate::keys::{Chord, KeyName, Modifier, parse_chord};
use crate::proto::*;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tokio::sync::oneshot;

enum Req {
    /// Action plus the logical bounding box of all displays (for coordinate mapping).
    Input(InputAction, Rect),
    ClipGet,
    ClipSet(String),
}

enum Resp {
    Unit(Result<()>),
    Text(Result<String>),
}

struct Job {
    req: Req,
    reply: oneshot::Sender<Resp>,
}

#[derive(Clone)]
pub struct InputWorker {
    tx: mpsc::Sender<Job>,
}

impl InputWorker {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("rdc-input".into())
            .spawn(move || worker(rx))
            .expect("spawn input thread");
        Self { tx }
    }

    async fn call(&self, req: Req) -> Resp {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(Job { req, reply }).is_err() {
            return Resp::Unit(Err(RdcError::Backend("input thread gone".into())));
        }
        rx.await.unwrap_or_else(|_| Resp::Unit(Err(RdcError::Backend("input thread dropped reply".into()))))
    }

    pub async fn input(&self, a: InputAction, desktop_bounds: Rect) -> Result<()> {
        match self.call(Req::Input(a, desktop_bounds)).await {
            Resp::Unit(r) => r,
            Resp::Text(_) => unreachable!(),
        }
    }

    pub async fn clipboard_get(&self) -> Result<String> {
        match self.call(Req::ClipGet).await {
            Resp::Text(r) => r,
            Resp::Unit(Err(e)) => Err(e),
            Resp::Unit(Ok(())) => unreachable!(),
        }
    }

    pub async fn clipboard_set(&self, t: String) -> Result<()> {
        match self.call(Req::ClipSet(t)).await {
            Resp::Unit(r) => r,
            Resp::Text(_) => unreachable!(),
        }
    }
}

struct Worker {
    enigo: Option<Enigo>,
    clip: Option<arboard::Clipboard>,
}

fn worker(rx: mpsc::Receiver<Job>) {
    let mut w = Worker { enigo: None, clip: None };
    for job in rx {
        let resp = match job.req {
            Req::Input(a, bounds) => Resp::Unit(w.enigo().and_then(|e| {
                let map = CoordMap::new(e, bounds);
                perform(e, &map, a)
            })),
            Req::ClipGet => Resp::Text(w.clip().and_then(|c| c.get_text().map_err(clip_err))),
            Req::ClipSet(t) => Resp::Unit(w.clip().and_then(|c| c.set_text(t).map_err(clip_err))),
        };
        let _ = job.reply.send(resp);
    }
}

fn clip_err(e: arboard::Error) -> RdcError {
    RdcError::Backend(format!("clipboard: {e}"))
}

impl Worker {
    fn enigo(&mut self) -> Result<&mut Enigo> {
        if self.enigo.is_none() {
            let settings = Settings {
                // Never pop a permissions dialog from a headless daemon; `doctor` reports instead.
                open_prompt_to_get_permissions: false,
                release_keys_when_dropped: true,
                ..Settings::default()
            };
            let e = Enigo::new(&settings).map_err(|e| match e {
                enigo::NewConError::NoPermission => RdcError::Permission("Accessibility not granted".into()),
                other => RdcError::Backend(format!("enigo init: {other}")),
            })?;
            self.enigo = Some(e);
        }
        Ok(self.enigo.as_mut().unwrap())
    }

    fn clip(&mut self) -> Result<&mut arboard::Clipboard> {
        if self.clip.is_none() {
            self.clip = Some(arboard::Clipboard::new().map_err(clip_err)?);
        }
        Ok(self.clip.as_mut().unwrap())
    }
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

/// Translate logical desktop coordinates into whatever enigo's absolute-move expects.
///
/// On Wayland, enigo sends `motion_absolute(x, y, extent_w, extent_h)` where the extent is the
/// first output's *physical* mode size, and the compositor maps that fraction across the whole
/// logical layout. So we express the point as a fraction of the logical desktop bounds and
/// scale it to the extent. Elsewhere enigo already speaks desktop coordinates.
struct CoordMap {
    bounds: Rect,
    extent: Option<(i32, i32)>,
}

impl CoordMap {
    fn new(e: &Enigo, bounds: Rect) -> Self {
        let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
        let extent = if wayland { e.main_display().ok().filter(|(w, h)| *w > 0 && *h > 0) } else { None };
        Self { bounds, extent }
    }

    fn map(&self, x: i32, y: i32) -> (i32, i32) {
        match self.extent {
            Some((ew, eh)) if self.bounds.w > 0 && self.bounds.h > 0 => {
                let fx = (x - self.bounds.x) as f64 / self.bounds.w as f64;
                let fy = (y - self.bounds.y) as f64 / self.bounds.h as f64;
                ((fx * ew as f64).round() as i32, (fy * eh as f64).round() as i32)
            }
            _ => (x, y),
        }
    }
}

fn move_abs(e: &mut Enigo, m: &CoordMap, x: i32, y: i32) -> Result<()> {
    let (mx, my) = m.map(x, y);
    e.move_mouse(mx, my, Coordinate::Abs).map_err(ie)
}

fn perform(e: &mut Enigo, m: &CoordMap, a: InputAction) -> Result<()> {
    match a {
        InputAction::MouseMove { x, y } => move_abs(e, m, x, y),
        InputAction::Click { x, y, button, count } => {
            move_abs(e, m, x, y)?;
            thread::sleep(Duration::from_millis(20));
            for i in 0..count.clamp(1, 3) {
                if i > 0 {
                    thread::sleep(Duration::from_millis(60));
                }
                e.button(btn(button), Direction::Click).map_err(ie)?;
            }
            Ok(())
        }
        InputAction::Button { button, down } => e
            .button(btn(button), if down { Direction::Press } else { Direction::Release })
            .map_err(ie),
        InputAction::Drag { from, to, button } => {
            move_abs(e, m, from.0, from.1)?;
            thread::sleep(Duration::from_millis(30));
            e.button(btn(button), Direction::Press).map_err(ie)?;
            let steps = 12;
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                let x = from.0 as f64 + (to.0 - from.0) as f64 * t;
                let y = from.1 as f64 + (to.1 - from.1) as f64 * t;
                move_abs(e, m, x.round() as i32, y.round() as i32)?;
                thread::sleep(Duration::from_millis(12));
            }
            thread::sleep(Duration::from_millis(30));
            e.button(btn(button), Direction::Release).map_err(ie)
        }
        InputAction::Scroll { at, dx, dy } => {
            if let Some((x, y)) = at {
                move_abs(e, m, x, y)?;
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

fn key_for(k: &KeyName) -> Key {
    match k {
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
        KeyName::F(n) => match n {
            1 => Key::F1, 2 => Key::F2, 3 => Key::F3, 4 => Key::F4, 5 => Key::F5, 6 => Key::F6,
            7 => Key::F7, 8 => Key::F8, 9 => Key::F9, 10 => Key::F10, 11 => Key::F11, 12 => Key::F12,
            13 => Key::F13, 14 => Key::F14, 15 => Key::F15, 16 => Key::F16, 17 => Key::F17, 18 => Key::F18,
            19 => Key::F19, 20 => Key::F20,
            #[cfg(not(target_os = "macos"))]
            21 => Key::F21,
            #[cfg(not(target_os = "macos"))]
            22 => Key::F22,
            #[cfg(not(target_os = "macos"))]
            23 => Key::F23,
            #[cfg(not(target_os = "macos"))]
            _ => Key::F24,
            // macOS defines no virtual keycodes above F20.
            #[cfg(target_os = "macos")]
            _ => Key::F20,
        },
        KeyName::Mod(m) => modifier_key(*m),
        KeyName::Char(c) => Key::Unicode(*c),
    }
}

fn press_chord(e: &mut Enigo, c: &Chord) -> Result<()> {
    for m in &c.mods {
        e.key(modifier_key(*m), Direction::Press).map_err(ie)?;
    }
    let r = e.key(key_for(&c.key), Direction::Click).map_err(ie);
    for m in c.mods.iter().rev() {
        // Always release, even if the key press failed.
        let _ = e.key(modifier_key(*m), Direction::Release);
    }
    r
}
