//! Sound in from PipeWire, the way ufoscene hears it in the browser: the default input (a
//! microphone, or the output's monitor when a phone plays over Bluetooth or AirPlay; the Pi's
//! server switches the default), or the monitor of the output for the radio this player
//! plays itself. A recorder runs as a child process (`parec` from pulseaudio-utils, or
//! `pw-record`) and writes mono 32-bit float samples at 48 kHz to a pipe.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::analyser::FFT;

pub const RATE: u32 = 48_000;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Input {
    /// PipeWire's default source
    Default,
    /// the monitor of the default sink: what the Pi itself plays
    Monitor,
}

struct Shared {
    ring: Vec<f32>,
    at: usize,
    last: Option<Instant>,
}

pub struct Capture {
    child: Option<Child>,
    shared: Arc<Mutex<Shared>>,
    pub tool: &'static str,
}

fn recorder(input: Input) -> Vec<(&'static str, Vec<String>)> {
    let rate = RATE.to_string();
    let parec_dev = match input {
        Input::Default => "@DEFAULT_SOURCE@",
        Input::Monitor => "@DEFAULT_MONITOR@",
    };
    let parec = vec![
        "--raw".into(),
        "--format=float32le".into(),
        format!("--rate={rate}"),
        "--channels=1".into(),
        "--latency-msec=10".into(),
        format!("--device={parec_dev}"),
        "--client-name=ufoscene".into(),
    ];
    let mut pw = vec!["--format".into(), "f32".into(), "--rate".into(), rate, "--channels".into(), "1".into(), "--latency".into(), "10ms".into()];
    if input == Input::Monitor {
        pw.extend(["-P".into(), "{ stream.capture.sink = true }".into()]);
    }
    pw.push("-".into());
    vec![("parec", parec), ("pw-record", pw)]
}

impl Capture {
    pub fn start(input: Input) -> Capture {
        let shared = Arc::new(Mutex::new(Shared { ring: vec![0.0; FFT], at: 0, last: None }));
        for (tool, args) in recorder(input) {
            let child = Command::new(tool).args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
            let Ok(mut child) = child else { continue };
            let mut out = child.stdout.take().unwrap();
            let sh = shared.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let mut carry = Vec::with_capacity(4);
                loop {
                    let n = match out.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    carry.extend_from_slice(&buf[..n]);
                    let whole = carry.len() / 4 * 4;
                    let mut s = sh.lock().unwrap();
                    for c in carry[..whole].chunks_exact(4) {
                        let at = s.at;
                        s.ring[at] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                        s.at = (at + 1) % FFT;
                    }
                    s.last = Some(Instant::now());
                    drop(s);
                    carry.drain(..whole);
                }
            });
            return Capture { child: Some(child), shared, tool };
        }
        eprintln!("ufo-native: neither parec nor pw-record could be started: running on autopilot");
        Capture { child: None, shared, tool: "none" }
    }

    /// The latest `FFT` samples, oldest first; false when no sound has come for a while
    /// (the recorder stopped, or PipeWire is not up yet).
    pub fn latest(&self, out: &mut [f32; FFT]) -> bool {
        let s = self.shared.lock().unwrap();
        let (a, b) = s.ring.split_at(s.at);
        out[..b.len()].copy_from_slice(b);
        out[b.len()..].copy_from_slice(a);
        s.last.is_some_and(|t| t.elapsed() < Duration::from_millis(500))
    }

    /// True when the recorder has exited (to be started again).
    pub fn ended(&mut self) -> bool {
        match &mut self.child {
            Some(c) => !matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
