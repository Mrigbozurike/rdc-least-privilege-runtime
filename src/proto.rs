//! Wire and domain types shared by the daemon, the HTTP client, the CLI and the MCP server.
//!
//! All coordinates are *logical desktop coordinates*: the virtual desktop spanning every
//! monitor, in points (not physical pixels). A [`Screenshot`] carries the logical [`Rect`] it
//! covers together with its pixel dimensions so callers can map image pixels back to points
//! without knowing anything about display scaling.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn union(&self, o: &Rect) -> Rect {
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w as i32).max(o.x + o.w as i32);
        let y1 = (self.y + self.h as i32).max(o.y + o.h as i32);
        Rect { x: x0, y: y0, w: (x1 - x0) as u32, h: (y1 - y0) as u32 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Display {
    pub id: u32,
    pub name: String,
    /// Logical geometry in desktop coordinates.
    pub rect: Rect,
    /// Physical pixels per logical point.
    pub scale: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub id: u64,
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub rect: Rect,
    pub focused: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayTarget {
    /// Every monitor composited into one image.
    #[default]
    All,
    Primary,
    Id(u32),
}

impl std::str::FromStr for DisplayTarget {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "all" => Ok(Self::All),
            "primary" => Ok(Self::Primary),
            n => n.parse().map(Self::Id).map_err(|_| format!("bad display target {n:?}")),
        }
    }
}

impl std::fmt::Display for DisplayTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Primary => write!(f, "primary"),
            Self::Id(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenshotReq {
    #[serde(default)]
    pub display: DisplayTarget,
    #[serde(default)]
    pub format: ImageFormat,
    /// Downscale so the longer edge is at most this many pixels. `None` = native.
    #[serde(default)]
    pub max_long_edge: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Screenshot {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// The logical desktop region this image shows.
    pub rect: Rect,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

impl std::str::FromStr for MouseButton {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "left" | "l" => Ok(Self::Left),
            "right" | "r" => Ok(Self::Right),
            "middle" | "m" => Ok(Self::Middle),
            o => Err(format!("unknown mouse button {o:?}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputAction {
    MouseMove {
        x: i32,
        y: i32,
    },
    Click {
        x: i32,
        y: i32,
        #[serde(default)]
        button: MouseButton,
        /// 1 = single, 2 = double, 3 = triple.
        #[serde(default = "one")]
        count: u8,
    },
    /// Press or release a button at the current pointer position.
    Button {
        button: MouseButton,
        down: bool,
    },
    Drag {
        from: (i32, i32),
        to: (i32, i32),
        #[serde(default)]
        button: MouseButton,
    },
    /// Scroll at a position; positive `dy` scrolls down, positive `dx` scrolls right.
    Scroll {
        #[serde(default)]
        at: Option<(i32, i32)>,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    },
    /// Type literal text (unicode ok).
    Type {
        text: String,
    },
    /// Press a key chord such as `cmd+shift+4`, `ctrl+c`, `enter`, `f5`.
    Key {
        chord: String,
    },
}

fn one() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "by", content = "value", rename_all = "snake_case")]
pub enum WindowTarget {
    Id(u64),
    /// Case-insensitive substring match on the application name.
    App(String),
    /// Case-insensitive substring match on the window title.
    Title(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Input(InputAction),
    Focus(WindowTarget),
    ClipboardSet { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub displays: Vec<Display>,
    pub windows: Vec<Window>,
}

/// What an identity is allowed to do. `view` covers displays, windows and screenshots;
/// `input` covers mouse, keyboard and window focus; `clipboard` covers reading and writing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    View,
    Input,
    Clipboard,
}

impl Capability {
    pub const ALL: [Capability; 3] = [Capability::View, Capability::Input, Capability::Clipboard];

    pub fn name(self) -> &'static str {
        match self {
            Capability::View => "view",
            Capability::Input => "input",
            Capability::Clipboard => "clipboard",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Capability {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "view" | "read" | "screen" => Ok(Capability::View),
            "input" | "control" => Ok(Capability::Input),
            "clipboard" | "clip" => Ok(Capability::Clipboard),
            o => Err(format!("unknown capability {o:?} (expected view, input, clipboard or all)")),
        }
    }
}

/// Identity of the caller as seen by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub login: Option<String>,
    pub node: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ip: String,
    /// Capabilities granted by the allowlist (empty until authorized).
    #[serde(default)]
    pub caps: Vec<Capability>,
}

impl Identity {
    pub fn has(&self, cap: Capability) -> bool {
        self.caps.contains(&cap)
    }

    /// Short human label: login, or node name for tagged devices.
    pub fn label(&self) -> &str {
        self.login.as_deref().unwrap_or(&self.node)
    }
}

impl Action {
    /// One-line description for logs and the audit trail. Never includes typed text.
    pub fn describe(&self) -> String {
        match self {
            Action::Input(a) => match a {
                InputAction::MouseMove { x, y } => format!("input.move {x},{y}"),
                InputAction::Click { x, y, button, count } => format!("input.click {x},{y} {button:?} x{count}"),
                InputAction::Button { button, down } => {
                    format!("input.button {button:?} {}", if *down { "down" } else { "up" })
                }
                InputAction::Drag { from, to, button } => {
                    format!("input.drag {},{} -> {},{} {button:?}", from.0, from.1, to.0, to.1)
                }
                InputAction::Scroll { at, dx, dy } => format!("input.scroll dx={dx} dy={dy} at={at:?}"),
                InputAction::Type { text } => format!("input.type {} chars", text.chars().count()),
                InputAction::Key { chord } => format!("input.key {chord}"),
            },
            Action::Focus(t) => format!("focus {t:?}"),
            Action::ClipboardSet { text } => format!("clipboard.set {} chars", text.chars().count()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RdcError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not supported on this platform/backend: {0}")]
    Unsupported(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("{0}")]
    Backend(String),
}

impl RdcError {
    /// The detail without the category prefix that `Display` adds.
    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(m)
            | Self::Unsupported(m)
            | Self::Permission(m)
            | Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::BadRequest(m)
            | Self::Backend(m) => m,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Unsupported(_) => "unsupported",
            Self::Permission(_) => "permission",
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::Backend(_) => "backend",
        }
    }
}

/// JSON body used for every non-2xx response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, RdcError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_json_shape() {
        let a = Action::Input(InputAction::Click { x: 1, y: 2, button: MouseButton::Left, count: 1 });
        let j = serde_json::to_string(&a).unwrap();
        assert_eq!(j, r#"{"kind":"input","type":"click","x":1,"y":2,"button":"left","count":1}"#);
        let f: Action = serde_json::from_str(r#"{"kind":"focus","by":"app","value":"Safari"}"#).unwrap();
        assert!(matches!(f, Action::Focus(WindowTarget::App(ref a)) if a == "Safari"));
    }
}
