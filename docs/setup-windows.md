# Windows setup

Verified on Windows 11 Home (build 26200) on an HP Omen laptop, single display at 150 % scaling.
This guide is for the **machine being controlled**; the client side just needs the binary.

## What works

- Screenshots (about 0.3 s for 2560×1600), window list with titles, focus, mouse, keyboard,
  clipboard, Tailscale identity through the LocalAPI named pipe, the audit log.
- `rdc service install` creates a Task Scheduler logon task that runs the daemon in your desktop
  session as a standard user, with a log file. Survives reboots and sign-in.

Not yet verified: multiple monitors (the code path exists, see issue #1), Windows 10.

## How it has to run

Two Windows facts shape the setup:

1. **The daemon must live in your interactive desktop session.** A Windows *service* or an
   SSH session runs in session 0, which has no real display: `rdc doctor` there reports a fake
   1024×768 monitor and screenshots fail. `rdc service install` therefore uses a scheduled task
   that runs at logon as you, not a service.
2. **It runs as a standard user by default.** The task uses your normal, filtered token
   (`LeastPrivilege` run level), so a remote-control process that anyone on your allowlist can drive holds no
   more privilege than any app you double-click. The trade-off is UIPI: Windows silently drops
   synthetic input aimed at *elevated* windows (an Administrator PowerShell, an installer) and
   refuses to move focus to them from a lower-integrity process. If you need to drive elevated
   windows remotely, `rdc service install --elevated` creates the task at the highest run level
   instead; understand that this leaves a permanently elevated process listening on your
   tailnet. UAC prompts on the secure desktop are out of reach either way, by design.

## Steps

1. **Get the binary.** Download `rdc-windows-x86_64.zip` from the releases page (SmartScreen
   will warn once; it is unsigned) or build from source with the MSVC toolchain:

   ```powershell
   winget install Git.Git
   # Visual Studio Build Tools with the C++ workload (MSVC + Windows SDK)
   Invoke-WebRequest https://aka.ms/vs/17/release/vs_BuildTools.exe -OutFile $env:TEMP\vs_BuildTools.exe
   & $env:TEMP\vs_BuildTools.exe --quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended
   Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile $env:TEMP\rustup-init.exe
   & $env:TEMP\rustup-init.exe -y --profile minimal
   git clone https://github.com/bscott/rdc; cd rdc; cargo build --release
   ```

   Put `rdc.exe` somewhere permanent, e.g. `%LOCALAPPDATA%\Programs\rdc\rdc.exe`. The scheduled
   task points at the path you install from.

2. **Config** at `%APPDATA%\rdc\config.toml`:

   ```toml
   [serve]
   port = 7770
   allow = ["you@example.com"]
   ```

3. **Firewall.** Allow the port from the tailnet only. The daemon binds just the Tailscale
   address regardless, but scoping the rule means a future misconfiguration cannot expose it:

   ```powershell
   New-NetFirewallRule -DisplayName "rdc (Tailscale)" -Direction Inbound -Protocol TCP -LocalPort 7770 `
     -RemoteAddress 100.64.0.0/10 -InterfaceAlias Tailscale -Action Allow
   ```

4. **Install the task** from a normal PowerShell, signed in as the account that uses the
   desktop. The default task runs at standard integrity and needs no Administrator shell to
   register. Only `--elevated` (highest run level) has to be run from an elevated PowerShell;
   `rdc service install --elevated` refuses otherwise.

   ```powershell
   rdc doctor                       # tailscaled, config, grants; display info is only real from the desktop
   rdc service install              # creates and starts task dev.rdc.daemon as a standard user
   rdc service install --elevated   # only if you must drive elevated windows (see above)
   rdc service status
   ```

   The task is named `dev.rdc.daemon.<username>`, one per account. It is registered from an
   XML definition with no battery restrictions, no run-time limit, and one instance at a time.

   Logs: `%LOCALAPPDATA%\rdc\serve.log`. Audit: `%LOCALAPPDATA%\rdc\audit.jsonl`.

5. **Verify from the client machine**: `rdc -t <omen> whoami`, `shot`, `windows`, `focus`.

## Upgrading

Copy the new `rdc.exe` over the old one after `rdc service uninstall` (which stops the daemon
started from that binary and nothing else), then `rdc service install` again. Or run `service
install` with the new binary in place; it stops the previous instance first.

## Things to know

- **Coordinates are physical pixels.** Windows reports monitor geometry and captures screenshots
  in physical pixels, so on a 150 % display a 2560×1600 screen is 2560×1600 points to rdc. That's
  consistent between screenshots and clicks, which is all that matters for the MCP mapping.
- **Multi-monitor.** Pointer moves use `SendInput` normalised against the whole virtual desktop,
  so secondary displays should work, but this is untested until someone runs it with two screens.
- **Elevated windows.** With the default (non-elevated) task, clicks and keystrokes aimed at an
  elevated window are dropped and `focus` on it fails; the audit log records the action as
  successful because Windows gives no error. Either close the elevated window, or reinstall with
  `--elevated`.
- **Focus.** Windows only lets a process take the foreground if it recently sent input. rdc
  attaches to the foreground thread's input queue (skipping threads that don't respond within
  200 ms) and, if that is refused, sends a zero-length mouse move and retries.
- **SSH for administration.** Enable OpenSSH Server (`Add-WindowsCapability -Online -Name
  OpenSSH.Server~~~~0.0.1.0`), put your key in `C:\ProgramData\ssh\administrators_authorized_keys`
  for admin accounts, and set PowerShell as the default shell via
  `HKLM:\SOFTWARE\OpenSSH\DefaultShell`. Remember that an SSH session is session 0: use it to
  build and to manage the task, not to run `rdc serve` directly.
- **Tailscale.** The LocalAPI is reached over the named pipe; the `tailscale.exe` CLI is the
  fallback. Turn on unattended mode in the Tailscale tray menu so the machine stays reachable
  while signed out.
