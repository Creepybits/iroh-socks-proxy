//! End-to-end tests for `--chat-app`: two apps on 127.0.0.1 (no relays, no internet),
//! driven over HTTP the way the page drives them.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use sovereign_daemon::chat_app;
use sovereign_daemon::p2p::{self, Accept, P2p, SecretKey};

const TOKEN: &str = "test-token";

/// Starts an app on a free port and returns its HTTP address.
async fn start(p2p: P2p) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(chat_app::serve(p2p, listener, TOKEN.to_string()));
    addr
}

/// Sends one request and returns the whole response.
async fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

/// An open `/events` stream.
struct Events(TcpStream, String);

impl Events {
    async fn open(addr: SocketAddr) -> Self {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let raw = format!("GET /events?t={TOKEN} HTTP/1.1\r\nHost: {addr}\r\n\r\n");
        stream.write_all(raw.as_bytes()).await.unwrap();
        Events(stream, String::new())
    }

    /// Reads until `text` has arrived, failing after 10 seconds.
    async fn wait_for(&mut self, text: &str) {
        let mut buf = [0u8; 1024];
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.1.contains(text) {
                let n = self.0.read(&mut buf).await.unwrap();
                assert!(n > 0, "stream ended; got: {}", self.1);
                self.1.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never got {text:?}; got: {}", self.1));
    }
}

#[tokio::test]
async fn two_apps_chat_and_hang_up() {
    let alice = P2p::bind_local(SecretKey::generate(), Accept::Chat)
        .await
        .unwrap();
    let bob = P2p::bind_local(SecretKey::generate(), Accept::Chat)
        .await
        .unwrap();
    let (alice_id, bob_id) = (alice.id(), bob.id());
    bob.add_peer_addr(alice_id, alice.local_addrs()[0]);
    let alice_http = start(alice).await;
    let bob_http = start(bob).await;

    let mut alice_events = Events::open(alice_http).await;
    alice_events
        .wait_for(&format!(
            "event: me\ndata: {}",
            p2p::anon_address(&alice_id)
        ))
        .await;
    alice_events.wait_for("event: state\ndata: idle").await;
    let mut bob_events = Events::open(bob_http).await;
    bob_events.wait_for("data: idle").await;

    // Bob calls Alice; the `.anon` ending is optional.
    let call = request(
        bob_http,
        "POST",
        &format!("/call?t={TOKEN}"),
        &alice_id.to_z32(),
    )
    .await;
    assert!(call.starts_with("HTTP/1.1 200"), "got: {call}");
    alice_events
        .wait_for(&format!("data: chatting {}", p2p::anon_address(&bob_id)))
        .await;
    bob_events
        .wait_for(&format!("data: chatting {}", p2p::anon_address(&alice_id)))
        .await;

    // A message with a terminal escape and a newline arrives cleaned, as one line.
    let send = request(
        bob_http,
        "POST",
        &format!("/send?t={TOKEN}"),
        "hej \x1b[31mAlice\nhow are you",
    )
    .await;
    assert!(send.starts_with("HTTP/1.1 200"), "got: {send}");
    let bob_name = p2p::short_id(&bob_id);
    alice_events
        .wait_for(&format!(
            "event: msg\ndata: {bob_name}: hej \u{FFFD}[31mAlice how are you\n\n"
        ))
        .await;

    request(alice_http, "POST", &format!("/send?t={TOKEN}"), "fine!").await;
    bob_events
        .wait_for(&format!("data: {}: fine!", p2p::short_id(&alice_id)))
        .await;

    // Bob hangs up: Alice sees him leave, and both are free again.
    request(bob_http, "POST", &format!("/hangup?t={TOKEN}"), "").await;
    alice_events
        .wait_for(&format!("data: {bob_name} has left."))
        .await;
    let idle_again = alice_events.1.matches("data: idle").count();
    assert!(idle_again >= 1);
    alice_events.1.clear();
    alice_events.wait_for("data: idle").await;
    bob_events.1.clear();
    bob_events.wait_for("data: idle").await;
}

#[tokio::test]
async fn wrong_token_is_refused() {
    let app = P2p::bind_local(SecretKey::generate(), Accept::Chat)
        .await
        .unwrap();
    let http = start(app).await;
    for (method, path) in [
        ("GET", "/"),
        ("GET", "/?t=nope"),
        ("GET", "/events?t=nope"),
        ("POST", "/send?t=nope"),
        ("POST", "/call?t=nope"),
    ] {
        let response = request(http, method, path, "").await;
        assert!(
            response.starts_with("HTTP/1.1 403"),
            "{method} {path} got: {response}"
        );
    }
    let page = request(http, "GET", &format!("/?t={TOKEN}"), "").await;
    assert!(page.starts_with("HTTP/1.1 200"), "got: {page}");
    assert!(page.contains("Sovereign Chat"));
}

#[tokio::test]
async fn bad_address_and_sending_while_idle_are_errors() {
    let app = P2p::bind_local(SecretKey::generate(), Accept::Chat)
        .await
        .unwrap();
    let http = start(app).await;
    let call = request(http, "POST", &format!("/call?t={TOKEN}"), "testsite.anon").await;
    assert!(call.starts_with("HTTP/1.1 400"), "got: {call}");
    let send = request(http, "POST", &format!("/send?t={TOKEN}"), "hello?").await;
    assert!(send.starts_with("HTTP/1.1 409"), "got: {send}");
}
