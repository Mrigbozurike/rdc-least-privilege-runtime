# Grants and Tailscale policy

Two lists decide who can control a machine with rdc:

1. **The Tailscale access policy** (in the admin console) decides whether a device's packets
   reach port 7770 on the machine at all.
2. **rdc's grants** (in `config.toml` on the machine) decide what an identity that gets through
   may do: `view`, `input`, `clipboard`, or `all`.

A connection blocked by the policy times out and never appears in rdc's audit log. A connection
that reaches rdc but is not in a grant gets `403` and is logged. Both lists name the same kinds of
identity: tailnet logins such as `alice@example.com` and tags such as `tag:ops`. rdc grants can
also name a device by its node name; the policy cannot, so tag devices you want to reference
there.

Every example below assumes the machine running `rdc serve` carries the tag `tag:rdc-host`.
Tag it with `tailscale up --advertise-tags=tag:rdc-host` or from the admin console, and declare
the tag under `tagOwners` in the policy.

## One person, full control

The common case: you control your own machines from your own devices.

`config.toml` on the controlled machine:

```toml
[serve]
allow = ["alice@example.com"]
```

Policy (`grants` syntax):

```jsonc
{
  "tagOwners": { "tag:rdc-host": ["autogroup:admin"] },
  "grants": [
    { "src": ["alice@example.com"], "dst": ["tag:rdc-host"], "ip": ["tcp:7770"] },
  ],
}
```

Legacy `acls` syntax:

```jsonc
"acls": [
  { "action": "accept", "src": ["alice@example.com"], "dst": ["tag:rdc-host:7770"] },
]
```

## A view-only monitoring device

A device tagged `tag:monitor` may take screenshots but cannot click or type. In rdc it is limited
to `view`; in the policy it needs the port open like anyone else.

```toml
[serve]
allow = [
  "alice@example.com",
  { who = "tag:monitor", can = "view" },
]
```

```jsonc
"grants": [
  { "src": ["alice@example.com", "tag:monitor"], "dst": ["tag:rdc-host"], "ip": ["tcp:7770"] },
]
```

A request from that device for anything but `screenshot`, `displays` or `windows` returns
`403 forbidden: … may not use input` and is written to the audit log.

## A family or team tag

Everyone whose device carries `tag:family` gets full control.

```toml
[serve]
allow = ["tag:family"]
```

```jsonc
"tagOwners": { "tag:rdc-host": ["autogroup:admin"], "tag:family": ["autogroup:admin"] },
"grants": [
  { "src": ["tag:family"], "dst": ["tag:rdc-host"], "ip": ["tcp:7770"] },
]
```

Tailscale reports the user who created a tagged device; rdc ignores that login. A tagged device
matches only by tag or node name.

## Several grants for the same person

Grants add up. Here a person gets `view` and `clipboard` from one grant and `input` from another,
so they end up with all three.

```toml
[serve]
allow = [
  { who = ["tag:ops", "bob@example.com"], can = ["view", "clipboard"] },
  { who = "bob@example.com", can = "input" },
]
```

```jsonc
"grants": [
  { "src": ["tag:ops", "bob@example.com"], "dst": ["tag:rdc-host"], "ip": ["tcp:7770"] },
]
```

## Block-style grants

The same grants can be written as `[[serve.grant]]` blocks if you prefer one per section. Keep
other `[serve]` keys such as `hosts` above the first block.

```toml
[serve]
port = 7770
hosts = ["desk.internal.example"]

[[serve.grant]]
who = "alice@example.com"

[[serve.grant]]
who = "tag:monitor"
can = "view"
```

## A user-owned host instead of a tagged one

If the controlled machine is not tagged, use its owner's login as the policy destination (this
covers all of that user's devices) or list its Tailscale IP under `hosts` in the policy:

```jsonc
"hosts": { "studio-mac": "100.101.102.103" },
"grants": [
  { "src": ["alice@example.com"], "dst": ["studio-mac"], "ip": ["tcp:7770"] },
]
```

## Mapping table

| rdc grant | Policy `src` | Notes |
|---|---|---|
| `"alice@example.com"` | `alice@example.com` | login, full control |
| `{ who = "tag:monitor", can = "view" }` | `tag:monitor` | screenshots only |
| `{ who = "studio-laptop" }` | a tag on that device, or a `hosts` entry | policies cannot name a node directly |
| `"tag:family"` | `tag:family` | everyone with the tag |
| `"*"` | whoever the policy admits | rdc accepts any tailnet identity that reaches it |

## Checking the two lists agree

From the client machine:

```sh
tailscale ping studio-mac          # the tunnel works
rdc -t studio-mac whoami           # rdc admits you and shows your capabilities
```

A timeout on the second command means the policy; a `403` means the rdc grant. On the daemon
machine, `rdc audit -n 20` shows what arrived and how it was decided.
