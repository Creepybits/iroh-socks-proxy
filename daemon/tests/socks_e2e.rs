//! Starts the real daemon on a free local port and talks SOCKS5 to it.
//! Nothing here touches the internet: blocked hosts are refused before any
//! outgoing connection would be made, and Iroh endpoints only use 127.0.0.1
//! with relays and address lookup services turned off.

use std::net::SocketAddr;
use std::sync::Arc;

use iroh::SecretKey;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use sovereign_daemon::p2p::{self, Accept, P2p};
use sovereign_daemon::server;
use sovereign_daemon::site::{self, Site};
use sovereign_daemon::socks::{
    ADDR_TYPE_DOMAIN, ADDR_TYPE_IPV4, REP_HOST_UNREACHABLE, REP_SUCCESS,
};

async fn start_daemon(p2p: Option<Arc<P2p>>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::run(listener, p2p));
    addr
}

/// Does the greeting and a CONNECT, returns the stream and the reply code.
async fn socks_connect(daemon: SocketAddr, atyp: u8, addr: &[u8], port: u16) -> (TcpStream, u8) {
    let mut stream = TcpStream::connect(daemon).await.unwrap();

    stream.write_all(&[5, 1, 0]).await.unwrap();
    let mut method = [0u8; 2];
    stream.read_exact(&mut method).await.unwrap();
    assert_eq!(method, [5, 0]);

    let mut request = vec![5, 1, 0, atyp];
    request.extend_from_slice(addr);
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[0], 5);
    (stream, reply[1])
}

fn domain(name: &str) -> Vec<u8> {
    let mut bytes = vec![name.len() as u8];
    bytes.extend_from_slice(name.as_bytes());
    bytes
}

/// Sends a GET for `name` through the daemon and returns the whole response.
async fn http_get(daemon: SocketAddr, name: &str) -> String {
    let (mut stream, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain(name), 80).await;
    assert_eq!(rep, REP_SUCCESS);

    let request = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", name);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn testsite_gets_landing_page() {
    let daemon = start_daemon(None).await;
    let response = http_get(daemon, "testsite.anon").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("Welcome to the Sovereign Web"));
}

#[tokio::test]
async fn uppercase_testsite_gets_landing_page() {
    let daemon = start_daemon(None).await;
    let response = http_get(daemon, "TESTSITE.ANON").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
}

#[tokio::test]
async fn clearnet_domain_is_unreachable() {
    let daemon = start_daemon(None).await;
    let (_, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain("example.com"), 443).await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}

#[tokio::test]
async fn clearnet_ip_is_unreachable() {
    let daemon = start_daemon(None).await;
    let (_, rep) = socks_connect(daemon, ADDR_TYPE_IPV4, &[93, 184, 216, 34], 80).await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}

#[tokio::test]
async fn invalid_anon_name_is_unreachable() {
    let daemon = start_daemon(None).await;
    let (_, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain("nokey.anon"), 80).await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}

/// A tiny "web server" peer: answers each tunnel with a fixed page that echoes
/// back the request line, so the test can see the request arrived intact.
fn spawn_peer(peer: Arc<P2p>) {
    tokio::spawn(async move {
        while let Some(incoming) = peer.next_incoming().await {
            tokio::spawn(async move {
                let mut tunnel = incoming.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = tunnel.read(&mut buf).await.unwrap();
                    assert!(n > 0, "peer saw EOF before the end of the headers");
                    request.extend_from_slice(&buf[..n]);
                }
                let first_line = String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let body = format!("hello from the peer, you asked: {}", first_line);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                tunnel.write_all(response.as_bytes()).await.unwrap();
                tunnel.shutdown().await.unwrap();
                tunnel.closed().await;
            });
        }
    });
}

#[tokio::test]
async fn http_request_is_tunnelled_to_peer() {
    let peer = Arc::new(
        P2p::bind_local(SecretKey::generate(), Accept::Tunnels)
            .await
            .unwrap(),
    );
    let local = Arc::new(
        P2p::bind_local(SecretKey::generate(), Accept::Nothing)
            .await
            .unwrap(),
    );
    local.add_peer_addr(peer.id(), peer.local_addrs()[0]);
    spawn_peer(peer.clone());

    let daemon = start_daemon(Some(local)).await;
    let response = http_get(daemon, &p2p::anon_address(&peer.id())).await;

    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {}", response);
    assert!(response.ends_with("hello from the peer, you asked: GET / HTTP/1.1"));
}

#[tokio::test]
async fn unknown_peer_is_unreachable() {
    let local = Arc::new(
        P2p::bind_local(SecretKey::generate(), Accept::Nothing)
            .await
            .unwrap(),
    );
    let daemon = start_daemon(Some(local)).await;
    let nobody = SecretKey::generate().public();
    let (_, rep) = socks_connect(
        daemon,
        ADDR_TYPE_DOMAIN,
        &domain(&p2p::anon_address(&nobody)),
        80,
    )
    .await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}

/// Two daemons: one publishes a folder with `--serve`, the other visits it
/// through its SOCKS proxy, like a browser would.
#[tokio::test]
async fn served_folder_is_reachable_through_tunnel() {
    let tmp = std::env::temp_dir().join(format!("sovereign-e2e-site-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("site")).unwrap();
    std::fs::write(tmp.join("site/index.html"), "<h1>my anon site</h1>").unwrap();
    std::fs::write(tmp.join("secret"), "TOP SECRET").unwrap();

    let publisher = Arc::new(
        P2p::bind_local(SecretKey::generate(), Accept::Tunnels)
            .await
            .unwrap(),
    );
    let site = Arc::new(Site::folder(&tmp.join("site")).unwrap());
    tokio::spawn(site::serve_tunnels(publisher.clone(), site));

    let visitor = Arc::new(
        P2p::bind_local(SecretKey::generate(), Accept::Nothing)
            .await
            .unwrap(),
    );
    visitor.add_peer_addr(publisher.id(), publisher.local_addrs()[0]);
    let daemon = start_daemon(Some(visitor)).await;
    let host = p2p::anon_address(&publisher.id());

    let home = http_get(daemon, &host).await;
    assert!(home.starts_with("HTTP/1.1 200 OK"), "got: {}", home);
    assert!(home.ends_with("<h1>my anon site</h1>"));

    let (mut stream, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain(&host), 80).await;
    assert_eq!(rep, REP_SUCCESS);
    stream
        .write_all(b"GET /../secret HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 404"), "got: {}", response);
    assert!(!response.contains("TOP SECRET"));

    std::fs::remove_dir_all(&tmp).unwrap();
}

/// A client that connects and then says nothing must not hold a slot forever.
#[tokio::test]
async fn silent_client_is_disconnected() {
    let daemon = start_daemon(None).await;
    let mut stream = TcpStream::connect(daemon).await.unwrap();
    tokio::time::pause(); // fast-forward through the 10 s handshake timeout
    let mut buf = [0u8; 1];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    assert_eq!(n, 0, "daemon should close the connection");
}
