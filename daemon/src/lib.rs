//! Sovereign daemon: a local SOCKS5 proxy that answers `.anon` hosts and
//! refuses everything else (Air-Gap Mode).

pub mod router;
pub mod server;
pub mod socks;
