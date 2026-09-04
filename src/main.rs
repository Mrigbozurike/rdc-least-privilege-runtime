mod config;
mod desktop;
mod doctor;
mod keys;
mod mcp;
mod permissions;
mod proto;
mod server;
mod service;
mod tailscale;
mod view;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use desktop::{Desktop, local::LocalDesktop, remote::RemoteDesktop};
use proto::*;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rdc", version, about = "Remote Desktop Control for AI agents, over Tailscale")]
struct Cli {
    /// Machine to control: `local`, a name from config `[targets]`, a host[:port], or a URL.
    #[arg(short, long, global = true, default_value = "local", env = "RDC_TARGET")]
    target: String,
    #[arg(long, global = true, default_value = "info", env = "RDC_LOG")]
    log: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon on this machine (the one to be controlled).
    Serve {
        /// Address to bind (default: this node's Tailscale IPv4).
        #[arg(long)]
        bind: Option<IpAddr>,
        #[arg(long)]
        port: Option<u16>,
        /// Tailnet login, node name or tag permitted to connect (repeatable; adds to config).
        #[arg(long = "allow")]
        allow: Vec<String>,
        /// Bind 127.0.0.1 and skip authentication for loopback. Testing only.
        #[arg(long)]
        dev_loopback: bool,
    },
    /// Run as an MCP stdio server for an agent, controlling --target.
    Mcp {
        /// Longest screenshot edge in pixels sent to the agent (default 1568).
        #[arg(long)]
        max: Option<u32>,
    },
    /// Check this machine's readiness to serve or to reach tailscaled.
    Doctor {
        /// macOS: trigger the Screen Recording / Accessibility prompts if not yet granted.
        #[arg(long)]
        request_permissions: bool,
    },
    /// Install, remove or inspect the background service that runs `rdc serve`.
    Service {
        #[arg(value_enum)]
        op: ServiceOp,
    },
    /// List displays.
    Displays,
    /// List windows.
    Windows,
    /// Take a screenshot.
    Shot {
        #[arg(long, default_value = "all")]
        display: DisplayTarget,
        /// Downscale so the longer edge is at most this many pixels.
        #[arg(long)]
        max: Option<u32>,
        #[arg(long)]
        jpeg: bool,
        /// Output file (default: shot-<timestamp>.<ext>).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Move the pointer.
    Move { x: i32, y: i32 },
    /// Click at a point.
    Click {
        x: i32,
        y: i32,
        #[arg(long, default_value = "left")]
        button: MouseButton,
        #[arg(long)]
        double: bool,
    },
    /// Drag from one point to another.
    Drag {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        #[arg(long, default_value = "left")]
        button: MouseButton,
    },
    /// Scroll (positive dy = down, positive dx = right), optionally at a point.
    Scroll {
        #[arg(long, default_value_t = 0)]
        dx: i32,
        #[arg(long, default_value_t = 0)]
        dy: i32,
        #[arg(long, num_args = 2, value_names = ["X", "Y"])]
        at: Option<Vec<i32>>,
    },
    /// Type literal text.
    Type { text: String },
    /// Press a key chord like `cmd+shift+4`, `ctrl+c`, `enter`.
    Key { chord: String },
    /// Focus a window by id, app name or title substring.
    Focus {
        #[arg(long, conflicts_with_all = ["app", "title"])]
        id: Option<u64>,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    /// Read the clipboard, or set it when TEXT is given.
    Clip { text: Option<String> },
    /// Ask the target daemon who it thinks we are.
    Whoami,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ServiceOp {
    Install,
    Uninstall,
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(format!("{},enigo=error", cli.log)).unwrap_or_else(|_| "info,enigo=error".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cfg = config::load()?;

    match cli.cmd {
        Cmd::Serve { bind, port, allow, dev_loopback } => {
            let desktop: Arc<dyn Desktop> = Arc::new(LocalDesktop::new()?);
            let ts = tailscale::Tailscale::detect();
            let mut allow_all = cfg.serve.allow.clone();
            allow_all.extend(allow);
            let bind = bind.or_else(|| cfg.serve.bind.as_deref().and_then(|s| s.parse().ok()));
            server::serve(desktop, ts, server::ServeOpts { bind, port: port.unwrap_or(cfg.serve.port), allow: allow_all, dev_loopback }).await
        }
        Cmd::Mcp { max } => {
            let desktop: Arc<dyn Desktop> = match cfg.resolve_target(&cli.target)? {
                config::TargetKind::Local => Arc::new(LocalDesktop::new()?),
                config::TargetKind::Url(u) => Arc::new(RemoteDesktop::new(&u)?),
            };
            mcp::run(desktop, cli.target.clone(), max).await
        }
        Cmd::Service { op } => service::run(match op {
            ServiceOp::Install => service::Op::Install,
            ServiceOp::Uninstall => service::Op::Uninstall,
            ServiceOp::Status => service::Op::Status,
        }),
        Cmd::Doctor { request_permissions } => {
            let ok = doctor::run(request_permissions).await?;
            if !ok {
                std::process::exit(1);
            }
            Ok(())
        }
        other => client(&cfg, &cli.target, other).await,
    }
}

async fn client(cfg: &config::Config, target: &str, cmd: Cmd) -> Result<()> {
    let (desktop, remote): (Arc<dyn Desktop>, Option<Arc<RemoteDesktop>>) = match cfg.resolve_target(target)? {
        config::TargetKind::Local => (Arc::new(LocalDesktop::new()?), None),
        config::TargetKind::Url(u) => {
            let r = Arc::new(RemoteDesktop::new(&u)?);
            (r.clone(), Some(r))
        }
    };
    match cmd {
        Cmd::Displays => print_json(&desktop.displays().await?),
        Cmd::Windows => print_json(&desktop.windows().await?),
        Cmd::Shot { display, max, jpeg, out } => {
            let format = if jpeg { ImageFormat::Jpeg } else { ImageFormat::Png };
            let s = desktop.screenshot(ScreenshotReq { display, format, max_long_edge: max }).await?;
            let path = out.unwrap_or_else(|| {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                PathBuf::from(format!("shot-{t}.{}", format.ext()))
            });
            std::fs::write(&path, &s.data).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("{} {}x{} covers desktop {:?}", path.display(), s.width, s.height, s.rect);
            println!("{}", path.display());
            Ok(())
        }
        Cmd::Move { x, y } => desktop.input(InputAction::MouseMove { x, y }).await.map_err(Into::into),
        Cmd::Click { x, y, button, double } => desktop
            .input(InputAction::Click { x, y, button, count: if double { 2 } else { 1 } })
            .await
            .map_err(Into::into),
        Cmd::Drag { x1, y1, x2, y2, button } => desktop.input(InputAction::Drag { from: (x1, y1), to: (x2, y2), button }).await.map_err(Into::into),
        Cmd::Scroll { dx, dy, at } => desktop
            .input(InputAction::Scroll { at: at.map(|v| (v[0], v[1])), dx, dy })
            .await
            .map_err(Into::into),
        Cmd::Type { text } => desktop.input(InputAction::Type { text }).await.map_err(Into::into),
        Cmd::Key { chord } => {
            keys::parse_chord(&chord)?;
            desktop.input(InputAction::Key { chord }).await.map_err(Into::into)
        }
        Cmd::Focus { id, app, title } => {
            let t = match (id, app, title) {
                (Some(i), _, _) => WindowTarget::Id(i),
                (_, Some(a), _) => WindowTarget::App(a),
                (_, _, Some(t)) => WindowTarget::Title(t),
                _ => anyhow::bail!("give one of --id, --app, --title"),
            };
            desktop.focus(t).await.map_err(Into::into)
        }
        Cmd::Clip { text: Some(t) } => desktop.clipboard_set(t).await.map_err(Into::into),
        Cmd::Clip { text: None } => {
            println!("{}", desktop.clipboard_get().await?);
            Ok(())
        }
        Cmd::Whoami => match remote {
            Some(r) => print_json(&r.whoami().await?),
            None => anyhow::bail!("whoami needs a remote --target"),
        },
        Cmd::Serve { .. } | Cmd::Mcp { .. } | Cmd::Doctor { .. } | Cmd::Service { .. } => unreachable!(),
    }
}

fn print_json<T: serde::Serialize>(v: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}
