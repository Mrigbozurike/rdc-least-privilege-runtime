//! The config file: `~/.config/rdc/config.toml` on Linux, `~/Library/Application Support/rdc/`
//! on macOS, `%APPDATA%\rdc\` on Windows.

use crate::proto::Capability;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 7770;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub serve: ServeConfig,
    #[serde(default)]
    pub targets: BTreeMap<String, Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    /// Address to bind. Default: this node's Tailscale IPv4.
    #[serde(default)]
    pub bind: Option<String>,
    /// Who may connect. Each entry is either a plain identity string (full control) or an
    /// inline table `{ who = ..., can = ... }`. See [`Grant`].
    #[serde(default)]
    pub allow: Vec<AllowEntry>,
    /// The same as table-form `allow` entries, for people who prefer `[[serve.grant]]` blocks.
    #[serde(default)]
    pub grant: Vec<Grant>,
    /// Extra host names clients may use in the URL (the node's Tailscale IPs, MagicDNS name and
    /// hostname are always accepted). Requests with any other `Host` header are refused.
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub audit: AuditConfig,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            bind: None,
            allow: vec![],
            grant: vec![],
            hosts: vec![],
            audit: AuditConfig::default(),
        }
    }
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

/// An `allow` entry: `"alice@example.com"` or `{ who = "alice@example.com", can = ["view"] }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AllowEntry {
    Simple(String),
    Grant(Grant),
}

/// Identities (`who`) and what they may do (`can`).
///
/// `who` is one identity or a list: tailnet logins (`alice@example.com`), node names
/// (`studio-mac`), tags (`tag:ops`), or `*`. `can` is `"all"` (default), one capability, or a
/// list of `view`, `input`, `clipboard`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub who: OneOrMany,
    #[serde(default)]
    pub can: Option<OneOrMany>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn items(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

/// A grant with everything parsed and validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGrant {
    pub who: Vec<String>,
    pub caps: BTreeSet<Capability>,
}

impl ResolvedGrant {
    pub fn full(who: Vec<String>) -> Self {
        Self { who, caps: Capability::ALL.into_iter().collect() }
    }

    pub fn describe(&self) -> String {
        let caps = if self.caps.len() == Capability::ALL.len() {
            "all".to_string()
        } else {
            self.caps.iter().map(|c| c.name()).collect::<Vec<_>>().join(",")
        };
        format!("{} ({caps})", self.who.join(", "))
    }
}

/// Parse a capability list: `all`, one name, or several names.
pub fn parse_caps<I: IntoIterator<Item = S>, S: AsRef<str>>(items: I) -> Result<BTreeSet<Capability>> {
    let mut caps = BTreeSet::new();
    for item in items {
        for part in item.as_ref().split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part.eq_ignore_ascii_case("all") || part == "*" {
                caps.extend(Capability::ALL);
            } else {
                caps.insert(part.parse::<Capability>().map_err(anyhow::Error::msg)?);
            }
        }
    }
    if caps.is_empty() {
        anyhow::bail!("a grant must allow at least one capability (view, input, clipboard or all)");
    }
    Ok(caps)
}

impl Grant {
    pub fn resolve(&self) -> Result<ResolvedGrant> {
        let who: Vec<String> =
            self.who.items().into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if who.is_empty() {
            anyhow::bail!("a grant needs at least one identity in `who`");
        }
        let caps = match &self.can {
            None => Capability::ALL.into_iter().collect(),
            Some(c) => parse_caps(c.items())?,
        };
        Ok(ResolvedGrant { who, caps })
    }
}

impl ServeConfig {
    /// Every grant from `allow` and `[[serve.grant]]`, validated.
    pub fn grants(&self) -> Result<Vec<ResolvedGrant>> {
        let mut out = Vec::new();
        for e in &self.allow {
            match e {
                AllowEntry::Simple(s) if !s.trim().is_empty() => {
                    out.push(ResolvedGrant::full(vec![s.trim().to_string()]))
                }
                AllowEntry::Simple(_) => {}
                AllowEntry::Grant(g) => out.push(g.resolve().context("in [serve].allow")?),
            }
        }
        for g in &self.grant {
            out.push(g.resolve().context("in [[serve.grant]]")?);
        }
        Ok(out)
    }
}

