# Contributing to rdc

Thanks for your interest. Bug reports with `rdc doctor` output, platform test reports, and small
focused pull requests are all welcome.

## Before you start

- Read [AGENTS.md](AGENTS.md): the design and security principles changes are reviewed against.

- For anything beyond a small fix, open an issue first so we can agree on the approach. The
  [architecture doc](docs/architecture.md) explains how the pieces fit.
- Security problems: do **not** open a public issue. See [SECURITY.md](SECURITY.md).
- Platform reports are contributions too. If you run rdc on a platform marked *untested* in the
  README, tell us what happened, good or bad.

## Development setup

```sh
git clone https://github.com/bscott/rdc && cd rdc
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Build dependencies per OS are in [docs/install.md](docs/install.md#build-dependencies). CI runs
the same four commands on Linux, macOS and Windows with warnings as errors, so run them before
pushing.

### Trying changes locally without a second machine

```sh
./target/release/rdc serve --dev-loopback              # unauthenticated 127.0.0.1, testing only
./target/release/rdc -t http://127.0.0.1:7770 shot
./target/release/rdc -t http://127.0.0.1:7770 click 400 300
```

Or skip the daemon entirely with `--target local`, which exercises the same `Desktop`
implementation in-process.

### Trying the MCP server

```sh
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' \
 '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | ./target/release/rdc mcp -t local
```

## Repository layout

| Path | Contents |
|---|---|
| `src/proto.rs` | wire and domain types shared by daemon, client, CLI and MCP |
| `src/desktop/mod.rs` | the `Desktop` trait |
| `src/desktop/local/` | in-process implementation: capture, input worker, per-platform window code |
| `src/desktop/remote.rs` | HTTP client implementation |
| `src/server/` | axum daemon, whois auth middleware, routes |
| `src/tailscale.rs` | LocalAPI discovery per platform, CLI fallback |
| `src/mcp.rs`, `src/view.rs` | MCP tools and pixel↔point mapping |
| `src/keys.rs` | key chord grammar |
| `src/service/` | LaunchAgent and systemd installers |
| `src/doctor.rs`, `src/permissions.rs` | readiness checks and macOS TCC helpers |
| `scripts/macos/` | signing identity and `.app` bundling |
| `skills/rdc/` | the agent skill |
| `docs/` | user documentation |

## Pull request checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass locally.
- [ ] Say which platforms you actually ran on, and how (foreground, service, MCP).
- [ ] If behaviour changed, update the relevant page in `docs/` and, if it affects agents,
      `skills/rdc/SKILL.md`.
- [ ] Add a line under **Unreleased** in `CHANGELOG.md`; it becomes the release notes.
- [ ] If you verified a platform that the README marks untested, update the status table.
- [ ] Commits are descriptive; one logical change per commit where practical.

## Style

- Keep the `Desktop` trait the single seam: new capabilities go there first, then to the wire
  API, then to the CLI and MCP. All three surfaces must stay equivalent.
- No new network listeners, no shell execution, no credentials in the tree.
- Prefer returning `RdcError` over panicking in daemon paths.
- Platform-specific code goes behind `cfg(target_os = …)` in its own module; keep the common
  path compiling on all three OSes. From Linux, after `rustup target add x86_64-pc-windows-gnu
  aarch64-apple-darwin`, run `cargo clippy --target <triple> --all-targets -- -D warnings` for
  both; CI runs clippy with warnings as errors on every platform, and a lint that only fires on
  one of them will fail the build there.

## License and sign-off

rdc is licensed under the GPL-3.0-or-later. By contributing you agree that your contributions
are licensed under the same terms.

Please sign off each commit (`git commit -s`), which adds a `Signed-off-by:` line certifying
the [Developer Certificate of Origin](https://developercertificate.org/): that you wrote the
change or otherwise have the right to submit it under the project license.
