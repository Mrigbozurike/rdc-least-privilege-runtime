//! Talk to the local tailscaled to (a) learn our own tailnet IPs and (b) identify peers.
//!
//! Primary path is the LocalAPI (unix socket / named pipe / macOS loopback+token).
//! Fallback is the `tailscale` CLI with `--json`, which shares the same response shape.

use crate::proto::{Identity, RdcError, Result};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::Command;
use tailscale_localapi::LocalApi;

#[derive(Clone)]
enum Api {
    #[cfg(unix)]
    Unix(LocalApi<tailscale_localapi::UnixStreamClient>),
    #[cfg(windows)]
    Pipe(LocalApi<tailscale_localapi::WindowsNamedPipeClient>),
    #[cfg(target_os = "macos")]
    Tcp(LocalApi<tailscale_localapi::TcpWithPasswordClient>),
    Cli(PathBuf),
    None,
}

#[derive(Clone)]
pub struct Tailscale {
    api: Api,
    cli: Option<PathBuf>,
}

/// Minimal, lenient mirror of tailscaled's WhoIsResponse for the CLI fallback.
#[derive(Deserialize)]
struct WhoIsJson {
    #[serde(rename = "Node")]
    node: NodeJson,
    #[serde(rename = "UserProfile", default)]
    user: Option<UserJson>,
}
#[derive(Deserialize)]
struct NodeJson {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ComputedName", default)]
    computed_name: String,
    #[serde(rename = "Tags", default)]
    tags: Vec<String>,
}
#[derive(Deserialize)]
struct UserJson {
    #[serde(rename = "LoginName", default)]
    login_name: String,
}

impl Tailscale {
    pub fn detect() -> Self {
        let cli = find_cli();
        let api = detect_api().unwrap_or_else(|| cli.clone().map(Api::Cli).unwrap_or(Api::None));
        Self { api, cli }
    }

    pub fn describe(&self) -> String {
        match &self.api {
            #[cfg(unix)]
            Api::Unix(_) => "LocalAPI via unix socket".into(),
            #[cfg(windows)]
            Api::Pipe(_) => "LocalAPI via named pipe".into(),
            #[cfg(target_os = "macos")]
            Api::Tcp(_) => "LocalAPI via loopback TCP (GUI variant)".into(),
            Api::Cli(p) => format!("tailscale CLI at {}", p.display()),
            Api::None => "not found".into(),
        }
    }

    pub fn available(&self) -> bool {
        !matches!(self.api, Api::None)
    }

    /// Our own tailnet IPs, IPv4 first.
    pub async fn self_ips(&self) -> Result<Vec<IpAddr>> {
        let mut ips = match &self.api {
            #[cfg(unix)]
            Api::Unix(a) => a.status().await.map(|s| s.tailscale_ips).map_err(ts),
            #[cfg(windows)]
            Api::Pipe(a) => a.status().await.map(|s| s.tailscale_ips).map_err(ts),
            #[cfg(target_os = "macos")]
            Api::Tcp(a) => a.status().await.map(|s| s.tailscale_ips).map_err(ts),
            Api::Cli(_) | Api::None => Err(RdcError::Backend("no LocalAPI".into())),
        }
        .or_else(|e| self.cli_ips().ok_or(e))?;
        ips.sort_by_key(|ip| !ip.is_ipv4());
        Ok(ips)
    }

