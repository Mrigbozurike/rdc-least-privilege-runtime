# macOS setup

Verified on macOS 15 and 26 (Tahoe) on Apple silicon. This guide is for the **machine being
controlled**; the client side just needs the binary.

## Why signing matters

macOS gates screen capture and synthetic input behind two permissions, Screen Recording and
Accessibility, granted per application in System Settings. The grant is keyed to the app's code
signature. An unsigned or ad-hoc-signed binary is identified by its exact hash, so **every
rebuild or upgrade loses the grants** and you are back to clicking prompts. A signed `.app`
bundle with a stable certificate keeps the grants across upgrades. A self-signed certificate you
make yourself is sufficient; no Apple developer account is needed.

Everything below assumes you build on the Mac. Cross-compiling from Linux is possible but you
still need the Mac to sign.

## Steps

1. **Rust and the source**

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
   git clone https://github.com/bscott/rdc ~/code/rdc
   ```

2. **Create the signing identity, once, from Terminal in the GUI session** (not over SSH; the
   trust step needs your login password):

   ```sh
   ~/code/rdc/scripts/macos/make-signing-identity.sh
   ```

   This creates a self-signed code-signing certificate named `rdc-dev` in a dedicated keychain
   (`~/Library/Keychains/rdc-signing.keychain-db`) with its password in
   `~/.config/rdc/signing-keychain-pass` (mode 0600), so later builds can sign non-interactively,
   including over SSH. The login keychain is locked in SSH sessions, which is why a separate
   keychain is used.

3. **Build, bundle and sign**

   ```sh
   ~/code/rdc/scripts/macos/bundle-and-sign.sh
   ```

   Produces `~/Applications/rdc.app`, signed with `rdc-dev`. The script refuses to fall back to
   ad-hoc signing unless `RDC_ALLOW_ADHOC=1`, for the reason above.

4. **Configure the allowlist**

   ```sh
   mkdir -p ~/Library/Application\ Support/rdc
   cat > ~/Library/Application\ Support/rdc/config.toml <<'EOF2'
   [serve]
   port = 7770
   allow = ["you@example.com"]
   EOF2
   ```

5. **Install the LaunchAgent from inside the bundle**

   ```sh
   ~/Applications/rdc.app/Contents/MacOS/rdc service install
   ```

   The daemon starts, logs to `~/Library/Application Support/rdc/serve.log`, and pops the two
   permission prompts on the Mac's screen.

6. **Grant the permissions** on the Mac's console (or via a KVM). Click "Open System Settings"
   on each prompt and turn on `rdc` under *Privacy & Security → Screen & System Audio Recording*
   and *Privacy & Security → Accessibility*. If `rdc` isn't listed, add it with the `+` button and
   pick `~/Applications/rdc.app` (in the file picker: Locations → your home → Applications).

   macOS may also show a second dialog saying rdc wants to "bypass the system private window
   picker"; allow it. It appears because the capture fallback path uses an older API.

7. **Restart and verify**

   ```sh
   launchctl kickstart -k gui/$(id -u)/dev.rdc.daemon
   grep permission ~/Library/Application\ Support/rdc/serve.log | tail -2
   ~/Applications/rdc.app/Contents/MacOS/rdc doctor
   ```

   Both permissions should read `granted`, and `doctor` should show a screenshot in well under a
   second with a window count larger than one.

## Upgrading

```sh
cd ~/code/rdc && git pull && ./scripts/macos/bundle-and-sign.sh
launchctl kickstart -k gui/$(id -u)/dev.rdc.daemon
```

Because the certificate is unchanged, the permissions carry over. No prompts.

## Things to know

- **Monthly re-consent.** Since macOS 15, the system periodically asks you to re-confirm Screen
  Recording for every app that uses it. When that happens screenshots show only the wallpaper;
  `rdc doctor` reports the permission missing and the daemon log says so on restart. Click the
  prompt once and restart the daemon.
- **Stuck grants.** If Settings shows `rdc` enabled but the daemon still reports "NOT granted",
  the entry belongs to an older signature. Clear it and let the daemon re-prompt:

  ```sh
  tccutil reset ScreenCapture dev.rdc.daemon
  tccutil reset Accessibility dev.rdc.daemon
  launchctl kickstart -k gui/$(id -u)/dev.rdc.daemon
  ```

- **Jump to a Settings pane from SSH** when you're driving the Mac remotely:

  ```sh
  open "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
  open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
  ```

- **Tailscale variants.** rdc finds the Tailscale LocalAPI for the App Store app, the standalone
  `Tailscale.app` (system extension) and open-source `tailscaled`. Nothing to configure.
- **Bundle identifier.** `dev.rdc.daemon` is the bundle id and LaunchAgent label, defined in
  `scripts/macos/bundle-and-sign.sh` and `src/service/mod.rs`.
- **Screenshots** use the system `screencapture` tool, which is fast (about 0.3 s for a 2560×1440
  display). The CoreGraphics fallback is much slower on recent macOS.
- **Window focus** activates the owning application; raising one specific window of a
  multi-window app is not implemented.
