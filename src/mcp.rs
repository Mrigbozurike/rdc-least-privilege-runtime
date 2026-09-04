//! MCP stdio server exposing a `Desktop` to an agent.
//!
//! Tool coordinates are pixels in the most recent screenshot returned by this server, which
//! is what a vision model naturally reads off the image. We translate to desktop points.

use crate::desktop::Desktop;
use crate::proto::*;
use crate::view::ViewMap;
use base64::Engine;
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::result::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const DEFAULT_MAX_EDGE: u32 = 1568;
const SETTLE: Duration = Duration::from_millis(350);

#[derive(Clone)]
pub struct RdcServer {
    desktop: Arc<dyn Desktop>,
    target: String,
    view: Arc<Mutex<Option<ViewMap>>>,
    /// Tools run one at a time so a burst of calls keeps its order and view mapping.
    ops: Arc<tokio::sync::Mutex<()>>,
    max_edge: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScreenshotParams {
    /// "all" (default), "primary", or a display id from `displays`.
    #[serde(default)]
    pub display: Option<String>,
    /// Longest edge in pixels (default 1568). Smaller is cheaper; larger shows more detail.
    #[serde(default)]
    pub max: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PointParams {
    /// X in pixels of the last screenshot.
    pub x: f64,
    /// Y in pixels of the last screenshot.
    pub y: f64,
    /// Return a fresh screenshot after the action (default true).
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ClickParams {
    pub x: f64,
    pub y: f64,
    /// left (default), right, middle.
    #[serde(default)]
    pub button: Option<String>,
    /// 1 = single (default), 2 = double, 3 = triple.
    #[serde(default)]
    pub count: Option<u8>,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct DragParams {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    #[serde(default)]
    pub button: Option<String>,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScrollParams {
    /// Position to scroll at (pixels of last screenshot). Default: current pointer.
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    /// Vertical amount in wheel steps; positive scrolls down (content moves up).
    #[serde(default)]
    pub dy: i32,
    /// Horizontal amount in wheel steps; positive scrolls right.
    #[serde(default)]
    pub dx: i32,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct TypeParams {
    /// Literal text to type, unicode ok. Use `key` for shortcuts and Enter/Tab.
    pub text: String,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct KeyParams {
    /// Key chord: `enter`, `escape`, `tab`, `cmd+q`, `ctrl+shift+t`, `alt+f4`, `f5`, `cmd+space`.
    /// `cmd`/`super`/`win` all mean the platform's Meta key.
    pub chord: String,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct FocusParams {
    /// Window id from `windows`.
    #[serde(default)]
    pub id: Option<u64>,
    /// Case-insensitive substring of the app name.
    #[serde(default)]
    pub app: Option<String>,
    /// Case-insensitive substring of the window title.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "yes")]
    pub then_screenshot: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct TextParams {
    pub text: String,
}

fn yes() -> bool {
    true
}

fn err(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

fn parse_button(b: &Option<String>) -> Result<MouseButton, ErrorData> {
    match b {
        None => Ok(MouseButton::Left),
        Some(s) => s.parse().map_err(|e: String| ErrorData::invalid_params(e, None)),
    }
}

#[tool_router]
impl RdcServer {
    pub fn new(desktop: Arc<dyn Desktop>, target: String, max_edge: Option<u32>) -> Self {
        Self { desktop, target, view: Arc::new(Mutex::new(None)), ops: Arc::new(tokio::sync::Mutex::new(())), max_edge: max_edge.unwrap_or(DEFAULT_MAX_EDGE) }
    }

    async fn take(&self, display: DisplayTarget, max: u32) -> Result<(Screenshot, ViewMap), ErrorData> {
        let shot = self
            .desktop
            .screenshot(ScreenshotReq { display, format: ImageFormat::Png, max_long_edge: Some(max) })
            .await
            .map_err(err)?;
        let map = ViewMap::from_screenshot(&shot);
        *self.view.lock().await = Some(map);
        Ok((shot, map))
    }

    async fn view(&self) -> Result<ViewMap, ErrorData> {
        if let Some(v) = *self.view.lock().await {
            return Ok(v);
        }
        Ok(self.take(DisplayTarget::All, self.max_edge).await?.1)
    }

    async fn to_desktop(&self, x: f64, y: f64) -> Result<(i32, i32), ErrorData> {
        Ok(self.view().await?.to_desktop(x, y))
    }

    fn image_result(&self, shot: &Screenshot, map: &ViewMap, note: Option<String>) -> CallToolResult {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&shot.data);
        let mut text = format!("{}: {}. Coordinates for click/move/drag/scroll are pixels in this image.", self.target, map.describe());
        if let Some(n) = note {
            text = format!("{n}\n{text}");
        }
        CallToolResult::success(vec![ContentBlock::text(text), ContentBlock::image(b64, shot.format.mime())])
    }

    async fn after(&self, then_screenshot: bool, note: String) -> Result<CallToolResult, ErrorData> {
        if !then_screenshot {
            return Ok(CallToolResult::success(vec![ContentBlock::text(note)]));
        }
        tokio::time::sleep(SETTLE).await;
        let display = DisplayTarget::All;
        let (shot, map) = self.take(display, self.max_edge).await?;
        Ok(self.image_result(&shot, &map, Some(note)))
    }

    #[tool(description = "Take a screenshot of the remote desktop. Returns the image plus the desktop region it covers. Always look at a screenshot before clicking.", annotations(read_only_hint = true))]
    async fn screenshot(&self, Parameters(p): Parameters<ScreenshotParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let display = match p.display.as_deref() {
            None => DisplayTarget::All,
            Some(s) => s.parse().map_err(|e: String| ErrorData::invalid_params(e, None))?,
        };
        let (shot, map) = self.take(display, p.max.unwrap_or(self.max_edge)).await?;
        Ok(self.image_result(&shot, &map, None))
    }

    #[tool(description = "List displays (id, name, logical geometry, scale).", annotations(read_only_hint = true))]
    async fn displays(&self) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let d = self.desktop.displays().await.map_err(err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(serde_json::to_string_pretty(&d).map_err(err)?)]))
    }

    #[tool(description = "List open windows with id, app, title, geometry (in desktop points and, if a screenshot exists, in image pixels) and focus state.", annotations(read_only_hint = true))]
    async fn windows(&self) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let ws = self.desktop.windows().await.map_err(err)?;
        let view = *self.view.lock().await;
        let rows: Vec<serde_json::Value> = ws
            .iter()
            .map(|w| {
                let mut v = serde_json::json!({
                    "id": w.id, "app": w.app, "title": w.title, "focused": w.focused, "minimized": w.minimized,
                    "desktop": { "x": w.rect.x, "y": w.rect.y, "w": w.rect.w, "h": w.rect.h },
                });
                if let Some(m) = view {
                    let (x0, y0) = m.to_view(w.rect.x, w.rect.y);
                    let (x1, y1) = m.to_view(w.rect.x + w.rect.w as i32, w.rect.y + w.rect.h as i32);
                    v["image"] = serde_json::json!({ "x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0 });
                }
                v
            })
            .collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(serde_json::to_string_pretty(&rows).map_err(err)?)]))
    }

    #[tool(description = "Bring a window to the front by id, app-name substring, or title substring.")]
    async fn focus(&self, Parameters(p): Parameters<FocusParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let t = match (p.id, p.app, p.title) {
            (Some(i), _, _) => WindowTarget::Id(i),
            (_, Some(a), _) => WindowTarget::App(a),
            (_, _, Some(t)) => WindowTarget::Title(t),
            _ => return Err(ErrorData::invalid_params("give one of id, app, title", None)),
        };
        self.desktop.focus(t.clone()).await.map_err(err)?;
        self.after(p.then_screenshot, format!("focused {t:?}")).await
    }

    #[tool(description = "Move the mouse pointer to a point (pixels of the last screenshot).")]
    async fn mouse_move(&self, Parameters(p): Parameters<PointParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let (x, y) = self.to_desktop(p.x, p.y).await?;
        self.desktop.input(InputAction::MouseMove { x, y }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("moved pointer to image ({}, {}) = desktop ({x}, {y})", p.x, p.y)).await
    }

    #[tool(description = "Click at a point (pixels of the last screenshot). button: left|right|middle; count: 1|2|3.")]
    async fn click(&self, Parameters(p): Parameters<ClickParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let (x, y) = self.to_desktop(p.x, p.y).await?;
        let button = parse_button(&p.button)?;
        let count = p.count.unwrap_or(1).clamp(1, 3);
        self.desktop.input(InputAction::Click { x, y, button, count }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("{button:?} click x{count} at image ({}, {}) = desktop ({x}, {y})", p.x, p.y)).await
    }

    #[tool(description = "Press-drag-release from one point to another (pixels of the last screenshot).")]
    async fn drag(&self, Parameters(p): Parameters<DragParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let from = self.to_desktop(p.x1, p.y1).await?;
        let to = self.to_desktop(p.x2, p.y2).await?;
        let button = parse_button(&p.button)?;
        self.desktop.input(InputAction::Drag { from, to, button }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("dragged desktop {from:?} → {to:?}")).await
    }

    #[tool(description = "Scroll by wheel steps, optionally at a point. Positive dy scrolls down, positive dx scrolls right.")]
    async fn scroll(&self, Parameters(p): Parameters<ScrollParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let at = match (p.x, p.y) {
            (Some(x), Some(y)) => Some(self.to_desktop(x, y).await?),
            _ => None,
        };
        self.desktop.input(InputAction::Scroll { at, dx: p.dx, dy: p.dy }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("scrolled dx={} dy={} at {at:?}", p.dx, p.dy)).await
    }

    #[tool(name = "type", description = "Type literal text into the focused window. For Enter, Tab, shortcuts, use `key`.")]
    async fn type_text(&self, Parameters(p): Parameters<TypeParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let n = p.text.chars().count();
        self.desktop.input(InputAction::Type { text: p.text }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("typed {n} characters")).await
    }

    #[tool(description = "Press a key or key chord, e.g. enter, escape, cmd+q, ctrl+shift+t, alt+f4, f5.")]
    async fn key(&self, Parameters(p): Parameters<KeyParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        crate::keys::parse_chord(&p.chord).map_err(|e| ErrorData::invalid_params(e.message().to_string(), None))?;
        self.desktop.input(InputAction::Key { chord: p.chord.clone() }).await.map_err(err)?;
        self.after(p.then_screenshot, format!("pressed {}", p.chord)).await
    }

    #[tool(description = "Read the remote clipboard as text.", annotations(read_only_hint = true))]
    async fn clipboard_get(&self) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        let t = self.desktop.clipboard_get().await.map_err(err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(t)]))
    }

    #[tool(description = "Set the remote clipboard text (useful for pasting long text with key ctrl+v / cmd+v).")]
    async fn clipboard_set(&self, Parameters(p): Parameters<TextParams>) -> Result<CallToolResult, ErrorData> {
        let _op = self.ops.lock().await;
        self.desktop.clipboard_set(p.text).await.map_err(err)?;
        Ok(CallToolResult::success(vec![ContentBlock::text("clipboard set")]))
    }
}

#[tool_handler]
impl ServerHandler for RdcServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("rdc", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "You control the desktop of `{}` through rdc. Take a screenshot first, read the \
                 image, then act. All x/y you pass are PIXELS IN THE MOST RECENT SCREENSHOT \
                 (not desktop points); rdc converts them. Most actions return a fresh screenshot \
                 so you can verify the result; pass then_screenshot=false to skip it. Use `key` \
                 for Enter, Escape and shortcuts; use `type` for text. There is no shell access.",
                self.target
            ))
    }
}

pub async fn run(desktop: Arc<dyn Desktop>, target: String, max_edge: Option<u32>) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    let server = RdcServer::new(desktop, target, max_edge);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
