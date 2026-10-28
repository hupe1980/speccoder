//! Who may talk to the host. Loopback is not access control: any local
//! process and any web page can reach 127.0.0.1. So:
//!
//! - every `/api` request carries the per-start token as a bearer header —
//!   never in a URL, which ends up in logs and history;
//! - the `Host` header must name the loopback address and this port, so a
//!   page on another site whose name resolves to 127.0.0.1 (DNS rebinding)
//!   is refused;
//! - a request that changes anything must come from this origin;
//! - the interface is served with a policy that lets it load and send to
//!   this host only, and never be framed.

use axum::http::{HeaderMap, Method, header};

/// What the page may load and where it may send: this host and nothing else.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; \
     font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// A fresh token: 32 random bytes, hex.
#[must_use]
pub fn token() -> String {
    let mut bytes = [0u8; 32];
    // Without an OS random source there is no safe token: an empty one
    // matches nothing (see `authorised`).
    if getrandom::fill(&mut bytes).is_err() {
        return String::new();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares without leaking where the first difference is.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The bearer token is the host's.
#[must_use]
pub fn authorised(token: &str, headers: &HeaderMap) -> bool {
    !token.is_empty()
        && headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| constant_time_eq(t.as_bytes(), token.as_bytes()))
}

/// The names this host answers to.
#[must_use]
pub fn hosts(port: u16) -> [String; 3] {
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
}

/// The `Host` header names this host (DNS rebinding defence).
#[must_use]
pub fn host_ok(port: u16, headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| hosts(port).iter().any(|ok| ok == h))
}

/// A request that changes something comes from this origin: browsers send
/// `Origin` on every non-GET fetch, so a cross-site form or script is
/// refused even if it guessed the token.
#[must_use]
pub fn origin_ok(port: u16, method: &Method, headers: &HeaderMap) -> bool {
    if method == Method::GET || method == Method::HEAD {
        return true;
    }
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        // A non-browser client with the token (a script, the CLI) sends none.
        None => true,
        Some(o) => hosts(port).iter().any(|h| o == format!("http://{h}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            if let Ok(v) = v.parse() {
                h.insert(k.clone(), v);
            }
        }
        h
    }

    #[test]
    fn the_token_host_and_origin_are_checked() {
        let t = token();
        assert_eq!(t.len(), 64);
        assert!(authorised(
            &t,
            &headers(&[(header::AUTHORIZATION, &format!("Bearer {t}"))])
        ));
        assert!(!authorised(
            &t,
            &headers(&[(header::AUTHORIZATION, "Bearer nope")])
        ));
        assert!(!authorised(&t, &HeaderMap::new()));
        assert!(!authorised(
            "",
            &headers(&[(header::AUTHORIZATION, "Bearer ")])
        ));
        assert!(host_ok(4321, &headers(&[(header::HOST, "127.0.0.1:4321")])));
        assert!(host_ok(4321, &headers(&[(header::HOST, "localhost:4321")])));
        assert!(!host_ok(
            4321,
            &headers(&[(header::HOST, "evil.example:4321")])
        ));
        assert!(!host_ok(4321, &headers(&[(header::HOST, "127.0.0.1:9")])));
        assert!(origin_ok(
            4321,
            &Method::POST,
            &headers(&[(header::ORIGIN, "http://127.0.0.1:4321")])
        ));
        assert!(!origin_ok(
            4321,
            &Method::POST,
            &headers(&[(header::ORIGIN, "https://evil.example")])
        ));
        assert!(origin_ok(
            4321,
            &Method::GET,
            &headers(&[(header::ORIGIN, "https://evil.example")])
        ));
    }
}
