//! Identify the peer behind each connection with tailscaled whois and check the allowlist.

use super::{AppState, is_tailscale_ip};
use crate::proto::{ApiError, Identity, RdcError};
use crate::tailscale::Tailscale;
use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Allowlist {
    entries: Vec<String>,
}

impl Allowlist {
    pub fn new(entries: Vec<String>) -> Self {
        Self { entries: entries.into_iter().map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()).collect() }
    }

    pub fn permits(&self, id: &Identity) -> bool {
        let node = id.node.to_lowercase();
        let login = id.login.as_deref().map(str::to_lowercase);
        self.entries.iter().any(|e| {
            e == "*"
                || login.as_deref() == Some(e.as_str())
                || node == *e
                || id.tags.iter().any(|t| t.to_lowercase() == *e)
        })
    }
}

pub struct Auth {
    ts: Tailscale,
    allow: Allowlist,
    dev_loopback: bool,
    cache: Mutex<HashMap<IpAddr, (Instant, Identity)>>,
}

const CACHE_TTL: Duration = Duration::from_secs(30);

impl Auth {
    pub fn new(ts: Tailscale, allow: Allowlist, dev_loopback: bool) -> Self {
        Self { ts, allow, dev_loopback, cache: Mutex::new(HashMap::new()) }
    }

    pub async fn identify(&self, ip: IpAddr) -> Result<Identity, RdcError> {
        if ip.is_loopback() {
            if self.dev_loopback {
                return Ok(Identity {
                    login: Some("dev@loopback".into()),
                    node: "localhost".into(),
                    tags: vec![],
                    ip: ip.to_string(),
                });
            }
            return Err(RdcError::Unauthorized(
                "loopback connections are not accepted (use --dev-loopback while testing)".into(),
            ));
        }
        if !is_tailscale_ip(ip) {
            return Err(RdcError::Unauthorized(format!("{ip} is not a Tailscale address")));
        }
        if let Some((at, id)) = self.cache.lock().await.get(&ip)
            && at.elapsed() < CACHE_TTL
        {
            return Ok(id.clone());
        }
        let id = self.ts.whois(ip).await?;
        self.cache.lock().await.insert(ip, (Instant::now(), id.clone()));
        Ok(id)
    }

    pub async fn authorize(&self, ip: IpAddr) -> Result<Identity, RdcError> {
        let id = self.identify(ip).await?;
        if self.dev_loopback && ip.is_loopback() {
            return Ok(id);
        }
        if self.allow.permits(&id) {
            Ok(id)
        } else {
            Err(RdcError::Unauthorized(format!(
                "{} ({}) is not in the allowlist",
                id.login.as_deref().unwrap_or("no-login"),
                id.node
            )))
        }
    }
}

pub fn error_response(e: &RdcError) -> Response {
    let status = match e {
        RdcError::NotFound(_) => StatusCode::NOT_FOUND,
        RdcError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
        RdcError::Permission(_) => StatusCode::SERVICE_UNAVAILABLE,
        RdcError::Unauthorized(_) => StatusCode::FORBIDDEN,
        RdcError::BadRequest(_) => StatusCode::BAD_REQUEST,
        RdcError::Backend(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = ApiError { code: e.code().into(), message: e.message().to_string() };
    (status, axum::Json(body)).into_response()
}

pub async fn middleware(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    match state.auth.authorize(peer.ip()).await {
        Ok(id) => {
            tracing::info!(peer = %peer.ip(), who = id.login.as_deref().unwrap_or(&id.node), method = %req.method(), path = %req.uri().path(), "request");
            req.extensions_mut().insert(id);
            next.run(req).await
        }
        Err(e) => {
            tracing::warn!(peer = %peer.ip(), error = %e, "rejected");
            error_response(&e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(login: Option<&str>, node: &str, tags: &[&str]) -> Identity {
        Identity {
            login: login.map(String::from),
            node: node.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            ip: "100.1.1.1".into(),
        }
    }

    #[test]
    fn allowlist_matches_login_node_tag_and_star() {
        let a = Allowlist::new(vec!["BLSTech84@github".into(), "omarchy-fw13".into(), "tag:family".into()]);
        assert!(a.permits(&id(Some("blstech84@github"), "x", &[])));
        assert!(a.permits(&id(None, "Omarchy-FW13", &[])));
        assert!(a.permits(&id(None, "aw-m18r2", &["tag:family"])));
        assert!(!a.permits(&id(Some("someone@github"), "other", &["tag:server"])));
        assert!(Allowlist::new(vec!["*".into()]).permits(&id(None, "anyone", &[])));
        assert!(!Allowlist::new(vec![]).permits(&id(Some("a@b"), "n", &[])));
    }

    #[test]
    fn tailscale_ranges() {
        assert!(is_tailscale_ip("100.66.219.116".parse().unwrap()));
        assert!(is_tailscale_ip("100.127.255.255".parse().unwrap()));
        assert!(!is_tailscale_ip("100.128.0.1".parse().unwrap()));
        assert!(!is_tailscale_ip("10.0.2.113".parse().unwrap()));
        assert!(is_tailscale_ip("fd7a:115c:a1e0::1".parse().unwrap()));
    }
}
