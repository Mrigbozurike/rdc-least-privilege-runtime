# Changelog

## 0.2.0 — 2026-09-09

### Added
- **Capabilities.** Grants can limit an identity to `view`, `input` and/or `clipboard`.
  Config accepts plain strings (full control), inline tables
  `{ who = "...", can = [...] }` inside `allow`, or `[[serve.grant]]` blocks; the CLI accepts
  `--allow who=view,clipboard`. Missing capability → `403 forbidden`. `whoami` reports `caps`.
- **Audit log.** One JSON line per authorized request or rejection (host, identity and
  capability denials included) with identity, action summary, outcome and duration. Mode 0600,
  size-rotated, configurable under `[serve.audit]`. New `rdc audit` command to read it.
- `rdc doctor` prints the configured grants and the audit log path.

### Changed
- `--allow` and `[serve].allow` entries are now grants; existing plain-string configs behave
  exactly as before (full control).

## 0.1.0 — 2026-09-08

First public release. Remote desktop control for AI agents over Tailscale: screenshot, mouse,
keyboard, window focus and clipboard as MCP tools and a CLI. Tailscale whois identity with an
allowlist, Host-header check, input validation. Verified on Linux (Hyprland) and macOS (Apple
silicon); Windows compiles but is untested.
