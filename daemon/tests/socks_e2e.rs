//! Starts the real daemon on a free local port and talks SOCKS5 to it.
//! Nothing here touches the internet: blocked hosts are refused before any
//! outgoing connection would be made.

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use sovereign_daemon::server;
use sovereign_daemon::socks::{
    ADDR_TYPE_DOMAIN, ADDR_TYPE_IPV4, REP_HOST_UNREACHABLE, REP_SUCCESS,
};

async fn start_daemon() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::run(listener));
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

async fn fetch_landing_page(name: &str) -> String {
    let daemon = start_daemon().await;
    let (mut stream, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain(name), 80).await;
    assert_eq!(rep, REP_SUCCESS);

    let request = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", name);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn anon_host_gets_landing_page() {
    let response = fetch_landing_page("testsite.anon").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("Welcome to the Sovereign Web"));
}

#[tokio::test]
async fn uppercase_anon_host_gets_landing_page() {
    let response = fetch_landing_page("TEST.ANON").await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
}

#[tokio::test]
async fn clearnet_domain_is_unreachable() {
    let daemon = start_daemon().await;
    let (_, rep) = socks_connect(daemon, ADDR_TYPE_DOMAIN, &domain("example.com"), 443).await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}

#[tokio::test]
async fn clearnet_ip_is_unreachable() {
    let daemon = start_daemon().await;
    let (_, rep) = socks_connect(daemon, ADDR_TYPE_IPV4, &[93, 184, 216, 34], 80).await;
    assert_eq!(rep, REP_HOST_UNREACHABLE);
}
