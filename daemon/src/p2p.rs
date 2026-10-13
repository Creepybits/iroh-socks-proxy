//! Everything that talks to Iroh lives here, so an Iroh upgrade only touches this file.
//!
//! In plain terms: each daemon has a secret key. Its public half is the daemon's
//! identity, and `<public key>.anon` is its address. Visiting that address opens an
//! encrypted Iroh connection to the peer, and the browser's bytes are copied through
//! one stream of that connection (a "tunnel"), much like a Tor onion service.

use std::fmt::Write as _;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};
use tokio::io::{AsyncRead, AsyncWrite, Join, ReadBuf};

/// A peer's identity (its public key).
pub use iroh::EndpointId as PeerId;

/// Protocol name sent in the Iroh handshake; both sides must agree on it.
pub const TUNNEL_ALPN: &[u8] = b"sovereign/tunnel/0";

/// How long to wait for a peer before telling the browser it is unreachable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

const KEY_FILE: &str = "secret.key";
const ANON_SUFFIX: &str = ".anon";
/// A 32-byte key in z-base-32 is always 52 characters.
const KEY_LABEL_LEN: usize = 52;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// The `.anon` address for a peer: its public key in z-base-32 (lowercase, no padding).
pub fn anon_address(id: &PeerId) -> String {
    format!("{}{}", id.to_z32(), ANON_SUFFIX)
}

/// The first 8 characters of a peer's address, enough to tell peers apart in logs.
pub fn short_id(id: &PeerId) -> String {
    id.to_z32()[..8].to_string()
}

/// Parses `<52-char key>.anon` (any case, optional trailing dot). Returns `None` for
/// anything else, including well-formed names that aren't valid keys.
pub fn parse_anon_address(host: &str) -> Option<PeerId> {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let label = host.strip_suffix(ANON_SUFFIX)?;
    if label.len() != KEY_LABEL_LEN {
        return None;
    }
    PeerId::from_z32(label).ok()
}

/// Loads the daemon's secret key from `data_dir`, creating and saving a new one on
/// first run, so the `.anon` address stays the same across restarts.
pub fn load_or_create_secret_key(data_dir: &Path) -> io::Result<SecretKey> {
    let path = data_dir.join(KEY_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => text.trim().parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a valid key: {}", path.display(), e),
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir_all(data_dir)?;
            let key = SecretKey::generate();
            write_private_file(&path, &to_hex(&key.to_bytes()))?;
            Ok(key)
        }
        Err(e) => Err(e),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{:02x}", b);
        s
    })
}

/// Creates a new file readable only by the current user (on Unix; on Windows the
/// user's app data folder is already private).
fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")
}

/// This daemon's Iroh endpoint.
pub struct P2p {
    endpoint: Endpoint,
    lookup: MemoryLookup,
}

impl P2p {
    /// Joins the public Iroh network: uses n0's relay servers and publishes this
    /// endpoint's address under its key so other peers can find it by key alone.
    /// With `accept_tunnels`, other peers may open tunnels to us.
    pub async fn bind_public(secret_key: SecretKey, accept_tunnels: bool) -> Result<Self, Error> {
        let lookup = MemoryLookup::new();
        let builder = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .address_lookup(lookup.clone())
            .alpns(alpns(accept_tunnels));
        let endpoint = builder.bind().await?;
        Ok(Self { endpoint, lookup })
    }

    /// An endpoint on 127.0.0.1 only, with no relays and no address lookup service.
    /// Peers must be added with [`P2p::add_peer_addr`]. Used by tests, so they never
    /// touch the internet.
    pub async fn bind_local(secret_key: SecretKey, accept_tunnels: bool) -> Result<Self, Error> {
        let lookup = MemoryLookup::new();
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(secret_key)
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")?
            .address_lookup(lookup.clone())
            .alpns(alpns(accept_tunnels))
            .bind()
            .await?;
        Ok(Self { endpoint, lookup })
    }

    pub fn id(&self) -> PeerId {
        self.endpoint.id()
    }

    /// The local UDP addresses this endpoint listens on.
    pub fn local_addrs(&self) -> Vec<SocketAddr> {
        self.endpoint.bound_sockets()
    }

    /// Tells this endpoint where to reach `peer` directly.
    pub fn add_peer_addr(&self, peer: PeerId, addr: SocketAddr) {
        self.lookup
            .add_endpoint_info(EndpointAddr::new(peer).with_ip_addr(addr));
    }

