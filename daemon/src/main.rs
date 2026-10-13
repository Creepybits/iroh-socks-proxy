use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;

use sovereign_daemon::p2p::{self, P2p};
use sovereign_daemon::server;
use sovereign_daemon::site::{self, Site};

/// Local SOCKS5 proxy for the Sovereign Web.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Address and port to listen on. Keep this on 127.0.0.1 unless you know
    /// why you need anything else.
    #[arg(long, default_value = "127.0.0.1:9999")]
    listen: SocketAddr,

    /// Folder for the daemon's secret key (its identity). Defaults to the
    /// system's app data folder, e.g. ~/.local/share/sovereign-web on Linux.
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Publish this folder as your .anon site (index.html is the home page).
    #[arg(long, value_name = "FOLDER", conflicts_with = "forward")]
    serve: Option<PathBuf>,

    /// Publish a web server already running on this machine as your .anon site,
    /// e.g. 127.0.0.1:8000. Only 127.0.0.1 / [::1] addresses are allowed.
    #[arg(long, value_name = "ADDRESS")]
    forward: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let data_dir = match args.data_dir {
        Some(dir) => dir,
        None => dirs::data_dir()
            .ok_or("could not find the app data folder; pass --data-dir")?
            .join("sovereign-web"),
    };
    let secret_key = p2p::load_or_create_secret_key(&data_dir).map_err(|e| {
        format!(
            "could not load the secret key from {}: {}",
            data_dir.display(),
            e
        )
    })?;

    // Check the site settings before anything starts, so mistakes fail fast.
    let site = match (&args.serve, args.forward) {
        (Some(folder), _) => Some(
            Site::folder(folder).map_err(|e| format!("can't serve {}: {}", folder.display(), e))?,
        ),
        (None, Some(addr)) => Some(Site::forward(addr)?),
        (None, None) => None,
    };

    let listener = TcpListener::bind(args.listen).await?;
    let bound = listener.local_addr()?;
    println!("⚡ Sovereign daemon bound strictly to {}", bound);
    if !bound.ip().is_loopback() {
        println!(
            "⚠️  Warning: {} is not a loopback address; other machines may be able to use this proxy.",
            bound.ip()
        );
    }
    println!("🛡️  Air-Gap Mode Active: Non-.anon traffic will be instantly dropped.");

    let p2p = match P2p::bind_public(secret_key, site.is_some()).await {
        Ok(p2p) => {
            let p2p = Arc::new(p2p);
            println!("🔑 Your .anon address: {}", p2p::anon_address(&p2p.id()));
            match site {
                Some(site) => {
                    match &site {
                        Site::Folder(root) => {
                            println!("📂 Publishing {} at that address.", root.display())
                        }
                        Site::Forward(addr) => {
                            println!("📡 Publishing http://{} at that address.", addr)
                        }
                    }
                    tokio::spawn(site::serve_tunnels(p2p.clone(), Arc::new(site)));
                }
                None => {
                    println!("   (Not publishing a site; use --serve <folder> to publish one.)")
                }
            }
            Some(p2p)
        }
        Err(e) => {
            println!(
                "⚠️  Iroh failed to start ({}); only testsite.anon will work.",
                e
            );
            None
        }
    };

    server::run(listener, p2p).await;
    Ok(())
}
