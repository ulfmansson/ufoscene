//! Internet radio without a browser: `mpv` plays the station into PipeWire (the TV, by the
//! Pi's WirePlumber settings) and the player listens to the output's monitor, so picture and
//! sound stay together. mpv prints the stations' song titles (ICY metadata) as they change.
//! The stations are the ones in web/radio.js that send the song now playing.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

pub const STATIONS: &[(&str, &str)] = &[
    ("Heart 80s", "https://media-ssl.musicradio.com/Heart80sMP3"),
    ("Heart 90s", "https://media-ssl.musicradio.com/Heart90sMP3"),
    ("Heart Dance", "https://media-ssl.musicradio.com/HeartDanceMP3"),
    ("Heart 70s", "https://media-ssl.musicradio.com/Heart70sMP3"),
    ("Heart 00s", "https://media-ssl.musicradio.com/Heart00sMP3"),
    ("Capital Dance", "https://media-ssl.musicradio.com/CapitalDanceMP3"),
    ("Smooth", "https://media-ssl.musicradio.com/SmoothUKMP3"),
    ("Radio X Classic Rock", "https://media-ssl.musicradio.com/RadioXClassicRockMP3"),
    ("Gold", "https://media-ssl.musicradio.com/GoldMP3"),
    ("Retro Rádió", "https://icast.connectmedia.hu/5001/live.mp3"),
];

pub enum RadioEvent {
    /// artist, title
    Song(String, String),
    /// mpv is missing or the station would not play
    Failed(String),
}

pub struct Radio {
    child: Option<Child>,
    pub station: usize,
    tx: Sender<RadioEvent>,
    pub events: Receiver<RadioEvent>,
}

/// "Artist - Title" as stations send it; a title with no dash is all title.
pub fn split_song(s: &str) -> (String, String) {
    let s = s.trim();
    match s.split_once(" - ") {
        Some((a, t)) => (a.trim().to_string(), t.trim().to_string()),
        None => (String::new(), s.to_string()),
    }
}

/// The song in one line of mpv's terminal output, if it is the icy-title tag.
pub fn icy_title(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let t = t.strip_prefix("icy-title:")?.trim();
    (!t.is_empty()).then_some(t)
}

impl Radio {
    pub fn new(station: usize) -> Radio {
        let (tx, events) = channel();
        Radio { child: None, station: station % STATIONS.len(), tx, events }
    }

    pub fn name(&self) -> &'static str {
        STATIONS[self.station].0
    }

    pub fn play(&mut self) {
        self.stop();
        let (name, url) = STATIONS[self.station];
        let child = Command::new("mpv")
            .args([
                "--no-video",
                "--no-config",
                "--msg-level=all=error,cplayer=info",
                "--term-osd=no",
                "--display-tags=icy-title",
                "--cache=yes",
                "--demuxer-max-bytes=1MiB",
                "--audio-client-name=ufoscene",
                url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match child {
            Ok(mut c) => {
                // mpv's messages land on stdout or stderr depending on version and terminal
                let out: Box<dyn std::io::Read + Send> = Box::new(c.stdout.take().unwrap());
                let err: Box<dyn std::io::Read + Send> = Box::new(c.stderr.take().unwrap());
                for pipe in [out, err] {
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(pipe).lines() {
                            let Ok(line) = line else { break };
                            if let Some(t) = icy_title(&line) {
                                let (a, t) = split_song(t);
                                if tx.send(RadioEvent::Song(a, t)).is_err() {
                                    return;
                                }
                            }
                        }
                    });
                }
                self.child = Some(c);
            }
            Err(e) => {
                let why = if e.kind() == std::io::ErrorKind::NotFound { "the radio needs mpv: sudo apt install mpv".to_string() } else { format!("{name}: {e}") };
                let _ = self.tx.send(RadioEvent::Failed(why));
            }
        }
    }

    pub fn step(&mut self, back: bool) {
        let n = STATIONS.len();
        self.station = if back { (self.station + n - 1) % n } else { (self.station + 1) % n };
        self.play();
    }

    /// mpv quit on its own (the stream broke): true once, then call `play` or `step`.
    pub fn ended(&mut self) -> bool {
        let gone = self.child.as_mut().is_some_and(|c| !matches!(c.try_wait(), Ok(None)));
        if gone {
            self.child = None;
        }
        gone
    }

    pub fn playing(&self) -> bool {
        self.child.is_some()
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Radio {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_lines() {
        assert_eq!(icy_title(" icy-title: Roxette - Joyride"), Some("Roxette - Joyride"));
        assert_eq!(icy_title("icy-title:"), None);
        assert_eq!(icy_title("File tags:"), None);
        assert_eq!(split_song("Roxette - Joyride"), ("Roxette".into(), "Joyride".into()));
        assert_eq!(split_song("A-ha - Take On Me - 2016 Remaster"), ("A-ha".into(), "Take On Me - 2016 Remaster".into()));
        assert_eq!(split_song("Heart 80s"), ("".into(), "Heart 80s".into()));
    }
}
