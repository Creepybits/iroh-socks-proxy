//! Chat in the browser (`--chat-app`): the same chat as `--chat`, with a web page
//! instead of the terminal.
//!
//! In plain terms: the daemon serves a small chat page on 127.0.0.1 only and opens it
//! in your browser. The page passes what you type to the daemon, and the daemon sends
//! it to the other side over the same encrypted Iroh chat stream as `--chat` (so an
//! app user and a terminal user can chat with each other). The page never talks to
//! the internet; the plain HTTP between page and daemon stays inside this computer.
//!
//! Other websites open in the same browser can also reach 127.0.0.1, so every request
//! must carry a random secret token that only the page's own URL contains.
//!
//! The page talks to the daemon with:
//! - `GET /events`: a stream of server-sent events (`me`, `state`, `msg`, `status`).
//! - `POST /call` (body: a `.anon` address), `POST /send` (body: one message),
//!   `POST /hangup`.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

use crate::chat::{self, MAX_LINE};
use crate::p2p::{self, Accept, Error, P2p, PeerId, SecretKey, Tunnel};

const PAGE: &str = include_str!("chat_app.html");

/// Request headers bigger than this are refused.
const MAX_HEADER_BYTES: usize = 8 * 1024;
/// Request bodies bigger than this are refused (a message is at most `MAX_LINE`).
const MAX_BODY_BYTES: usize = MAX_LINE * 2;
/// How long the page may take to send a whole request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the chat app: joins the Iroh network, serves the page on a free port on
/// 127.0.0.1 and opens it in the browser.
pub async fn run(secret_key: SecretKey) -> Result<(), Error> {
    let p2p = P2p::bind_public(secret_key, Accept::Chat).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let token = p2p::random_token();
    let url = format!("http://{}/?t={}", listener.local_addr()?, token);
    println!("🔑 Your .anon address: {}", p2p::anon_address(&p2p.id()));
    println!("💬 Chat app: {}", url);
    println!("   (Opening it in your browser. Keep this window open; Ctrl+C quits.)");
    open_in_browser(&url);
    serve(p2p, listener, token).await;
    Ok(())
}

/// Serves the chat page and its requests on `listener`, guarded by `token`, and
/// runs the chats. Returns when the endpoint closes.
pub async fn serve(p2p: P2p, listener: TcpListener, token: String) {
    let (calls_tx, calls_rx) = mpsc::channel(1);
    let app = Arc::new(App {
        p2p,
        token,
        state: Mutex::new(State::Idle),
        events: broadcast::channel(256).0,
        calls: calls_tx,
    });
    tokio::spawn(accept_page_requests(app.clone(), listener));
    app.chats(calls_rx).await;
}

struct App {
    p2p: P2p,
    token: String,
    state: Mutex<State>,
    /// Events for every open page, already formatted as server-sent events.
    events: broadcast::Sender<String>,
    /// Addresses the page asked us to call.
    calls: mpsc::Sender<PeerId>,
}

enum State {
    Idle,
    Calling(PeerId),
    Chatting {
        peer: PeerId,
        /// Messages typed in the page. `None` after hanging up.
        input: Option<mpsc::Sender<String>>,
    },
}

impl App {
    /// Waits for callers and calls out when asked, one chat at a time (like `--chat`).
    async fn chats(&self, mut calls: mpsc::Receiver<PeerId>) {
        loop {
            let tunnel = tokio::select! {
                incoming = self.p2p.next_incoming() => {
                    let Some(incoming) = incoming else { return };
                    match incoming.accept().await {
                        Ok(tunnel) => tunnel,
                        Err(e) => {
                            self.status(&format!("Someone tried to connect but failed: {}", e));
                            continue;
                        }
                    }
                }
                Some(peer) = calls.recv() => {
                    self.set_state(State::Calling(peer));
                    match chat::call(&self.p2p, peer).await {
                        Ok(tunnel) => tunnel,
                        Err(e) => {
                            self.set_state(State::Idle);
                            self.status(&format!(
                                "Could not reach that address ({}). Is the other side running the chat and online?",
                                e
                            ));
                            continue;
                        }
                    }
                }
            };
            self.converse(tunnel).await;
            self.set_state(State::Idle);
        }
    }

    /// One conversation, run by [`chat::converse`] with the page as keyboard and screen.
    async fn converse(&self, tunnel: Tunnel) {
        let (input_tx, mut input_rx) = mpsc::channel::<String>(32);
        self.set_state(State::Chatting {
            peer: tunnel.peer(),
            input: Some(input_tx),
        });

        // Typed messages become lines of input. Hanging up drops the sender, which
        // ends the input, just like Ctrl+D in the terminal.
        let (mut typed, input) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            while let Some(text) = input_rx.recv().await {
                if typed
                    .write_all(format!("{}\n", text).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Each line `converse` would print becomes a `msg` event for the page.
        let (mut screen, shown) = tokio::io::duplex(64 * 1024);
        let events = self.events.clone();
        let show = tokio::spawn(async move {
            let mut lines = BufReader::new(shown).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = events.send(sse("msg", &line));
            }
        });

        let mut input = BufReader::new(input).lines();
        if let Err(e) = chat::converse(tunnel, &mut input, &mut screen).await {
            self.status(&format!("Chat ended with an error: {}", e));
        }
        drop(screen);
        let _ = show.await;
    }

    fn set_state(&self, state: State) {
        let event = state_event(&state);
        *self.state.lock().unwrap() = state;
        let _ = self.events.send(event);
    }

    fn status(&self, text: &str) {
        let _ = self.events.send(sse("status", text));
    }
}

fn state_event(state: &State) -> String {
    match state {
        State::Idle => sse("state", "idle"),
        State::Calling(peer) => sse("state", &format!("calling {}", p2p::anon_address(peer))),
        State::Chatting { peer, .. } => {
            sse("state", &format!("chatting {}", p2p::anon_address(peer)))
        }
    }
}

/// One server-sent event. `data` must be a single line.
fn sse(event: &str, data: &str) -> String {
    format!("event: {}\ndata: {}\n\n", event, data)
}

async fn accept_page_requests(app: Arc<App>, listener: TcpListener) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let app = app.clone();
        tokio::spawn(async move {
            let _ = handle_request(&app, stream).await;
        });
    }
}

