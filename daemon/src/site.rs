//! Answers incoming tunnels: either a minimal static file server for one folder
//! (`--serve`), or a byte-for-byte forward to a web server on this machine
//! (`--forward`).
//!
//! The file server speaks just enough HTTP/1.1 for a browser: one GET per
//! connection, `Connection: close`, `index.html` as the default page.

use std::io;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::p2p::{self, P2p};

/// Request headers bigger than this are refused.
const MAX_HEADER_BYTES: usize = 8 * 1024;
/// How long a peer may take to send its request headers.
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// After answering, how long to wait for the peer to hang up before we do.
const LINGER: Duration = Duration::from_secs(10);

/// Answers every tunnel peers open to us with `site`, until the endpoint closes.
pub async fn serve_tunnels(p2p: Arc<P2p>, site: Arc<Site>) {
    while let Some(incoming) = p2p.next_incoming().await {
        let site = site.clone();
        tokio::spawn(async move {
            let mut tunnel = match incoming.accept().await {
                Ok(tunnel) => tunnel,
                Err(e) => {
                    eprintln!("Incoming tunnel failed: {}", e);
                    return;
                }
            };
            let peer = p2p::short_id(&tunnel.peer());
            if let Err(e) = site.handle(&mut tunnel).await {
                eprintln!("Tunnel from {} closed: {}", peer, e);
            }
            // Let the last bytes reach the peer before the tunnel is dropped.
            let _ = tokio::time::timeout(LINGER, tunnel.closed()).await;
        });
    }
}

/// What to do with incoming tunnels.
#[derive(Debug, Clone)]
pub enum Site {
    /// Serve files from this folder (already canonicalized).
    Folder(PathBuf),
    /// Forward to a web server on this machine.
    Forward(SocketAddr),
}

impl Site {
    /// Checks the folder exists and remembers its absolute path.
    pub fn folder(path: &Path) -> io::Result<Self> {
        let root = path.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a folder", root.display()),
            ));
        }
        Ok(Site::Folder(root))
    }

    /// Only loopback targets are allowed, so a site can't turn the daemon into a
    /// gateway to other machines or the internet.
    pub fn forward(addr: SocketAddr) -> io::Result<Self> {
        if !addr.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "--forward must point at this machine (127.0.0.1 or [::1]), not {}",
                    addr
                ),
            ));
        }
        Ok(Site::Forward(addr))
    }

    /// Answers one tunnel.
    pub async fn handle<S>(&self, tunnel: &mut S) -> io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        match self {
            Site::Folder(root) => serve_file(root, tunnel).await,
            Site::Forward(addr) => {
                let mut upstream = TcpStream::connect(addr).await?;
                tokio::io::copy_bidirectional(tunnel, &mut upstream).await?;
                Ok(())
            }
        }
    }
}

async fn serve_file<S>(root: &Path, stream: &mut S) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let head = match tokio::time::timeout(HEADER_TIMEOUT, read_head(stream)).await {
        Ok(Ok(Some(head))) => head,
        Ok(Ok(None)) => return send_error(stream, 431, "Request Header Fields Too Large").await,
        Ok(Err(e)) => return Err(e),
        Err(_) => return send_error(stream, 408, "Request Timeout").await,
    };

    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let (method, target) = (parts.next().unwrap_or_default(), parts.next());
    let Some(target) = target else {
        return send_error(stream, 400, "Bad Request").await;
    };
    if method != "GET" {
        return send_error(stream, 405, "Method Not Allowed").await;
    }

    let Some(path) = resolve_path(root, target) else {
        return send_error(stream, 404, "Not Found").await;
    };
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(_) => return send_error(stream, 404, "Not Found").await,
    };
    let len = file.metadata().await?.len();
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        content_type(&path),
        len
    );
    stream.write_all(header.as_bytes()).await?;
    tokio::io::copy(&mut file, stream).await?;
    stream.shutdown().await
}

/// Reads up to the blank line ending the request headers. `None` if they are too big.
async fn read_head<S>(stream: &mut S) -> io::Result<Option<String>>
where
    S: AsyncRead + Unpin,
{
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        if head.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        head.extend_from_slice(&buf[..n]);
    }
    Ok(Some(String::from_utf8_lossy(&head).into_owned()))
}

async fn send_error<S>(stream: &mut S, code: u16, reason: &str) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let body = format!("{} {}\n", code, reason);
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        code,
        reason,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

/// Maps a request target like `/blog/post.html?x=1` to a file inside `root`.
/// Returns `None` for anything that could escape the folder: `..` parts (also
/// percent-encoded), backslashes, drive letters, or symlinks pointing outside.
pub fn resolve_path(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next()?;
    let decoded = percent_decode(path)?;
    if decoded.contains('\\') || decoded.contains('\0') {
        return None;
    }

    let mut full = root.to_path_buf();
    for part in decoded.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            _ => {
                // Reject anything the OS would treat as more than a plain name,
                // e.g. "C:" on Windows.
                let mut components = Path::new(part).components();
                match (components.next(), components.next()) {
                    (Some(Component::Normal(_)), None) => full.push(part),
                    _ => return None,
                }
            }
        }
    }

    if full.is_dir() {
        full.push("index.html");
    }
    // Resolves symlinks; the result must still be inside the served folder.
    let real = full.canonicalize().ok()?;
    if real.starts_with(root) && real.is_file() {
        Some(real)
    } else {
        None
    }
}

