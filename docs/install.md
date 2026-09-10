# Install

rdc is a single binary. Install it on the machine you want to control **and** on the machine
your agent runs on. Both need to be on the same Tailscale tailnet.

## Release binaries

Each tagged release on GitHub attaches:

| File | Platform |
|---|---|
| `rdc-linux-x86_64.tar.gz` | Linux, x86_64, glibc |
| `rdc-macos-arm64.tar.gz` | macOS 15+, Apple silicon |
| `rdc-windows-x86_64.zip` | Windows 10/11, x86_64 |
| `SHA256SUMS` | checksums for the above |

Unpack and put `rdc` (or `rdc.exe`) somewhere on your `PATH`. Verify with
`sha256sum -c SHA256SUMS`.

### Unsigned binaries

The release binaries are **not code-signed or notarized**.

- **macOS**: Gatekeeper refuses to run the downloaded binary. You can clear the flag with
  `xattr -d com.apple.quarantine rdc`, but for the *daemon* you should not. macOS ties the
  Screen Recording and Accessibility permissions to the exact code hash of an unsigned binary,
  so every upgrade silently loses them. Build on the Mac and sign with your own certificate
  instead; [macOS setup](setup-macos.md) walks through it and a self-signed certificate is
  enough. The unsigned binary is fine for the *client* side (`rdc -t …`, `rdc mcp`).
- **Windows**: SmartScreen warns on first run. "More info" → "Run anyway", or build from source.
- **Linux**: no signature checks apply.

## With cargo

```sh
cargo install --git https://github.com/bscott/rdc --locked
```

Requires a recent stable Rust (the repo pins `stable` via `rust-toolchain.toml`; 1.90 or newer
is known to work).

## Build from source

```sh
git clone https://github.com/bscott/rdc
cd rdc
cargo build --release
./target/release/rdc --version
```

### Build dependencies

**Linux** (Debian/Ubuntu names; the CI workflow uses exactly this list):

```sh
sudo apt-get install -y pkg-config libclang-dev libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev \
  libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev libxkbcommon-dev libgbm-dev
```

Arch: `pacman -S clang pkgconf libxcb libxrandr dbus pipewire wayland libglvnd libxkbcommon mesa`.

**macOS**: Xcode Command Line Tools (`xcode-select --install`) and Rust. Nothing else.

**Windows**: Rust with the MSVC toolchain (the default from rustup). Nothing else.

## Where things live

| | Linux | macOS | Windows |
|---|---|---|---|
| config | `~/.config/rdc/config.toml` | `~/Library/Application Support/rdc/config.toml` | `%APPDATA%\rdc\config.toml` |
| service | `~/.config/systemd/user/dev.rdc.daemon.service` | `~/Library/LaunchAgents/dev.rdc.daemon.plist` | scheduled task `dev.rdc.daemon.<user>` |
| daemon log | `journalctl --user -u dev.rdc.daemon` | `~/Library/Application Support/rdc/serve.log` | `%LOCALAPPDATA%\rdc\serve.log` |
| audit log | `~/.local/state/rdc/audit.jsonl` | `~/Library/Application Support/rdc/audit.jsonl` | `%LOCALAPPDATA%\rdc\audit.jsonl` |

Next: [Configuration](configuration.md), then the setup guide for your target platform.
