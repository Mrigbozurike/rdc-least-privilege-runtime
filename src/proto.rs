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
    MouseMove { x: i32, y: i32 },
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
    Button { button: MouseButton, down: bool },
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
    Type { text: String },
    /// Press a key chord such as `cmd+shift+4`, `ctrl+c`, `enter`, `f5`.
    Key { chord: String },
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

/// Identity of the caller as seen by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub login: Option<String>,
    pub node: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ip: String,
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
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("{0}")]
    Backend(String),
}

impl RdcError {
    /// The detail without the category prefix that `Display` adds.
    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(m) | Self::Unsupported(m) | Self::Permission(m) | Self::Unauthorized(m) | Self::BadRequest(m) | Self::Backend(m) => m,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Unsupported(_) => "unsupported",
            Self::Permission(_) => "permission",
            Self::Unauthorized(_) => "unauthorized",
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