/// Decodes `%XX` escapes. `None` if an escape is malformed or the result isn't UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// A served folder `<tmp>/site` with a secret file next to it in `<tmp>`.
    struct Fixture {
        tmp: PathBuf,
        site: Site,
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let tmp = std::env::temp_dir().join(format!(
                "sovereign-site-{}-{}",
                name,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(tmp.join("site/blog")).unwrap();
            std::fs::write(tmp.join("site/index.html"), "<h1>home</h1>").unwrap();
            std::fs::write(tmp.join("site/blog/index.html"), "<h1>blog</h1>").unwrap();
            std::fs::write(tmp.join("site/style.css"), "body{}").unwrap();
            std::fs::write(tmp.join("site/my file.txt"), "spaces").unwrap();
            std::fs::write(tmp.join("secret"), "TOP SECRET").unwrap();
            let site = Site::folder(&tmp.join("site")).unwrap();
            let Site::Folder(root) = site.clone() else {
                unreachable!()
            };
            Fixture { tmp, site, root }
        }

        async fn request(&self, raw: &str) -> String {
            let (mut client, mut server) = duplex(64 * 1024);
            client.write_all(raw.as_bytes()).await.unwrap();
            self.site.handle(&mut server).await.unwrap();
            drop(server);
            let mut response = String::new();
            client.read_to_string(&mut response).await.unwrap();
            response
        }

        async fn get(&self, path: &str) -> String {
            self.request(&format!("GET {} HTTP/1.1\r\nHost: x.anon\r\n\r\n", path))
                .await
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }

    #[tokio::test]
    async fn serves_index_and_files() {
        let f = Fixture::new("serve");
        let home = f.get("/").await;
        assert!(home.starts_with("HTTP/1.1 200 OK"));
        assert!(home.contains("text/html"));
        assert!(home.ends_with("<h1>home</h1>"));
        assert!(f.get("/blog/").await.ends_with("<h1>blog</h1>"));
        assert!(f.get("/blog").await.ends_with("<h1>blog</h1>"));
        assert!(f.get("/style.css?v=2").await.contains("text/css"));
        assert!(f.get("/my%20file.txt").await.ends_with("spaces"));
    }

    #[tokio::test]
    async fn missing_file_is_404() {
        let f = Fixture::new("missing");
        assert!(f.get("/nope.html").await.starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn only_get_is_allowed() {
        let f = Fixture::new("post");
        let response = f.request("POST / HTTP/1.1\r\nHost: x.anon\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 405"));
    }

    #[tokio::test]
    async fn huge_headers_are_refused() {
        let f = Fixture::new("huge");
        let raw = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(20_000));
        assert!(f.request(&raw).await.starts_with("HTTP/1.1 431"));
    }

    #[tokio::test]
    async fn directory_traversal_does_not_escape() {
        let f = Fixture::new("traversal");
        for path in [
            "/../secret",
            "/blog/../../secret",
            "/%2e%2e/secret",
            "/%2E%2E%2Fsecret",
            "/..%2fsecret",
            "/..%5csecret",
            "/..\\secret",
            "//../secret",
            "/./../secret",
        ] {
            let response = f.get(path).await;
            assert!(
                response.starts_with("HTTP/1.1 404"),
                "{} gave: {}",
                path,
                response
            );
            assert!(
                !response.contains("TOP SECRET"),
                "{} leaked the secret",
                path
            );
        }
        assert_eq!(resolve_path(&f.root, "/../secret"), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_out_of_folder_is_refused() {
        let f = Fixture::new("symlink");
        std::os::unix::fs::symlink(f.tmp.join("secret"), f.tmp.join("site/link")).unwrap();
        let response = f.get("/link").await;
        assert!(response.starts_with("HTTP/1.1 404"));
        assert!(!response.contains("TOP SECRET"));
    }

    #[test]
    fn forward_must_be_loopback() {
        assert!(Site::forward("127.0.0.1:8000".parse().unwrap()).is_ok());
        assert!(Site::forward("[::1]:8000".parse().unwrap()).is_ok());
        assert!(Site::forward("192.168.1.10:80".parse().unwrap()).is_err());
        assert!(Site::forward("0.0.0.0:80".parse().unwrap()).is_err());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("/a%20b").as_deref(), Some("/a b"));
        assert_eq!(percent_decode("/%2e%2E").as_deref(), Some("/.."));
        assert_eq!(percent_decode("/%zz"), None);
        assert_eq!(percent_decode("/%2"), None);
        assert_eq!(percent_decode("/%ff"), None);
    }
}
