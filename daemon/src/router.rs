//! Decides what happens to a connection based on the host the browser asked for.

use crate::p2p::{self, PeerId};
use crate::socks::TargetHost;

/// The built-in demo site, served without any network.
pub const LANDING_HOST: &str = "testsite.anon";

/// What the daemon does with a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `testsite.anon`: serve the built-in landing page.
    Landing,
    /// `<peer key>.anon`: tunnel to that peer over Iroh.
    Tunnel(PeerId),
    /// Anything else (Air-Gap Mode), including `.anon` names that aren't valid
    /// keys: refuse with "host unreachable".
    Blocked,
}

pub fn route(host: &TargetHost) -> Route {
    let TargetHost::Domain(name) = host else {
        return Route::Blocked;
    };
    let normalized = name.strip_suffix('.').unwrap_or(name);
    if normalized.eq_ignore_ascii_case(LANDING_HOST) {
        return Route::Landing;
    }
    match p2p::parse_anon_address(name) {
        Some(peer) => Route::Tunnel(peer),
        None => Route::Blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::SecretKey;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn domain(name: &str) -> TargetHost {
        TargetHost::Domain(name.into())
    }

    #[test]
    fn landing_page_host() {
        assert_eq!(route(&domain("testsite.anon")), Route::Landing);
        assert_eq!(route(&domain("TESTSITE.ANON")), Route::Landing);
        assert_eq!(route(&domain("testsite.anon.")), Route::Landing);
    }

    #[test]
    fn key_addresses_tunnel() {
        let id = SecretKey::generate().public();
        let addr = p2p::anon_address(&id);
        assert_eq!(route(&domain(&addr)), Route::Tunnel(id));
        assert_eq!(route(&domain(&addr.to_uppercase())), Route::Tunnel(id));
    }

    #[test]
    fn everything_else_is_blocked() {
        assert_eq!(route(&domain("other.anon")), Route::Blocked);
        assert_eq!(route(&domain("anon")), Route::Blocked);
        assert_eq!(route(&domain("example.com")), Route::Blocked);
        assert_eq!(route(&domain("testsite.anon.example.com")), Route::Blocked);
        assert_eq!(
            route(&TargetHost::Ipv4(Ipv4Addr::LOCALHOST)),
            Route::Blocked
        );
        assert_eq!(
            route(&TargetHost::Ipv6(Ipv6Addr::LOCALHOST)),
            Route::Blocked
        );
    }
}
