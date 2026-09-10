//! Windows: a Task Scheduler task that runs `rdc serve` in the user's interactive session at
//! logon. A Windows *service* would run in session 0 with no access to the desktop, so a logon
//! task is the right tool. The task is registered from an XML definition because `schtasks`
//! flags cannot express the settings a persistent daemon needs (no battery restrictions, no
//! execution time limit, one instance).

use super::{LABEL, Op, log_dir, service_binary};
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

fn user() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "user".into())
}

/// The account to register the task for. Task Scheduler accepts a SID, which avoids guessing
/// between `DOMAIN\user` and `COMPUTER\user` (USERDOMAIN can be `WORKGROUP` on a standalone
/// machine, which resolves to nothing).
fn principal() -> String {
    if let Ok(out) = Command::new("whoami").args(["/user", "/fo", "csv", "/nh"]).output()
        && out.status.success()
    {
        let line = String::from_utf8_lossy(&out.stdout);
        // "COMPUTER\user","S-1-5-21-..."
        if let Some(sid) = line.split(',').nth(1).map(|s| s.trim().trim_matches('"').to_string())
            && sid.starts_with("S-1-")
        {
            return sid;
        }
    }
    let host = std::env::var("COMPUTERNAME").unwrap_or_default();
    if host.is_empty() { user() } else { format!("{host}\\{}", user()) }
}

/// Per-user task name. Task names are machine-global, so two accounts must not share one.
fn task_name() -> String {
    format!("{LABEL}.{}", user())
}

fn schtasks(args: &[&str]) -> Result<std::process::Output> {
    Command::new("schtasks").args(args).output().context("running schtasks")
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// True when this process runs with a high-integrity (elevated) token. Registering a task at
/// the highest run level requires it.
fn is_elevated() -> bool {
    Command::new("whoami")
        .args(["/groups"])
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains("S-1-16-12288") || s.contains("S-1-16-16384")
        })
        .unwrap_or(false)
}

/// Stop daemons started from *this installed binary* for *this user*: same executable path, a
/// `serve` command line, and never this process. Nothing else named rdc.exe is touched.
fn stop_running_daemons(bin: &Path) -> Result<()> {
    let me = std::process::id();
    let bin_s = bin.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         Get-CimInstance Win32_Process -Filter \"Name='rdc.exe'\" | \
         Where-Object {{ $_.ProcessId -ne {me} -and $_.ExecutablePath -eq '{bin_s}' -and $_.CommandLine -match '(^|\\s)serve(\\s|$)' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force; \"stopped $($_.ProcessId)\" }}"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .context("running powershell to stop the daemon")?;
    if !out.status.success() {
        bail!("could not stop the running daemon: {}", stderr_of(&out));
    }
    for line in String::from_utf8_lossy(&out.stdout).lines().filter(|l| l.starts_with("stopped")) {
        println!("{line}");
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn task_xml(bin: &Path, log: &Path) -> String {
    let who = xml_escape(&principal());
    let args = xml_escape(&format!("--headless \"{}\" --log-file \"{}\" serve", bin.display(), log.display()));
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>rdc remote desktop control daemon (runs in the interactive session)</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{who}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{who}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>conhost.exe</Command>
      <Arguments>{args}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn write_utf16(path: &Path, text: &str) -> Result<()> {
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// Remove a task registered by an older rdc under the unscoped name, but only if it is ours.
fn remove_legacy_task() {
    if let Ok(out) = schtasks(&["/Query", "/TN", LABEL, "/V", "/FO", "LIST"])
        && out.status.success()
        && String::from_utf8_lossy(&out.stdout).contains("rdc.exe")
    {
        let _ = schtasks(&["/End", "/TN", LABEL]);
        let _ = schtasks(&["/Delete", "/F", "/TN", LABEL]);
        println!("removed legacy task {LABEL}");
    }
}

pub fn run(op: Op) -> Result<()> {
    let name = task_name();
    match op {
        Op::Install => {
            if !is_elevated() {
                bail!(
                    "installing the rdc task needs an elevated PowerShell (run as Administrator) under the account that will use the desktop; \
                     the task runs at the highest run level so it can drive elevated windows"
                );
            }
            let bin = service_binary()?;
            let logs = log_dir();
            std::fs::create_dir_all(&logs)?;
            let log = logs.join("serve.log");
            remove_legacy_task();
            let xml_path = std::env::temp_dir().join(format!("{name}.xml"));
            write_utf16(&xml_path, &task_xml(&bin, &log))?;
            let out = schtasks(&["/Create", "/F", "/TN", &name, "/XML", &xml_path.to_string_lossy()])?;
            let _ = std::fs::remove_file(&xml_path);
            if !out.status.success() {
                bail!("schtasks /Create failed: {}", stderr_of(&out));
            }
            // Replace any daemon left over from a previous install before starting the new one.
            let _ = schtasks(&["/End", "/TN", &name]);
            stop_running_daemons(&bin)?;
            let out = schtasks(&["/Run", "/TN", &name])?;
            if !out.status.success() {
                bail!("task created but /Run failed: {}", stderr_of(&out));
            }
            println!("installed scheduled task {name} → {}\nlogs: {}", bin.display(), log.display());
            Ok(())
        }
        Op::Uninstall => {
            let query = schtasks(&["/Query", "/TN", &name])?;
            if !query.status.success() {
                println!("{name}: not installed");
                // Still stop a stray daemon from this binary, if any.
                if let Ok(bin) = service_binary() {
                    stop_running_daemons(&bin)?;
                }
                return Ok(());
            }
            let _ = schtasks(&["/End", "/TN", &name]);
            if let Ok(bin) = service_binary() {
                stop_running_daemons(&bin)?;
            }
            let out = schtasks(&["/Delete", "/F", "/TN", &name])?;
            if !out.status.success() {
                bail!("schtasks /Delete failed: {}", stderr_of(&out));
            }
            println!("removed {name}");
            Ok(())
        }
        Op::Status => {
            let out = schtasks(&["/Query", "/TN", &name, "/V", "/FO", "LIST"])?;
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for l in s.lines().filter(|l| {
                    l.starts_with("Status")
                        || l.starts_with("Last Run")
                        || l.starts_with("Task To Run")
                        || l.starts_with("Run As User")
                        || l.starts_with("Logon Mode")
                }) {
                    println!("{}", l.trim());
                }
            } else {
                println!("{name}: not installed");
            }
            Ok(())
        }
    }
}
