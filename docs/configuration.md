# Configuration

rdc reads one TOML file. If it is missing, defaults apply and everything can be given on the
command line instead.

| Platform | Path |
|---|---|
| Linux | `~/.config/rdc/config.toml` |
| macOS | `~/Library/Application Support/rdc/config.toml` |
| Windows | `%APPDATA%\rdc\config.toml` |

`rdc doctor` prints the path it is using.

## Full example

```toml
[serve]
port = 7770
# Optional. Default: this machine's Tailscale IPv4. Must be a Tailscale address.
# bind = "100.101.102.103"
# Who may control this machine. Empty list = daemon refuses to start.
allow = [
  "you@example.com",   # a tailnet login (as shown by `tailscale whois`)
  "studio-laptop",     # a node (device) name
  "tag:ops",           # every node carrying this ACL tag
]
# Optional. Extra names clients may use in the URL besides this node's Tailscale IPs,
# MagicDNS name and hostname (e.g. a CNAME you point at it).
# hosts = ["desk.internal.example"]

# Names you can pass to `--target` on the client side.
[targets.studio-mac]
url = "http://studio-mac.example-tailnet.ts.net:7770"

[targets.workshop-pc]
url = "http://100.64.10.20:7770"
```

## `[serve]`

| Key | Default | Meaning |
|---|---|---|
| `port` | `7770` | TCP port for the daemon |
| `bind` | Tailscale IPv4 | Address to listen on. Anything that isn't a Tailscale address (100.64.0.0/10 or fd7a:115c:a1e0::/48) is rejected at startup. |
| `allow` | `[]` | Allowlist entries, see below |
| `hosts` | `[]` | Extra accepted `Host` header names; the node's own IPs, MagicDNS name and hostname are always accepted |

Command-line equivalents: `rdc serve --port 7771 --bind 100.x.y.z --allow a@b --allow tag:ops`.
`--allow` flags are **added** to the config list.

### Allowlist rules

Each request's peer IP is resolved with `tailscaled`'s `whois`. The result has a node name and
either a login name (user-owned devices) or one or more tags (tagged devices). Tailscale still
reports the *creating* user's profile for tagged devices, but rdc ignores it: a tagged device can
only match by tag or node name, never by that user's login. An entry matches when,
case-insensitively:

- it equals the caller's **login name**, e.g. `alice@github`, `alice@example.com`;
- it equals the caller's **node name** (the short device name, without the tailnet suffix);
- it equals one of the caller's **tags**, e.g. `tag:family`;
- it is `*`, which allows everyone on the tailnet who can reach the port.

Results are cached for 30 seconds per IP. Loopback connections are always refused unless the
daemon was started with `--dev-loopback`.

### Host check

Before identity, the daemon checks the request's `Host` header against the names it answers to:
its Tailscale IPs, its MagicDNS name, its short hostname, and anything in `[serve].hosts`. A
request addressed to any other name gets `421 Misdirected Request`. This stops a web page on an
allowed machine from reaching the daemon through DNS rebinding, since the browser would send the
attacker's hostname. Use the Tailscale name or IP in your `[targets]` URLs.

## `[targets]`

Each table under `[targets]` names a machine for the client side. `url` is the daemon's base URL;
use the Tailscale MagicDNS name or the Tailscale IP. Only plain `http://` is needed since the
tailnet is already encrypted.

## Resolving `--target`

`rdc -t VALUE …` and `rdc mcp --target VALUE` accept, in order:

1. `local` — control this machine directly, no daemon involved (default).
2. A name from `[targets]`.
3. A full URL, `http://host:port`.
4. A bare `host` or `host:port`; the port defaults to `[serve].port`.

## Environment variables

| Variable | Effect |
|---|---|
| `RDC_TARGET` | default for `--target` |
| `RDC_LOG` | log filter, e.g. `debug`, `rdc=debug,hyper=warn` (tracing syntax) |
| `RDC_SIGN_IDENTITY` | macOS: code-signing identity name for `scripts/macos/bundle-and-sign.sh` (default `rdc-dev`) |
| `RDC_ALLOW_ADHOC` | macOS: set to `1` to let the bundle script fall back to ad-hoc signing |