/// `--allow who` or `--allow who=view,clipboard`.
pub fn parse_allow_flag(s: &str) -> Result<ResolvedGrant> {
    let (who, caps) = match s.split_once('=') {
        Some((w, c)) => (w.trim(), parse_caps([c])?),
        None => (s.trim(), Capability::ALL.into_iter().collect()),
    };
    if who.is_empty() {
        anyhow::bail!("--allow needs an identity, e.g. --allow alice@example.com or --allow tag:ops=view");
    }
    Ok(ResolvedGrant { who: vec![who.to_string()], caps })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    /// Write one JSON line per request. Default on.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Log file. Default: `rdc/audit.jsonl` under the platform state directory.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Rotate when the file exceeds this many megabytes.
    #[serde(default = "default_audit_mb")]
    pub max_size_mb: u64,
    /// How many rotated files to keep (`audit.jsonl.1` … `.N`).
    #[serde(default = "default_audit_keep")]
    pub keep: u32,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self { enabled: true, path: None, max_size_mb: default_audit_mb(), keep: default_audit_keep() }
    }
}

fn yes() -> bool {
    true
}
fn default_audit_mb() -> u64 {
    50
}
fn default_audit_keep() -> u32 {
    5
}

impl AuditConfig {
    pub fn resolved_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(|| state_dir().join("audit.jsonl"))
    }
}

/// Platform state directory for rdc (logs, audit trail).
pub fn state_dir() -> PathBuf {
    dirs::state_dir().or_else(dirs::data_local_dir).unwrap_or_else(|| PathBuf::from(".")).join("rdc")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    let cfg: Config = toml::from_str(&raw).with_context(|| format!("parsing {}", p.display()))?;
    // Fail early on malformed grants so the daemon never starts with a half-read allowlist.
    cfg.serve.grants().with_context(|| format!("in {}", p.display()))?;
    Ok(cfg)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(v: &[Capability]) -> BTreeSet<Capability> {
        v.iter().copied().collect()
    }

    #[test]
    fn simple_and_table_entries_mix() {
        let cfg: Config = toml::from_str(
            r#"
            [serve]
            allow = [
              "alice@example.com",
              { who = "monitor-bot", can = "view" },
              { who = ["tag:ops", "bob@example.com"], can = ["view", "clipboard"] },
            ]
            [[serve.grant]]
            who = "tag:family"
            can = "all"
            "#,
        )
        .unwrap();
        let g = cfg.serve.grants().unwrap();
        assert_eq!(g.len(), 4);
        assert_eq!(g[0], ResolvedGrant::full(vec!["alice@example.com".into()]));
        assert_eq!(g[1], ResolvedGrant { who: vec!["monitor-bot".into()], caps: caps(&[Capability::View]) });
        assert_eq!(g[2].who, vec!["tag:ops".to_string(), "bob@example.com".to_string()]);
        assert_eq!(g[2].caps, caps(&[Capability::View, Capability::Clipboard]));
        assert_eq!(g[3].caps.len(), 3);
    }

    #[test]
    fn rejects_unknown_fields_instead_of_granting_everything() {
        // `caps` instead of `can` must not silently become full control.
        assert!(
            toml::from_str::<Config>(
                r#"[serve]
allow = [{ who = "monitor-bot", caps = ["view"] }]"#
            )
            .is_err()
        );
        assert!(
            toml::from_str::<Config>(
                r#"[[serve.grant]]
who = "monitor-bot"
caps = "view""#
            )
            .is_err()
        );
        assert!(toml::from_str::<Config>("[serve]\nalow = ['x']").is_err());
    }

    #[test]
    fn rejects_unknown_capability() {
        let cfg: Config = toml::from_str(
            r#"[serve]
allow = [{ who = "x", can = "shell" }]"#,
        )
        .unwrap();
        assert!(cfg.serve.grants().is_err());
    }

    #[test]
    fn allow_flag_syntax() {
        assert_eq!(
            parse_allow_flag("alice@example.com").unwrap(),
            ResolvedGrant::full(vec!["alice@example.com".into()])
        );
        let g = parse_allow_flag("tag:ops=view,clipboard").unwrap();
        assert_eq!(g.who, vec!["tag:ops".to_string()]);
        assert_eq!(g.caps, caps(&[Capability::View, Capability::Clipboard]));
        assert!(parse_allow_flag("=view").is_err());
        assert!(parse_allow_flag("x=bogus").is_err());
    }

    #[test]
    fn audit_defaults() {
        let cfg: Config = toml::from_str("[serve]\nallow = ['a']").unwrap();
        assert!(cfg.serve.audit.enabled);
        assert_eq!(cfg.serve.audit.max_size_mb, 50);
        let cfg: Config = toml::from_str("[serve.audit]\nenabled = false\npath = '/tmp/x.jsonl'").unwrap();
        assert!(!cfg.serve.audit.enabled);
        assert_eq!(cfg.serve.audit.resolved_path(), PathBuf::from("/tmp/x.jsonl"));
    }
}
