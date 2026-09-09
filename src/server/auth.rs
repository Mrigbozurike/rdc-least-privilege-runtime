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
use std::collections::{HashMap, HashSet};
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

/// Host names (without port) that requests may be addressed to.
#[derive(Debug, Clone)]
pub struct HostAllow {
    names: HashSet<String>,
}

impl HostAllow {
    pub fn new(names: Vec<String>) -> Self {
        Self { names: names.into_iter().map(|n| normalize_host(&n)).filter(|n| !n.is_empty()).collect() }
    }

    /// `host` is the raw `Host` header value or URI authority, possibly with a port.
    pub fn permits(&self, host: &str) -> bool {
        self.names.contains(&normalize_host(host))
    }
}

/// Lower-case, strip a trailing dot, the port, and IPv6 brackets.
fn normalize_host(raw: &str) -> String {
    let h = raw.trim().to_lowercase();
    let h = h.trim_end_matches('.');
    let h = if let Some(rest) = h.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else if h.matches(':').count() == 1 {
        h.split(':').next().unwrap_or("")
    } else {
        h
    };
    h.trim_end_matches('.').to_string()
}

pub struct Auth {
    ts: Tailscale,
    allow: Allowlist,
    hosts: HostAllow,
    dev_loopback: bool,
    cache: Mutex<HashMap<IpAddr, (Instant, Identity)>>,
}

const CACHE_TTL: Duration = Duration::from_secs(30);

impl Auth {
    pub fn new(ts: Tailscale, allow: Allowlist, hosts: HostAllow, dev_loopback: bool) -> Self {
        Self { ts, allow, hosts, dev_loopback, cache: Mutex::new(HashMap::new()) }
    }

    /// Refuse requests whose `Host` names something we are not. This is the one line of
    /// defence against a browser on an allowed machine being pointed at us via DNS rebinding.
    pub fn check_host(&self, host: Option<&str>) -> Result<(), RdcError> {
        match host {
            Some(h) if self.hosts.permits(h) => Ok(()),
            Some(h) => Err(RdcError::Unauthorized(format!("request addressed to unexpected host {h:?}"))),
            None => Err(RdcError::Unauthorized("request has no Host header".into())),
        }
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
    let host = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| req.uri().authority().map(|a| a.to_string()));
    if let Err(e) = state.auth.check_host(host.as_deref()) {
        tracing::warn!(peer = %peer.ip(), error = %e, "rejected");
        return (
            StatusCode::MISDIRECTED_REQUEST,
            axum::Json(ApiError { code: e.code().into(), message: e.message().to_string() }),
        )
            .into_response();
    }
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
        let a = Allowlist::new(vec!["Alice@github".into(), "studio-mac".into(), "tag:family".into()]);
        assert!(a.permits(&id(Some("alice@github"), "x", &[])));
        assert!(a.permits(&id(None, "Studio-Mac", &[])));
        assert!(a.permits(&id(None, "gaming-pc", &["tag:family"])));
        assert!(!a.permits(&id(Some("someone@github"), "other", &["tag:server"])));
        assert!(Allowlist::new(vec!["*".into()]).permits(&id(None, "anyone", &[])));
        assert!(!Allowlist::new(vec![]).permits(&id(Some("a@b"), "n", &[])));
        // A tagged node never carries a login (see tailscale::identity), so only its tag or
        // node name can match.
        assert!(!a.permits(&id(None, "alice-server", &["tag:server"])));
    }

    #[test]
    fn host_allow_normalizes() {
        let h =
            HostAllow::new(vec!["Studio-Mac.example.ts.net.".into(), "100.64.0.5".into(), "fd7a:115c:a1e0::1".into()]);
        assert!(h.permits("studio-mac.example.ts.net:7770"));
        assert!(h.permits("STUDIO-MAC.EXAMPLE.TS.NET"));
        assert!(h.permits("100.64.0.5:7770"));
        assert!(h.permits("[fd7a:115c:a1e0::1]:7770"));
        assert!(!h.permits("attacker.example.com:7770"));
        assert!(!h.permits("100.64.0.6"));
        assert!(!h.permits(""));
    }

    #[test]
    fn tailscale_ranges() {
        assert!(is_tailscale_ip("100.64.0.1".parse().unwrap()));
        assert!(is_tailscale_ip("100.127.255.255".parse().unwrap()));
        assert!(!is_tailscale_ip("100.128.0.1".parse().unwrap()));
        assert!(!is_tailscale_ip("10.0.2.113".parse().unwrap()));
        assert!(is_tailscale_ip("fd7a:115c:a1e0::1".parse().unwrap()));
    }
}
