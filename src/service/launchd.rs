use super::{LABEL, Op, log_dir, service_binary};
use anyhow::{Context, Result};
use std::process::Command;

fn plist_path() -> Result<std::path::PathBuf> {
    Ok(dirs::home_dir().context("no home dir")?.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

fn uid() -> String {
    String::from_utf8_lossy(&Command::new("id").arg("-u").output().map(|o| o.stdout).unwrap_or_default())
        .trim()
        .to_string()
}

pub fn run(op: Op) -> Result<()> {
    let plist = plist_path()?;
    let target = format!("gui/{}/{LABEL}", uid());
    match op {
        Op::Install => {
            let bin = service_binary()?;
            if !bin.to_string_lossy().contains(".app/Contents/MacOS/") {
                eprintln!(
                    "warning: {} is not inside an .app bundle; TCC permissions will reset on every rebuild.\n         Use scripts/macos/bundle-and-sign.sh and install from the bundle.",
                    bin.display()
                );
            }
            let logs = log_dir();
            std::fs::create_dir_all(&logs)?;
            std::fs::create_dir_all(plist.parent().unwrap())?;
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{bin}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>{out}</string>
  <key>StandardErrorPath</key><string>{err}</string>
  <key>EnvironmentVariables</key>
  <dict><key>RDC_LOG</key><string>info</string></dict>
</dict>
</plist>
"#,
                bin = bin.display(),
                out = logs.join("serve.log").display(),
                err = logs.join("serve.log").display(),
            );
            let _ = Command::new("launchctl").args(["bootout", &target]).output();
            std::fs::write(&plist, xml).with_context(|| format!("writing {}", plist.display()))?;
            let out = Command::new("launchctl")
                .args(["bootstrap", &format!("gui/{}", uid()), &plist.to_string_lossy()])
                .output()?;
            if !out.status.success() {
                anyhow::bail!("launchctl bootstrap failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            println!("installed {} → {}\nlogs: {}", plist.display(), bin.display(), logs.join("serve.log").display());
            Ok(())
        }
        Op::Uninstall => {
            let _ = Command::new("launchctl").args(["bootout", &target]).output();
            if plist.exists() {
                std::fs::remove_file(&plist)?;
            }
            println!("removed {LABEL}");
            Ok(())
        }
        Op::Status => {
            let out = Command::new("launchctl").args(["print", &target]).output()?;
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for l in s.lines().filter(|l| {
                    l.contains("state =") || l.contains("pid =") || l.contains("program =") || l.contains("last exit")
                }) {
                    println!("{}", l.trim());
                }
            } else {
                println!("{LABEL}: not loaded");
            }
            Ok(())
        }
    }
}
