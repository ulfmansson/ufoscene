//! Minimal static file server for the demo page. Std only, localhost only.
//!
//!     cargo run -p ufo-serve --release -- web
//!
//! With PORT set it uses exactly that port (run.sh sets 8100, the port the Spotify
//! redirect URI names); without it, the first free port from 8000 up.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8], head_only: bool) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if !head_only {
        s.write_all(body)?;
    }
    s.flush()
}

fn handle(mut s: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(s.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/");
    // `/callback` is where Spotify sends the browser back after login: it is the page itself
    let rel = if path == "/" || path == "/callback" { "index.html" } else { path.trim_start_matches('/') };

    if method != "GET" && method != "HEAD" {
        return respond(&mut s, "405 Method Not Allowed", "text/plain", b"method not allowed", false);
    }
    if rel.split('/').any(|c| c.is_empty() || c == ".." || c.starts_with('.')) {
        return respond(&mut s, "404 Not Found", "text/plain", b"not found", false);
    }
    match fs::read(root.join(rel)) {
        Ok(body) => respond(&mut s, "200 OK", content_type(rel), &body, method == "HEAD"),
        Err(_) => respond(&mut s, "404 Not Found", "text/plain", b"not found", false),
    }
}

fn main() {
    let root = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("web"));
    // An explicit PORT is used as-is; otherwise take the first free port from 8000 up.
    let fixed: Option<u16> = env::var("PORT").ok().and_then(|p| p.parse().ok());
    let candidates: Vec<u16> = match fixed {
        Some(p) => vec![p],
        None => (8000..8020).collect(),
    };
    let mut bound = None;
    for &p in &candidates {
        match bind_both(p) {
            Ok(listeners) => {
                bound = Some((listeners, p));
                break;
            }
            Err(e) if fixed.is_some() => {
                eprintln!("cannot listen on port {p}: {e}");
                eprintln!("another program uses it; run with PORT=<other port> (and register that port's redirect URI in Spotify)");
                std::process::exit(1);
            }
            Err(_) => continue,
        }
    }
    let Some((listeners, port)) = bound else {
        eprintln!("no free port in 8000-8019; set PORT=<n> to choose one");
        std::process::exit(1);
    };
    println!("UFO demo: open http://127.0.0.1:{port}/ in Chrome, then ⋮ → Cast… → Sources → Cast tab");
    println!("serving {} — Ctrl+C to stop", root.display());
    let mut threads = Vec::new();
    for listener in listeners {
        let root = root.clone();
        threads.push(thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let root = root.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &root);
                });
            }
        }));
    }
    for t in threads {
        let _ = t.join();
    }
}

/// Listen on both loopback addresses. Browsers try `localhost` as IPv6 (::1) first,
/// so a port that another program holds on ::1 only would send them there instead.
/// A port counts as free only if neither address is taken; IPv6 is skipped only
/// when the machine has no IPv6 loopback at all.
fn bind_both(port: u16) -> std::io::Result<Vec<TcpListener>> {
    let v4 = TcpListener::bind(("127.0.0.1", port))?;
    match TcpListener::bind(("::1", port)) {
        Ok(v6) => Ok(vec![v4, v6]),
        Err(e) if e.kind() == std::io::ErrorKind::AddrNotAvailable => Ok(vec![v4]),
        Err(e) => Err(e),
    }
}