async fn handle_request(app: &App, mut stream: TcpStream) -> io::Result<()> {
    let request = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(Some(request))) => request,
        Ok(Ok(None)) => return respond(&mut stream, 413, "text/plain", "Too large").await,
        Ok(Err(e)) => return Err(e),
        Err(_) => return respond(&mut stream, 408, "text/plain", "Too slow").await,
    };
    if request.token.as_deref() != Some(app.token.as_str()) {
        return respond(&mut stream, 403, "text/plain", "Wrong or missing token").await;
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => respond(&mut stream, 200, "text/html; charset=utf-8", PAGE).await,
        ("GET", "/events") => send_events(app, stream).await,
        ("POST", "/call") => {
            let peer = match chat::parse_address(&request.body) {
                Ok(peer) => peer,
                Err(e) => return respond(&mut stream, 400, "text/plain", &e).await,
            };
            if peer == app.p2p.id() {
                return respond(&mut stream, 400, "text/plain", "That is your own address").await;
            }
            let idle = matches!(*app.state.lock().unwrap(), State::Idle);
            if !idle || app.calls.try_send(peer).is_err() {
                return respond(&mut stream, 409, "text/plain", "Already in a chat").await;
            }
            respond(&mut stream, 200, "text/plain", "Calling").await
        }
        ("POST", "/send") => {
            // One message is one line: newlines typed in the page become spaces.
            let text = request.body.replace(['\r', '\n'], " ");
            let input = match &*app.state.lock().unwrap() {
                State::Chatting { input, .. } => input.clone(),
                _ => None,
            };
            match input {
                Some(input) if input.try_send(text).is_ok() => {
                    respond(&mut stream, 200, "text/plain", "Sent").await
                }
                _ => respond(&mut stream, 409, "text/plain", "Not in a chat").await,
            }
        }
        ("POST", "/hangup") => {
            if let State::Chatting { input, .. } = &mut *app.state.lock().unwrap() {
                *input = None;
            }
            respond(&mut stream, 200, "text/plain", "Hanging up").await
        }
        _ => respond(&mut stream, 404, "text/plain", "Not found").await,
    }
}

/// Streams events to one open page until it closes.
async fn send_events(app: &App, mut stream: TcpStream) -> io::Result<()> {
    // Subscribe first, so nothing that happens while we send the header is missed.
    let mut events = app.events.subscribe();
    let first = {
        let state = app.state.lock().unwrap();
        format!(
            "{}{}",
            sse("me", &p2p::anon_address(&app.p2p.id())),
            state_event(&state)
        )
    };
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n",
        )
        .await?;
    stream.write_all(first.as_bytes()).await?;
    loop {
        match events.recv().await {
            Ok(event) => stream.write_all(event.as_bytes()).await?,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

struct Request {
    method: String,
    path: String,
    token: Option<String>,
    body: String,
}

/// Reads one request: headers, then a body of `Content-Length` bytes. `None` if the
/// headers or body are too big.
async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<Option<Request>> {
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    let head_end = loop {
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if data.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        data.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();

    let mut lines = head.lines();
    let mut parts = lines.next().unwrap_or_default().split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let token = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("t="))
        .map(str::to_string);

    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Ok(None);
    }
    let mut body = data.split_off(head_end);
    while body.len() < length {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(length);

    Ok(Some(Request {
        method,
        path: path.to_string(),
        token,
        body: String::from_utf8_lossy(&body).into_owned(),
    }))
}

async fn respond<S: AsyncWrite + Unpin>(
    stream: &mut S,
    code: u16,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Referrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; \
         style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'\r\n\
         Connection: close\r\n\r\n{}",
        code,
        if code == 200 { "OK" } else { "Error" },
        content_type,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

/// Best effort; if it fails, the user can open the printed address by hand.
fn open_in_browser(url: &str) {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let result = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();
    if result.is_err() {
        println!("   Could not open the browser; copy the address above into it.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_token_and_body() {
        let raw: &[u8] =
            b"POST /send?t=abc HTTP/1.1\r\nHost: x\r\nContent-Length: 6\r\n\r\nhej \xc3\xa5";
        let mut stream = raw;
        let request = read_request(&mut stream).await.unwrap().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/send");
        assert_eq!(request.token.as_deref(), Some("abc"));
        assert_eq!(request.body, "hej å");
    }

    #[tokio::test]
    async fn refuses_huge_bodies() {
        let raw = format!(
            "POST /send?t=abc HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        let mut stream = raw.as_bytes();
        assert!(read_request(&mut stream).await.unwrap().is_none());
    }
}