    fn cli_ips(&self) -> Option<Vec<IpAddr>> {
        let out = Command::new(self.cli.as_ref()?).arg("ip").output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect())
    }

    /// Identify the peer behind `ip`. Fails for non-tailnet addresses.
    pub async fn whois(&self, ip: IpAddr) -> Result<Identity> {
        let addr = SocketAddr::new(ip, 0);
        let via_api = match &self.api {
            #[cfg(unix)]
            Api::Unix(a) => Some(a.whois(addr).await),
            #[cfg(windows)]
            Api::Pipe(a) => Some(a.whois(addr).await),
            #[cfg(target_os = "macos")]
            Api::Tcp(a) => Some(a.whois(addr).await),
            Api::Cli(_) | Api::None => None,
        };
        match via_api {
            Some(Ok(w)) => {
                return Ok(Identity {
                    login: Some(w.user_profile.login_name).filter(|s| !s.is_empty()),
                    node: node_short(&w.node.name, &w.node.computed_name),
                    tags: w.node.tags,
                    ip: ip.to_string(),
                });
            }
            Some(Err(e)) => tracing::debug!("LocalAPI whois failed ({e}); trying CLI"),
            None => {}
        }
        let cli = self.cli.as_ref().ok_or_else(|| RdcError::Unauthorized(format!("cannot identify {ip}: no tailscaled access")))?;
        let out = Command::new(cli)
            .args(["whois", "--json", &ip.to_string()])
            .output()
            .map_err(|e| RdcError::Backend(format!("tailscale whois: {e}")))?;
        if !out.status.success() {
            return Err(RdcError::Unauthorized(format!(
                "{ip} is not a tailnet peer ({})",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let w: WhoIsJson = serde_json::from_slice(&out.stdout).map_err(|e| RdcError::Backend(format!("whois json: {e}")))?;
        Ok(Identity {
            login: w.user.map(|u| u.login_name).filter(|s| !s.is_empty()),
            node: node_short(&w.node.name, &w.node.computed_name),
            tags: w.node.tags,
            ip: ip.to_string(),
        })
    }
}

fn ts(e: tailscale_localapi::Error) -> RdcError {
    RdcError::Backend(format!("tailscale LocalAPI: {e}"))
}

/// "brians-m4-mac-mini.dog-dragon.ts.net." → "brians-m4-mac-mini"
fn node_short(name: &str, computed: &str) -> String {
    if !computed.is_empty() {
        return computed.to_string();
    }
    name.trim_end_matches('.').split('.').next().unwrap_or(name).to_string()
}

fn find_cli() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
            "/usr/local/bin/tailscale",
            "/opt/homebrew/bin/tailscale",
        ]
    } else if cfg!(windows) {
        &["C:\\Program Files\\Tailscale\\tailscale.exe"]
    } else {
        &["/usr/bin/tailscale", "/usr/local/bin/tailscale", "/usr/sbin/tailscale"]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists()).or_else(|| which("tailscale"))
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(bin)).find(|p| p.is_file())
}

#[cfg(target_os = "linux")]
fn detect_api() -> Option<Api> {
    let p = std::path::Path::new("/var/run/tailscale/tailscaled.sock");
    p.exists().then(|| Api::Unix(LocalApi::new_with_socket_path(p)))
}

#[cfg(target_os = "macos")]
fn detect_api() -> Option<Api> {
    let p = std::path::Path::new("/var/run/tailscaled.socket");
    if p.exists() {
        return Some(Api::Unix(LocalApi::new_with_socket_path(p)));
    }
    let (port, token) = macos_proof()?;
    Some(Api::Tcp(LocalApi::new_with_port_and_password(port, token)))
}

#[cfg(target_os = "windows")]
fn detect_api() -> Option<Api> {
    Some(Api::Pipe(LocalApi::new_with_named_pipe_path(
        r"\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled",
    )))
}

/// GUI Tailscale on macOS publishes `sameuserproof-<port>-<token>`; the CLI finds it via lsof
/// on the IPNExtension process. Standalone builds use /Library/Tailscale/ipnport + sameuserproof-<port>.
#[cfg(target_os = "macos")]
fn macos_proof() -> Option<(u16, String)> {
    // App Store / standalone GUI: parse the open file name out of lsof.
    let out = Command::new("lsof").args(["-n", "-a", "-c", "IPNExtension", "-F", "n"]).output().ok()?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(idx) = line.find("sameuserproof-") {
            let rest = &line[idx + "sameuserproof-".len()..];
            let mut it = rest.split('-');
            if let (Some(port), Some(tok)) = (it.next().and_then(|p| p.parse().ok()), it.next()) {
                return Some((port, tok.trim().to_string()));
            }
        }
    }
    // Standalone "macsys" variant (system extension): `ipnport` is a symlink whose *target
    // name* is the port, and the proof file is readable by the admin group.
    let link = std::fs::read_link("/Library/Tailscale/ipnport").ok()?;
    let port: u16 = link.to_string_lossy().trim().parse().ok()?;
    let tok = std::fs::read_to_string(format!("/Library/Tailscale/sameuserproof-{port}")).ok()?;
    Some((port, tok.trim().to_string()))
}
