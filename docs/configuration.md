# Configuration

rdc reads one TOML file. If it is missing, defaults apply and everything can be given on the
command line instead.

| Platform | Path |
|---|---|
| Linux | `~/.config/rdc/config.toml` |
| macOS | `~/Library/Application Support/rdc/config.toml` |
| Windows | `%APPDATA%\rdc\config.toml` |

`rdc doctor` prints the path it is using. Unknown keys anywhere in the file are errors, so a
typo such as `caps` instead of `can` stops the daemon from starting rather than silently granting
more than intended.

## Full example

```toml
[serve]
port = 7770
# Optional. Default: this machine's Tailscale IPv4. Must be a Tailscale address.
# bind = "100.101.102.103"

# Who may control this machine, and how much. Empty = daemon refuses to start.
# A plain string grants everything; an inline table limits it to some capabilities.
allow = [
  "you@example.com",                                        # full control
  { who = "monitor-bot", can = "view" },                    # screenshots only
  { who = ["tag:ops", "bob@example.com"], can = ["view", "clipboard"] },
]

# Optional. Extra names clients may use in the URL besides this node's Tailscale IPs,
# MagicDNS name and hostname (e.g. a CNAME you point at it). Keep this above any
# [[serve.grant]] block; TOML would otherwise attach it to the grant.
# hosts = ["desk.internal.example"]

# The same thing as a block, if you prefer one grant per section.
[[serve.grant]]
who = "tag:family"
can = "all"

[serve.audit]
enabled = true                 # default
# path = "/var/log/rdc/audit.jsonl"   # default: rdc/audit.jsonl in the platform state dir
max_size_mb = 50               # rotate above this size
keep = 5                       # keep audit.jsonl.1 … .5

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
| `allow` | `[]` | Grants: plain identity strings (full control) or `{ who, can }` tables, see below |
| `grant` | `[]` | `[[serve.grant]]` blocks, same shape as the table form of `allow` |
| `audit` | enabled | Audit log settings, see below |
| `hosts` | `[]` | Extra accepted `Host` header names; the node's own IPs, MagicDNS name and hostname are always accepted |

Command-line equivalents: `rdc serve --port 7771 --bind 100.x.y.z --allow a@b --allow tag:ops=view`.
`--allow` flags are **added** to the config grants; `who=cap,cap` limits capabilities, a bare
identity grants all.

### Grants and capabilities

Each grant names one or more identities (`who`) and what they may do (`can`):

| Capability | Allows |
|---|---|
| `view` | `displays`, `windows`, `screenshot`, `whoami` |
| `input` | mouse, keyboard, `focus` |
| `clipboard` | reading and writing the clipboard |
| `all` | everything (the default when `can` is omitted, and what a plain string grants) |

`who` and `can` each take one value or a list. When several grants match the same caller, their
capabilities are combined. A caller that lacks a capability gets `403 forbidden` with a message
naming the missing one, and the attempt is written to the audit log. `whoami` and `/health` need
a valid identity but no particular capability.

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

### Audit log

Every authorized request and every rejection is appended as one JSON object per line:

```json
{"ts":"2026-09-09T16:08:55.979Z","peer":"100.64.0.7","login":"alice@example.com","node":"laptop",
 "method":"POST","path":"/v1/act","action":"input.click 100,100 Left x1","outcome":"denied",
 "status":403,"detail":"alice@example.com may not use `input` on this machine","ms":0}
```

`outcome` is `ok`, `denied` (host, identity or capability) or `error`. `action` describes the
request without its payload: typed text is recorded only as a character count. Key chords, window
selectors and error messages are recorded as sent, with control characters replaced, so a hostile
value cannot break the file or the terminal you read it in. The file and its rotated copies are
mode 0600. Read it with `rdc audit` (`-n`, `--json`, `--path`).

Default location: `~/.local/state/rdc/audit.jsonl` (Linux), `~/Library/Application
Support/rdc/audit.jsonl` (macOS), `%LOCALAPPDATA%\rdc\audit.jsonl` (Windows).

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
