//! HTTP client implementing `Desktop` against a remote `rdc serve`.

use super::Desktop;
use crate::proto::*;
use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode};
use std::time::Duration;

pub struct RemoteDesktop {
    base: String,
    http: Client,
}

pub const H_RECT: &str = "x-rdc-rect";
pub const H_SIZE: &str = "x-rdc-size";

impl RemoteDesktop {
    pub fn new(base_url: &str) -> Result<Self> {
        let base = base_url.trim_end_matches('/').to_string();
        if !base.starts_with("http://") && !base.starts_with("https://") {
            return Err(RdcError::BadRequest(format!("target url must start with http:// or https://, got {base:?}")));
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| RdcError::Backend(e.to_string()))?;
        Ok(Self { base, http })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}/v1{}", self.base, path)
    }

    async fn check(resp: Response) -> Result<Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        if let Ok(e) = serde_json::from_str::<ApiError>(&body) {
            return Err(match e.code.as_str() {
                "not_found" => RdcError::NotFound(e.message),
                "unsupported" => RdcError::Unsupported(e.message),
                "permission" => RdcError::Permission(e.message),
                "unauthorized" => RdcError::Unauthorized(e.message),
                "bad_request" => RdcError::BadRequest(e.message),
                _ => RdcError::Backend(e.message),
            });
        }
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => RdcError::Unauthorized(body),
            _ => RdcError::Backend(format!("HTTP {status}: {body}")),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.http.get(self.url(path)).send().await.map_err(net)?;
        Self::check(resp).await?.json().await.map_err(net)
    }

    async fn post_json<B: serde::Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let resp = self.http.post(self.url(path)).json(body).send().await.map_err(net)?;
        Self::check(resp).await.map(|_| ())
    }

    /// The daemon's view of who we are (requires the daemon to accept us).
    pub async fn whoami(&self) -> Result<Identity> {
        self.get_json("/whoami").await
    }
}

fn net(e: reqwest::Error) -> RdcError {
    RdcError::Backend(format!("http: {e}"))
}

fn parse_pair<T: std::str::FromStr>(s: &str) -> Option<Vec<T>> {
    s.split(',').map(|p| p.trim().parse().ok()).collect()
}

#[async_trait]
impl Desktop for RemoteDesktop {
    async fn displays(&self) -> Result<Vec<Display>> {
        Ok(self.state().await?.displays)
    }

    async fn state(&self) -> Result<State> {
        self.get_json("/state").await
    }

    async fn screenshot(&self, req: ScreenshotReq) -> Result<Screenshot> {
        let mut q = vec![("display", req.display.to_string()), ("format", req.format.ext().to_string())];
        if let Some(m) = req.max_long_edge {
            q.push(("max", m.to_string()));
        }
        let resp = self.http.get(self.url("/screenshot")).query(&q).send().await.map_err(net)?;
        let resp = Self::check(resp).await?;
        let hdr = |k: &str| resp.headers().get(k).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let rect: Vec<i64> = hdr(H_RECT)
            .and_then(|s| parse_pair(&s))
            .filter(|v| v.len() == 4)
            .ok_or_else(|| RdcError::Backend("missing x-rdc-rect header".into()))?;
        let size: Vec<u32> = hdr(H_SIZE)
            .and_then(|s| parse_pair(&s))
            .filter(|v| v.len() == 2)
            .ok_or_else(|| RdcError::Backend("missing x-rdc-size header".into()))?;
        let data = resp.bytes().await.map_err(net)?.to_vec();
        Ok(Screenshot {
            format: req.format,
            width: size[0],
            height: size[1],
            rect: Rect { x: rect[0] as i32, y: rect[1] as i32, w: rect[2] as u32, h: rect[3] as u32 },
            data,
        })
    }

    async fn windows(&self) -> Result<Vec<Window>> {
        Ok(self.state().await?.windows)
    }

    async fn focus(&self, target: WindowTarget) -> Result<()> {
        self.post_json("/act", &Action::Focus(target)).await
    }

    async fn input(&self, action: InputAction) -> Result<()> {
        self.post_json("/act", &Action::Input(action)).await
    }

    async fn clipboard_get(&self) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct Clip {
            text: String,
        }
        Ok(self.get_json::<Clip>("/clipboard").await?.text)
    }

    async fn clipboard_set(&self, text: String) -> Result<()> {
        self.post_json("/act", &Action::ClipboardSet { text }).await
    }
}