    /// Opens a tunnel to `peer`, giving up after [`CONNECT_TIMEOUT`].
    pub async fn open_tunnel(&self, peer: PeerId) -> Result<Tunnel, Error> {
        let open = async {
            let conn = self.endpoint.connect(peer, TUNNEL_ALPN).await?;
            let (send, recv) = conn.open_bi().await?;
            Ok::<_, Error>(Tunnel::new(conn, send, recv))
        };
        tokio::time::timeout(CONNECT_TIMEOUT, open)
            .await
            .map_err(|_| "timed out connecting to peer")?
    }

    /// Waits for the next peer to start connecting. Returns `None` once the endpoint
    /// is closed. Finish the handshake with [`IncomingTunnel::accept`], ideally in its
    /// own task so one slow peer doesn't hold up the others.
    pub async fn next_incoming(&self) -> Option<IncomingTunnel> {
        self.endpoint.accept().await.map(IncomingTunnel)
    }

    /// Closes all connections gracefully.
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// A peer that has started connecting to us.
pub struct IncomingTunnel(Incoming);

impl IncomingTunnel {
    /// Completes the handshake and waits for the peer's stream, giving up after
    /// [`CONNECT_TIMEOUT`].
    pub async fn accept(self) -> Result<Tunnel, Error> {
        let accept = async {
            let conn = self.0.await?;
            let (send, recv) = conn.accept_bi().await?;
            Ok::<_, Error>(Tunnel::new(conn, send, recv))
        };
        tokio::time::timeout(CONNECT_TIMEOUT, accept)
            .await
            .map_err(|_| "timed out accepting peer")?
    }
}

fn alpns(accept_tunnels: bool) -> Vec<Vec<u8>> {
    if accept_tunnels {
        vec![TUNNEL_ALPN.to_vec()]
    } else {
        Vec::new()
    }
}

/// One stream to a peer, usable like a TCP socket (read and write bytes).
/// Shutting down the write side tells the peer we're done sending.
pub struct Tunnel {
    conn: Connection,
    io: Join<RecvStream, SendStream>,
}

impl Tunnel {
    fn new(conn: Connection, send: SendStream, recv: RecvStream) -> Self {
        Self {
            conn,
            io: tokio::io::join(recv, send),
        }
    }

    pub fn peer(&self) -> PeerId {
        self.conn.remote_id()
    }

    /// Waits until the peer closes the connection. Call this after shutting down
    /// the write side, so the last bytes reach the peer before we drop (and close)
    /// the tunnel.
    pub async fn closed(&self) {
        self.conn.closed().await;
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.conn.close(0u32.into(), b"done");
    }
}

impl AsyncRead for Tunnel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Tunnel {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_round_trip() {
        let id = SecretKey::generate().public();
        let addr = anon_address(&id);
        assert_eq!(addr.len(), KEY_LABEL_LEN + ANON_SUFFIX.len());
        assert_eq!(addr, addr.to_ascii_lowercase());
        assert_eq!(parse_anon_address(&addr), Some(id));
        assert_eq!(parse_anon_address(&addr.to_ascii_uppercase()), Some(id));
        assert_eq!(parse_anon_address(&format!("{}.", addr)), Some(id));
    }

    #[test]
    fn rejects_non_key_addresses() {
        let id = SecretKey::generate().public();
        let label = id.to_z32();
        assert_eq!(parse_anon_address("testsite.anon"), None);
        assert_eq!(parse_anon_address(&label), None);
        assert_eq!(parse_anon_address(&format!("{}.com", label)), None);
        assert_eq!(parse_anon_address(&format!("{}a.anon", label)), None);
        assert_eq!(parse_anon_address(&format!("{}.anon", &label[1..])), None);
        // Right length, but '0' is not in the z-base-32 alphabet.
        assert_eq!(
            parse_anon_address(&format!("{}.anon", "0".repeat(52))),
            None
        );
        assert_eq!(parse_anon_address(&format!("www.{}.anon", label)), None);
    }

    #[test]
    fn secret_key_survives_restart() {
        let dir = std::env::temp_dir().join(format!("sovereign-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = load_or_create_secret_key(&dir).unwrap();
        let second = load_or_create_secret_key(&dir).unwrap();
        assert_eq!(first.public(), second.public());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_key_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("sovereign-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(KEY_FILE), "not a key").unwrap();
        assert!(load_or_create_secret_key(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
