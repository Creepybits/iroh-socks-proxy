//! Accepts browser connections and handles each one.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::p2p::P2p;
use crate::router::{self, Route};
use crate::socks::{self, REP_HOST_UNREACHABLE, REP_SUCCESS, SocksError};

const LANDING_PAGE: &str = "HTTP/1.1 200 OK\r\n\
    Content-Type: text/html; charset=utf-8\r\n\
    Connection: close\r\n\r\n\
    <!DOCTYPE html><html><head><title>Sovereign Web</title><style>body { background-color: #0f172a; color: #a855f7; font-family: system-ui, sans-serif; text-align: center; padding-top: 100px; } h1 { font-size: 2.5rem; } p { color: #94a3b8; }</style></head>\
    <body><h1>⚡ Welcome to the Sovereign Web ⚡</h1><p>Rendering decentralized content from <strong>.anon</strong> cryptographic storage.</p></body></html>";

/// Serves connections from `listener` until the task is cancelled. Without `p2p`
/// (Iroh failed to start), key addresses are refused like any other blocked host.
pub async fn run(listener: TcpListener, p2p: Option<Arc<P2p>>) {
    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        let p2p = p2p.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, p2p.as_deref()).await {
                eprintln!("Connection closed [{}]: {}", client_addr, e);
            }
        });
    }
}

async fn handle_client(mut stream: TcpStream, p2p: Option<&P2p>) -> Result<(), SocksError> {
    let target = socks::handshake(&mut stream).await?;

    match router::route(&target.host) {
        Route::Landing => {
            println!("⚡ [Sovereign Domain Resolved]: {}", target);
            socks::send_reply(&mut stream, REP_SUCCESS).await?;

            // Wait for the browser's HTTP request; its contents are ignored.
            let mut request = [0u8; 1024];
            if stream.read(&mut request).await? == 0 {
                return Ok(());
            }
            stream.write_all(LANDING_PAGE.as_bytes()).await?;
            stream.shutdown().await?;
        }
        Route::Tunnel(peer) => {
            let Some(p2p) = p2p else {
                println!("🔒 [Tunnel Unavailable]: {} (Iroh not running)", target);
                socks::send_reply(&mut stream, REP_HOST_UNREACHABLE).await?;
                return Ok(());
            };
            println!("🌐 [Tunnel Opening]: {}", target);
            let mut tunnel = match p2p.open_tunnel(peer).await {
                Ok(tunnel) => tunnel,
                Err(e) => {
                    println!("🔒 [Peer Unreachable]: {} ({})", target, e);
                    socks::send_reply(&mut stream, REP_HOST_UNREACHABLE).await?;
                    return Ok(());
                }
            };
            socks::send_reply(&mut stream, REP_SUCCESS).await?;
            tokio::io::copy_bidirectional(&mut stream, &mut tunnel).await?;
        }
        Route::Blocked => {
            println!("🔒 [Clearnet Blocked]: {} (Access Denied)", target);
            socks::send_reply(&mut stream, REP_HOST_UNREACHABLE).await?;
        }
    }
    Ok(())
}
