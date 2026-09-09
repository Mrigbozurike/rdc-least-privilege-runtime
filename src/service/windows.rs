//! Windows: a Task Scheduler task that runs `rdc serve` in the user's interactive session at
//! logon. A Windows *service* would run in session 0 with no access to the desktop, so a
//! logon task is the right tool here.

use super::{LABEL, Op, log_dir, service_binary};
use anyhow::{Context, Result};
use std::process::Command;

fn schtasks(args: &[&str]) -> Result<std::process::Output> {
    Command::new("schtasks").args(args).output().context("running schtasks")
}

/// `schtasks /End` does not reliably stop a task started through `conhost --headless`, so kill
/// any other `rdc.exe` belonging to this user (never ourselves).
fn stop_running_daemons() {
    let me = std::process::id().to_string();
    let _ = Command::new("taskkill").args(["/F", "/IM", "rdc.exe", "/FI", &format!("PID ne {me}")]).output();
}

pub fn run(op: Op) -> Result<()> {
    match op {
        Op::Install => {
            let bin = service_binary()?;
            let logs = log_dir();
            std::fs::create_dir_all(&logs)?;
            let log = logs.join("serve.log");
            // conhost --headless keeps the console window from appearing at logon.
            let cmd = format!("conhost.exe --headless \"{}\" --log-file \"{}\" serve", bin.display(), log.display());
            let user = std::env::var("USERNAME").unwrap_or_default();
            // HIGHEST: run with the user's full (elevated) token. At LIMITED integrity Windows
            // (UIPI) silently drops input aimed at elevated windows and refuses to move focus to
            // them, so an admin PowerShell in front would make the desktop uncontrollable. UAC
            // prompts on the secure desktop remain out of reach either way.
            let mut args = vec!["/Create", "/F", "/TN", LABEL, "/SC", "ONLOGON", "/RL", "HIGHEST", "/TR", &cmd];
            // Without /RP the task runs only when this user is logged on, with the interactive
            // token, which is exactly what a desktop daemon needs.
            if !user.is_empty() {
                args.extend(["/RU", user.as_str()]);
            }
            let out = schtasks(&args)?;
            if !out.status.success() {
                anyhow::bail!("schtasks /Create failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            // Replace any daemon left over from a previous install before starting the new one.
            let _ = schtasks(&["/End", "/TN", LABEL]);
            stop_running_daemons();
            let out = schtasks(&["/Run", "/TN", LABEL])?;
            if !out.status.success() {
                anyhow::bail!("task created but /Run failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            println!("installed scheduled task {LABEL} → {}\nlogs: {}", bin.display(), log.display());
            Ok(())
        }
        Op::Uninstall => {
            let _ = schtasks(&["/End", "/TN", LABEL]);
            stop_running_daemons();
            let out = schtasks(&["/Delete", "/F", "/TN", LABEL])?;
            if out.status.success() {
                println!("removed {LABEL}");
            } else {
                println!("{LABEL}: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            Ok(())
        }
        Op::Status => {
            let out = schtasks(&["/Query", "/TN", LABEL, "/V", "/FO", "LIST"])?;
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for l in s.lines().filter(|l| {
                    l.starts_with("Status")
                        || l.starts_with("Last Run")
                        || l.starts_with("Task To Run")
                        || l.starts_with("Run As User")
                }) {
                    println!("{}", l.trim());
                }
            } else {
                println!("{LABEL}: not installed");
            }
            Ok(())
        }
    }
}
