use super::{LABEL, Op, service_binary};
use anyhow::{Context, Result};
use std::process::Command;

fn unit_path() -> Result<std::path::PathBuf> {
    Ok(dirs::config_dir().context("no config dir")?.join("systemd/user").join(format!("{LABEL}.service")))
}

fn systemctl(args: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new("systemctl").arg("--user").args(args).output()?)
}

pub fn run(op: Op) -> Result<()> {
    let unit = unit_path()?;
    let name = format!("{LABEL}.service");
    match op {
        Op::Install => {
            let bin = service_binary()?;
            std::fs::create_dir_all(unit.parent().unwrap())?;
            let text = format!(
                "[Unit]\nDescription=rdc remote desktop control daemon\nAfter=graphical-session.target tailscaled.service\nPartOf=graphical-session.target\n\n[Service]\nExecStart={} serve\nRestart=on-failure\nRestartSec=3\nEnvironment=RDC_LOG=info\n\n[Install]\nWantedBy=graphical-session.target\n",
                bin.display()
            );
            std::fs::write(&unit, text).with_context(|| format!("writing {}", unit.display()))?;
            systemctl(&["daemon-reload"])?;
            let out = systemctl(&["enable", "--now", &name])?;
            if !out.status.success() {
                anyhow::bail!("systemctl enable failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            println!("installed {} → {}\nlogs: journalctl --user -u {name} -f", unit.display(), bin.display());
            Ok(())
        }
        Op::Uninstall => {
            let _ = systemctl(&["disable", "--now", &name]);
            if unit.exists() {
                std::fs::remove_file(&unit)?;
            }
            systemctl(&["daemon-reload"])?;
            println!("removed {name}");
            Ok(())
        }
        Op::Status => {
            let out = systemctl(&["status", "--no-pager", &name])?;
            print!("{}", String::from_utf8_lossy(&out.stdout));
            Ok(())
        }
    }
}
