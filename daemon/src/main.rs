use std::net::SocketAddr;

use clap::Parser;
use tokio::net::TcpListener;

use sovereign_daemon::server;

/// Local SOCKS5 proxy for the Sovereign Web.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Address and port to listen on. Keep this on 127.0.0.1 unless you know
    /// why you need anything else.
    #[arg(long, default_value = "127.0.0.1:9999")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

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

    server::run(listener).await;
    Ok(())
}
