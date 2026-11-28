//! The phone remote, through the Pi's own server (server.py), exactly as the TV page talks to
//! it: commands arrive as server-sent events on /api/events, and the player reports what it
//! shows (fps, demo, show) to /api/state. Plain HTTP/1.0 to 127.0.0.1, std only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use crate::json::{self, Json};

pub struct Command {
    pub cmd: String,
    pub arg: Json,
}

/// One server-sent event's `data:` line, parsed.
pub fn event(line: &str) -> Option<Command> {
    let data = line.strip_prefix("data:")?.trim();
    let v = json::parse(data)?;
    let cmd = v.get("cmd").str()?.to_string();
    let arg = v.get("arg").clone();
    Some(Command { cmd, arg })
}

/// Listen for commands, reconnecting when the server restarts: a "connected" command arrives
/// each time the stream opens, and one with an empty name when it could not.
pub fn listen(server: String) -> Receiver<Command> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        loop {
            if stream(&server, &tx).is_err() && tx.send(Command { cmd: String::new(), arg: Json::Null }).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    rx
}

fn stream(server: &str, tx: &Sender<Command>) -> std::io::Result<()> {
    let mut s = TcpStream::connect(server)?;
    // the server pings every 15 s: no ping for a minute means the connection is gone
    s.set_read_timeout(Some(Duration::from_secs(60)))?;
    write!(s, "GET /api/events HTTP/1.0\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n\r\n")?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line)?;
    if !line.contains(" 200") {
        return Err(std::io::Error::other(format!("events: {}", line.trim())));
    }
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let _ = tx.send(Command { cmd: "connected".into(), arg: Json::Null });
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if let Some(c) = event(line.trim_end())
            && tx.send(c).is_err()
        {
            return Ok(());
        }
    }
}

fn request(server: &str, method: &str, path: &str, body: Option<&str>) -> std::io::Result<String> {
    let mut s = TcpStream::connect(server)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.set_write_timeout(Some(Duration::from_secs(5)))?;
    let body = body.unwrap_or("");
    write!(
        s,
        "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )?;
    let mut resp = String::new();
    s.read_to_string(&mut resp)?;
    let (head, body) = resp.split_once("\r\n\r\n").unwrap_or((&resp, ""));
    if !head.lines().next().unwrap_or("").contains(" 200") {
        return Err(std::io::Error::other(head.lines().next().unwrap_or("no answer").to_string()));
    }
    Ok(body.to_string())
}

/// GET `path` and parse the JSON answer.
pub fn get(server: &str, path: &str) -> Option<Json> {
    json::parse(&request(server, "GET", path, None).ok()?)
}

/// POST a JSON object in the background (the render loop does not wait for it).
pub fn post(server: &str, path: &'static str, body: String) {
    let server = server.to_string();
    std::thread::spawn(move || {
        let _ = request(&server, "POST", path, Some(&body));
    });
}

/// Ask the server in the background; the answer arrives on the returned channel.
pub fn fetch(server: &str, path: &'static str) -> Receiver<Option<Json>> {
    let (tx, rx) = channel();
    let server = server.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(get(&server, path));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn events_parse() {
        let c = event(r#"data: {"cmd": "next", "arg": null}"#).unwrap();
        assert_eq!(c.cmd, "next");
        assert_eq!(c.arg, Json::Null);
        assert!(event(": ping").is_none());
        assert!(event("data: not json").is_none());
    }

    #[test]
    fn talks_to_a_server() {
        let l = match TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            // a sandbox that forbids listening (macOS seatbelt): nothing to talk to
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return eprintln!("skipped: {e}"),
            Err(e) => panic!("{e}"),
        };
        let addr = l.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let mut s = s.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                let mut h = String::new();
                while r.read_line(&mut h).unwrap() > 2 {
                    h.clear();
                }
                if first.starts_with("GET /api/events") {
                    s.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: text/event-stream\r\n\r\n: hello\n\ndata: {\"cmd\": \"demo\", \"arg\": 7}\n\n").unwrap();
                } else {
                    s.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"urls\": {\"mdns\": \"http://ufoscene.local/\"}}").unwrap();
                }
            }
        });
        let st = get(&addr, "/api/state").unwrap();
        assert_eq!(st.get("urls").get("mdns").str(), Some("http://ufoscene.local/"));
        let rx = listen(addr);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().cmd, "connected");
        let c = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((c.cmd.as_str(), c.arg.num()), ("demo", Some(7.0)));
    }
}
