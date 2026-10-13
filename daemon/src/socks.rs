//! SOCKS5 protocol parsing (RFC 1928), limited to what the daemon needs:
//! no authentication and the CONNECT command only.
//!
//! The functions are generic over any async stream, so the tests can feed them
//! in-memory bytes instead of a real socket.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const SOCKS_VERSION: u8 = 0x05;
pub const METHOD_NO_AUTH: u8 = 0x00;
pub const METHOD_NO_ACCEPTABLE: u8 = 0xFF;
pub const CMD_CONNECT: u8 = 0x01;

pub const ADDR_TYPE_IPV4: u8 = 0x01;
pub const ADDR_TYPE_DOMAIN: u8 = 0x03;
pub const ADDR_TYPE_IPV6: u8 = 0x04;

pub const REP_SUCCESS: u8 = 0x00;
pub const REP_HOST_UNREACHABLE: u8 = 0x04;
pub const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const REP_ADDR_TYPE_NOT_SUPPORTED: u8 = 0x08;

/// The host part of the address the browser wants to reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetHost {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Domain(String),
}

/// Where the browser wants to connect: host plus port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: TargetHost,
    pub port: u16,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            TargetHost::Ipv4(ip) => write!(f, "{}:{}", ip, self.port),
            TargetHost::Ipv6(ip) => write!(f, "[{}]:{}", ip, self.port),
            TargetHost::Domain(name) => write!(f, "{}:{}", name, self.port),
        }
    }
}

#[derive(Debug)]
pub enum SocksError {
    Io(io::Error),
    BadVersion(u8),
    NoAcceptableAuth,
    UnsupportedCommand(u8),
    UnsupportedAddressType(u8),
    /// Domain name that is empty or not valid UTF-8.
    InvalidDomain,
}

impl fmt::Display for SocksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocksError::Io(e) => write!(f, "I/O error: {}", e),
            SocksError::BadVersion(v) => write!(f, "unsupported SOCKS version {}", v),
            SocksError::NoAcceptableAuth => write!(f, "no acceptable authentication method"),
            SocksError::UnsupportedCommand(c) => write!(f, "unsupported command {}", c),
            SocksError::UnsupportedAddressType(t) => write!(f, "unsupported address type {}", t),
            SocksError::InvalidDomain => write!(f, "invalid domain name"),
        }
    }
}

impl std::error::Error for SocksError {}

impl From<io::Error> for SocksError {
    fn from(e: io::Error) -> Self {
        SocksError::Io(e)
    }
}

/// Runs the SOCKS5 greeting and reads the CONNECT request.
///
/// On success the caller must still answer with [`send_reply`]. On a protocol
/// error, the matching error reply has already been sent where SOCKS5 has one.
pub async fn handshake<S>(stream: &mut S) -> Result<Target, SocksError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    negotiate_method(stream).await?;
    read_request(stream).await
}

/// Step 1: the client lists the auth methods it supports; we only accept "no auth".
async fn negotiate_method<S>(stream: &mut S) -> Result<(), SocksError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;
    let [version, nmethods] = header;
    if version != SOCKS_VERSION {
        return Err(SocksError::BadVersion(version));
    }

    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;

    if !methods.contains(&METHOD_NO_AUTH) {
        stream
            .write_all(&[SOCKS_VERSION, METHOD_NO_ACCEPTABLE])
            .await?;
        return Err(SocksError::NoAcceptableAuth);
    }

    stream.write_all(&[SOCKS_VERSION, METHOD_NO_AUTH]).await?;
    Ok(())
}

/// Step 2: the client says which command it wants and where to connect.
async fn read_request<S>(stream: &mut S) -> Result<Target, SocksError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let [version, cmd, _reserved, atyp] = header;

    if version != SOCKS_VERSION {
        return Err(SocksError::BadVersion(version));
    }
    if cmd != CMD_CONNECT {
        send_reply(stream, REP_COMMAND_NOT_SUPPORTED).await?;
        return Err(SocksError::UnsupportedCommand(cmd));
    }

    let host = match atyp {
        ADDR_TYPE_IPV4 => {
            let mut bytes = [0u8; 4];
            stream.read_exact(&mut bytes).await?;
            TargetHost::Ipv4(Ipv4Addr::from(bytes))
        }
        ADDR_TYPE_IPV6 => {
            let mut bytes = [0u8; 16];
            stream.read_exact(&mut bytes).await?;
            TargetHost::Ipv6(Ipv6Addr::from(bytes))
        }
        ADDR_TYPE_DOMAIN => {
            let len = stream.read_u8().await? as usize;
            let mut bytes = vec![0u8; len];
            stream.read_exact(&mut bytes).await?;
            match String::from_utf8(bytes) {
                Ok(name) if !name.is_empty() => TargetHost::Domain(name),
                _ => {
                    send_reply(stream, REP_HOST_UNREACHABLE).await?;
                    return Err(SocksError::InvalidDomain);
                }
            }
        }
        other => {
            send_reply(stream, REP_ADDR_TYPE_NOT_SUPPORTED).await?;
            return Err(SocksError::UnsupportedAddressType(other));
        }
    };

    let port = stream.read_u16().await?;
    Ok(Target { host, port })
}

