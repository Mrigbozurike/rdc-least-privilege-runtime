# Contributing

Thanks for looking at rdc. Small, focused pull requests are easiest to review.

## Development

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
./target/release/rdc serve --dev-loopback     # unauthenticated 127.0.0.1, local testing only
./target/release/rdc -t http://127.0.0.1:7770 shot
```

CI runs rustfmt, clippy with warnings as errors, the tests and a release build on Linux, macOS
and Windows. Please make sure `cargo fmt` and `cargo clippy --all-targets -- -D warnings` are
clean before pushing.

Linux needs the development packages for xcb, xrandr, dbus, pipewire, wayland, EGL and
xkbcommon plus `libclang` (see the `apt-get` step in `.github/workflows/ci.yml` for the exact
Debian names). macOS and Windows need only the Rust toolchain.

## Layout

- `src/proto.rs` — wire and domain types shared by daemon, client, CLI and MCP.
- `src/desktop/` — the `Desktop` trait; `local/` (xcap, enigo, arboard, per-platform window
  code) and `remote.rs` (HTTP client).
- `src/server/` — axum daemon, Tailscale whois auth middleware, routes.
- `src/tailscale.rs` — LocalAPI discovery per platform and the CLI fallback.
- `src/mcp.rs` and `src/view.rs` — MCP tools and image-pixel to desktop-point mapping.
- `src/service/` — LaunchAgent / systemd user unit installers.
- `scripts/macos/` — bundle and sign `rdc.app`.
- `skills/rdc/` — the agent skill describing how to use rdc.

## Platform testing

Say in the PR which platforms you actually ran on. The support table in the README marks what
is verified versus what merely compiles; please update it when you verify something new.

## Commits

Conventional, descriptive commit messages. One logical change per commit where practical.

## License

By contributing you agree that your contributions are licensed under the AGPL-3.0-or-later,
the same license as the project.
