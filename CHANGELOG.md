# Changelog

Entries under **Unreleased** go into the next release's notes; `scripts/release-notes.sh`
builds the GitHub release body from the matching `## <version>` section.

## Unreleased

### Added
- **Windows support, verified on Windows 11.** `rdc service install` creates an elevated Task
  Scheduler logon task that runs the daemon in the interactive session with a log file;
  `service uninstall` stops the running daemon. Window `focus` implemented (foreground-thread
  attach with an Alt-tap fallback). Absolute pointer moves use `SendInput` over the whole
  virtual desktop so secondary monitors are addressable (untested on real hardware yet).
- `--log-file` / `RDC_LOG_FILE` to append logs to a file instead of stderr.

### Fixed
- Windows: the service path no longer carries the `\\?\` verbatim prefix.

## 0.2.1 — 2026-09-09

### Changed
- **License is now GPL-3.0-or-later** (was AGPL-3.0-or-later). rdc is a program you run on
  your own machines, not a hosted service, so the AGPL network clause added friction without
  protecting anything; plain GPL keeps the requirement that distributed modifications are
  published. There are no external contributions to date, so no consent was needed.
- GitHub releases carry the changelog section for the version as their notes.
- CONTRIBUTING asks for DCO sign-off (`git commit -s`).

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
