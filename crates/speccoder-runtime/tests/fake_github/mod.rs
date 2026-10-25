//! A GitHub that answers from a table (F-089): a plain HTTP server on
//! 127.0.0.1 standing in for `api.github.com`, recording every request.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// One request as it arrived.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub authorization: String,
    pub body: String,
}

/// The server; its address goes in `SPECCODER_GITHUB_API`.
pub struct Fake {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
}

impl Fake {
    /// Every request so far, `METHOD path` each.
    pub fn calls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect()
    }
}

/// Serves `routes` — `("GET /user", 200, json)`, matched on method and the
/// path without its query — forever; anything else is a 404.
pub fn serve(routes: Vec<(&'static str, u16, String)>) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_owned();
            let path = parts.next().unwrap_or_default().to_owned();
            let (mut length, mut authorization) = (0usize, String::new());
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                let lower = h.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("authorization:") {
                    authorization = h["authorization:".len()..].trim().to_owned();
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let body = String::from_utf8_lossy(&body).into_owned();
            seen.lock().unwrap().push(Request {
                method: method.clone(),
                path: path.clone(),
                authorization,
                body,
            });
            let bare = path.split('?').next().unwrap_or_default();
            let key = format!("{method} {bare}");
            let (status, text) = routes
                .iter()
                .find(|(k, ..)| *k == key)
                .map(|(_, s, t)| (*s, t.clone()))
                .unwrap_or((404, r#"{"message":"Not Found"}"#.to_owned()));
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    Fake { url, requests }
}
