//! `~/.config/rdc/config.toml`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 7770;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub serve: ServeConfig,
    #[serde(default)]
    pub targets: BTreeMap<String, Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    /// Address to bind. Default: this node's Tailscale IPv4.
    #[serde(default)]
    pub bind: Option<String>,
    /// Tailnet logins (`user@github`), node names (`studio-mac`), tags (`tag:family`), or `*`.
    #[serde(default)]
    pub allow: Vec<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT, bind: None, allow: vec![] }
    }
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub url: String,
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("rdc").join("config.toml")
}

pub fn load() -> Result<Config> {
    let p = path();
    if !p.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    toml::from_str(&raw).with_context(|| format!("parsing {}", p.display()))
}

impl Config {
    /// Resolve a `--target` value: `local`, a configured target name, or a raw URL.
    pub fn resolve_target(&self, t: &str) -> Result<TargetKind> {
        if t == "local" {
            return Ok(TargetKind::Local);
        }
        if let Some(tg) = self.targets.get(t) {
            return Ok(TargetKind::Url(tg.url.clone()));
        }
        if t.starts_with("http://") || t.starts_with("https://") {
            return Ok(TargetKind::Url(t.to_string()));
        }
        // Bare host[:port] → http://host:port
        if t.contains('.') || t.contains(':') || !t.contains(' ') {
            let url = if t.contains(':') { format!("http://{t}") } else { format!("http://{t}:{}", self.serve.port) };
            return Ok(TargetKind::Url(url));
        }
        anyhow::bail!("unknown target {t:?}; configure it in {} or pass a URL", path().display())
    }
}

pub enum TargetKind {
    Local,
    Url(String),
}
