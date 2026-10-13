//! Decides what happens to a connection based on the host the browser asked for.

use crate::socks::TargetHost;

/// What the daemon does with a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A `.anon` host: serve the built-in landing page.
    Landing,
    /// Anything else (Air-Gap Mode): refuse with "host unreachable".
    Blocked,
}

pub fn route(host: &TargetHost) -> Route {
    match host {
        TargetHost::Domain(name) if is_anon_domain(name) => Route::Landing,
        _ => Route::Blocked,
    }
}

/// True for names ending in `.anon`, ignoring case and one trailing dot
/// (`TEST.ANON` and `test.anon.` both count). A bare `anon` does not.
pub fn is_anon_domain(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name).to_ascii_lowercase();
    name.len() > ".anon".len() && name.ends_with(".anon")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn anon_check() {
        assert!(is_anon_domain("testsite.anon"));
        assert!(is_anon_domain("TEST.ANON"));
        assert!(is_anon_domain("Mixed.Anon"));
        assert!(is_anon_domain("testsite.anon."));
        assert!(!is_anon_domain("anon"));
        assert!(!is_anon_domain(".anon"));
        assert!(!is_anon_domain("example.com"));
        assert!(!is_anon_domain("anon.example.com"));
        assert!(!is_anon_domain("notanon"));
    }

    #[test]
    fn routes() {
        assert_eq!(route(&TargetHost::Domain("x.anon".into())), Route::Landing);
        assert_eq!(
            route(&TargetHost::Domain("example.com".into())),
            Route::Blocked
        );
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