/// Sends a SOCKS5 reply. The bound address is always reported as 0.0.0.0:0,
/// which browsers ignore.
pub async fn send_reply<S>(stream: &mut S, rep: u8) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let reply = [SOCKS_VERSION, rep, 0x00, ADDR_TYPE_IPV4, 0, 0, 0, 0, 0, 0];
    stream.write_all(&reply).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// Feeds `client_bytes` to `handshake` and returns its result plus every
    /// byte the daemon wrote back.
    async fn run_handshake(client_bytes: &[u8]) -> (Result<Target, SocksError>, Vec<u8>) {
        let (mut client, mut server) = duplex(1024);
        client.write_all(client_bytes).await.unwrap();
        let result = handshake(&mut server).await;
        drop(server);
        let mut written = Vec::new();
        client.read_to_end(&mut written).await.unwrap();
        (result, written)
    }

    const GREETING: [u8; 3] = [SOCKS_VERSION, 1, METHOD_NO_AUTH];
    const METHOD_OK: [u8; 2] = [SOCKS_VERSION, METHOD_NO_AUTH];

    fn connect_request(atyp: u8, addr: &[u8], port: u16) -> Vec<u8> {
        let mut bytes = GREETING.to_vec();
        bytes.extend_from_slice(&[SOCKS_VERSION, CMD_CONNECT, 0, atyp]);
        bytes.extend_from_slice(addr);
        bytes.extend_from_slice(&port.to_be_bytes());
        bytes
    }

    fn reply(rep: u8) -> Vec<u8> {
        let mut bytes = METHOD_OK.to_vec();
        bytes.extend_from_slice(&[SOCKS_VERSION, rep, 0, ADDR_TYPE_IPV4, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    #[tokio::test]
    async fn parses_ipv4() {
        let (result, written) =
            run_handshake(&connect_request(ADDR_TYPE_IPV4, &[192, 168, 1, 2], 8080)).await;
        let target = result.unwrap();
        assert_eq!(target.host, TargetHost::Ipv4(Ipv4Addr::new(192, 168, 1, 2)));
        assert_eq!(target.port, 8080);
        assert_eq!(target.to_string(), "192.168.1.2:8080");
        assert_eq!(written, METHOD_OK);
    }

    #[tokio::test]
    async fn parses_ipv6() {
        let ip: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let (result, _) = run_handshake(&connect_request(ADDR_TYPE_IPV6, &ip.octets(), 443)).await;
        let target = result.unwrap();
        assert_eq!(target.host, TargetHost::Ipv6(ip));
        assert_eq!(target.to_string(), "[2001:db8::1]:443");
    }

    #[tokio::test]
    async fn parses_domain() {
        let name = b"testsite.anon";
        let mut addr = vec![name.len() as u8];
        addr.extend_from_slice(name);
        let (result, written) = run_handshake(&connect_request(ADDR_TYPE_DOMAIN, &addr, 80)).await;
        let target = result.unwrap();
        assert_eq!(target.host, TargetHost::Domain("testsite.anon".into()));
        assert_eq!(target.to_string(), "testsite.anon:80");
        assert_eq!(written, METHOD_OK);
    }

    #[tokio::test]
    async fn rejects_bad_version_in_greeting() {
        let (result, written) = run_handshake(&[0x04, 1, METHOD_NO_AUTH]).await;
        assert!(matches!(result, Err(SocksError::BadVersion(0x04))));
        assert!(written.is_empty());
    }

    #[tokio::test]
    async fn rejects_bad_version_in_request() {
        let mut bytes = GREETING.to_vec();
        bytes.extend_from_slice(&[0x04, CMD_CONNECT, 0, ADDR_TYPE_IPV4]);
        let (result, _) = run_handshake(&bytes).await;
        assert!(matches!(result, Err(SocksError::BadVersion(0x04))));
    }

    #[tokio::test]
    async fn rejects_unsupported_command() {
        let mut bytes = GREETING.to_vec();
        bytes.extend_from_slice(&[SOCKS_VERSION, 0x02, 0, ADDR_TYPE_IPV4]); // BIND
        let (result, written) = run_handshake(&bytes).await;
        assert!(matches!(result, Err(SocksError::UnsupportedCommand(0x02))));
        assert_eq!(written, reply(REP_COMMAND_NOT_SUPPORTED));
    }

    #[tokio::test]
    async fn rejects_unsupported_address_type() {
        let mut bytes = GREETING.to_vec();
        bytes.extend_from_slice(&[SOCKS_VERSION, CMD_CONNECT, 0, 0x09]);
        let (result, written) = run_handshake(&bytes).await;
        assert!(matches!(
            result,
            Err(SocksError::UnsupportedAddressType(0x09))
        ));
        assert_eq!(written, reply(REP_ADDR_TYPE_NOT_SUPPORTED));
    }

    #[tokio::test]
    async fn rejects_when_no_acceptable_auth_method() {
        let (result, written) = run_handshake(&[SOCKS_VERSION, 1, 0x02]).await; // username/password only
        assert!(matches!(result, Err(SocksError::NoAcceptableAuth)));
        assert_eq!(written, [SOCKS_VERSION, METHOD_NO_ACCEPTABLE]);
    }

    #[tokio::test]
    async fn replies_when_domain_is_not_utf8() {
        let (result, written) =
            run_handshake(&connect_request(ADDR_TYPE_DOMAIN, &[2, 0xFF, 0xFE], 80)).await;
        assert!(matches!(result, Err(SocksError::InvalidDomain)));
        assert_eq!(written, reply(REP_HOST_UNREACHABLE));
    }

    #[tokio::test]
    async fn replies_when_domain_is_empty() {
        let (result, written) = run_handshake(&connect_request(ADDR_TYPE_DOMAIN, &[0], 80)).await;
        assert!(matches!(result, Err(SocksError::InvalidDomain)));
        assert_eq!(written, reply(REP_HOST_UNREACHABLE));
    }
}
