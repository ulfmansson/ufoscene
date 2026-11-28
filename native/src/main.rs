//! ufo-native: the 100 UFO demos on a Raspberry Pi with no browser in between. The same Rust
//! demo code as ufo.wasm, compiled for the Pi's CPU, drawn straight to the TV with DRM/KMS,
//! listening to PipeWire, and driven by the phone remote through the Pi's server.
//!
//!     ufo-native                        on the TV (run on tty1 by ufoscene-native.service)
//!     ufo-native --bench 20             time the demos with no screen: is this Pi fast enough?
//!     ufo-native --bench 5 --png f.png  ... and write the last frame as the TV would show it
//!
//! See native/README.md for the options.

// Most of the player only runs with the TV output, which is Linux only; the rest still builds
// and tests on a Mac.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

mod analyser;
mod capture;
#[cfg(target_os = "linux")]
mod drm;
mod json;
mod overlay;
mod radio;
mod remote;
mod scale;

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use analyser::{Analyser, FFT};
use capture::{Capture, Input};
use json::{Json, quote};
use overlay::{Card, Overlay};
use radio::{Radio, RadioEvent};
use scale::Crt;
use ufo_demo::Demo;
use ufo_demo::gfx::{Fb, H, W};

const MIN_FPS: f32 = 25.0;
const USAGE: &str = "usage: ufo-native [--card /dev/dri/cardN] [--mode WxH[@Hz]] [--server HOST:PORT | --no-remote]
                  [--audio radio|phone|mic] [--demo N] [--tv] [--no-scanlines]
                  [--bench SECONDS [--png FILE] [--size WxH]]";

struct Args {
    card: Option<String>,
    mode: Option<String>,
    server: Option<String>,
    audio: Option<String>,
    demo: Option<usize>,
    tv: Option<bool>,
    scanlines: bool,
    bench: Option<f32>,
    png: Option<String>,
    size: (usize, usize),
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        card: None,
        mode: None,
        server: Some("127.0.0.1:80".into()),
        audio: None,
        demo: None,
        tv: None,
        scanlines: true,
        bench: None,
        png: None,
        size: (1920, 1080),
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = || it.next().ok_or(format!("{k} needs a value"));
        match k.as_str() {
            "--card" => a.card = Some(val()?),
            "--mode" => a.mode = Some(val()?),
            "--server" => a.server = Some(val()?),
            "--no-remote" => a.server = None,
            "--audio" => {
                let v = val()?;
                if !["radio", "phone", "mic", "none"].contains(&v.as_str()) {
                    return Err(format!("--audio: radio, phone, mic or none, not {v}"));
                }
                a.audio = Some(v);
            }
            "--demo" => a.demo = Some(val()?.parse::<usize>().map_err(|e| format!("--demo: {e}"))?.max(1) - 1),
            "--tv" => a.tv = Some(true),
            "--no-scanlines" => a.scanlines = false,
            "--bench" => a.bench = Some(val()?.parse().map_err(|e| format!("--bench: {e}"))?),
            "--png" => a.png = Some(val()?),
            "--size" => {
                let v = val()?;
                a.size = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).ok_or(format!("--size: WxH, not {v}"))?;
            }
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {k}")),
        }
    }
    Ok(a)
}

// ------------------------------------------------------------------ settings

/// The few choices that survive a restart, as the TV page keeps them in localStorage.
struct Settings {
    path: PathBuf,
    audio: String,
    station: usize,
    tv: bool,
}

impl Settings {
    fn load() -> Settings {
        let dir = std::env::var_os("STATE_DIRECTORY")
            .or_else(|| std::env::var_os("XDG_STATE_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ufoscene-native");
        let path = dir.join("settings");
        let mut s = Settings { path, audio: "radio".into(), station: 0, tv: false };
        if let Ok(text) = std::fs::read_to_string(&s.path) {
            for line in text.lines() {
                match line.split_once('=') {
                    Some(("audio", v)) if ["radio", "phone", "mic"].contains(&v) => s.audio = v.into(),
                    Some(("station", v)) => s.station = v.parse().unwrap_or(0),
                    Some(("tv", v)) => s.tv = v == "1",
                    _ => {}
                }
            }
        }
        s
    }

    fn save(&self) {
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let text = format!("audio={}\nstation={}\ntv={}\n", self.audio, self.station, self.tv as u8);
        let _ = std::fs::write(&self.path, text);
    }
}

// ------------------------------------------------------------------ the player

#[derive(Clone, Copy, PartialEq)]
enum Show {
    Rock,
    Ukraine,
}

impl Show {
    fn cmd(self) -> &'static str {
        match self {
            Show::Rock => "concert",
            Show::Ukraine => "ukraine",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Show::Rock => "Concert",
            Show::Ukraine => "Ukraine at Eurovision",
        }
    }
}

enum Pending {
    Connect(Receiver<Option<Json>>, Option<f32>),
    WifiCode(Receiver<Option<Json>>),
    WifiSetup(Receiver<Option<Json>>),
    WifiPoll(Receiver<Option<Json>>),
}

struct Player {
    demo: Box<Demo>,
    shown: Fb,
    overlay: Overlay,
    analyser: Analyser,
    samples: [f32; FFT],
    capture: Option<Capture>,
    capture_tried: Instant,
    radio: Radio,
    first_song: bool,
    settings: Settings,
    audio_off: bool,
    server: Option<String>,
    show: Option<Show>,
    paused_at: Option<f64>,
    paused_total: f64,
    tv: bool,
    last_song: String,
    last_device: Option<String>,
    last_wifi: Option<String>,
    pending: Vec<Pending>,
    quit: bool,
}

fn set_track(demo: &mut Demo, artist: &str, title: &str) {
    let s = format!("{artist}\n{title}");
    let mut n = s.len().min(demo.text_buf.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    demo.text_buf[..n].copy_from_slice(&s.as_bytes()[..n]);
    demo.set_track(n);
}

/// From 23 to 28 December the first start opens with the Christmas scene, as on the page.
fn christmas_week(unix: u64) -> bool {
    // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm), UTC
    let z = (unix / 86_400) as i64 + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    month == 12 && (23..=28).contains(&day)
}

impl Player {
    fn new(args: &Args, settings: Settings) -> Player {
        let mut demo = Box::new(Demo::new());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        demo.seed(now.subsec_nanos() ^ std::process::id().rotate_left(16) ^ now.as_secs() as u32);
        // shuffle, as on the page unless the visitor chose otherwise
        demo.jukebox = 2;
        match args.demo {
            Some(d) => demo.set_demo(d),
            None => demo.random_demo(),
        }
        if christmas_week(now.as_secs()) {
            demo.christmas_first();
        }
        let tv = args.tv.unwrap_or(settings.tv);
        Player {
            demo,
            shown: Fb::new(),
            overlay: Overlay::default(),
            analyser: Analyser::new(),
            samples: [0.0; FFT],
            capture: None,
            capture_tried: Instant::now(),
            radio: Radio::new(settings.station),
            first_song: true,
            audio_off: args.audio.as_deref() == Some("none"),
            settings,
            server: args.server.clone(),
            show: None,
            paused_at: None,
            paused_total: 0.0,
            tv,
            last_song: String::new(),
            last_device: None,
            last_wifi: None,
            pending: Vec::new(),
            quit: false,
        }
    }

    fn say(&mut self, s: &str, secs: f32) {
        self.overlay.say(s, secs);
    }

    fn report(&self, state: String) {
        if let Some(server) = &self.server {
            remote::post(server, "/api/state", state);
        }
    }

    // -------------------------------------------------------------- sound

    fn input(&self) -> Input {
        if self.settings.audio == "radio" { Input::Monitor } else { Input::Default }
    }

    fn start_audio(&mut self) {
        if self.audio_off {
            return;
        }
        if self.settings.audio == "radio" {
            self.radio.play();
            self.station_changed();
        } else {
            self.radio.stop();
        }
        self.capture = Some(Capture::start(self.input()));
        self.capture_tried = Instant::now();
    }

    fn set_audio(&mut self, source: &str) {
        let changed = self.settings.audio != source;
        self.settings.audio = source.into();
        self.settings.save();
        self.say(
            match source {
                "radio" => "Internet radio",
                "phone" => "Music from your phone",
                _ => "Microphone / line in",
            },
            3.0,
        );
        if changed || self.capture.is_none() || (source == "radio" && !self.radio.playing()) {
            self.capture = None;
            self.start_audio();
        }
        self.report(format!("{{\"audio\": {}}}", quote(source)));
    }

    fn station_changed(&mut self) {
        self.settings.station = self.radio.station;
        self.settings.save();
        let name = self.radio.name();
        set_track(&mut self.demo, name, "RADIO - SPACE FOR NEXT STATION");
        if self.demo.concert_left() <= 0.0 {
            self.demo.random_demo();
        }
        self.first_song = true;
        self.say(&format!("Radio: {name}. Skip on the remote = next station."), 6.0);
    }

    fn pump_audio(&mut self) -> bool {
        while let Ok(ev) = self.radio.events.try_recv() {
            match ev {
                RadioEvent::Song(artist, title) => {
                    set_track(&mut self.demo, &artist, &title);
                    if !self.first_song && self.demo.concert_left() <= 0.0 {
                        self.demo.random_demo();
                    }
                    self.first_song = false;
                }
                RadioEvent::Failed(why) => self.say(&why, 10.0),
            }
        }
        if self.settings.audio == "radio" && self.radio.ended() && !self.audio_off {
            // the stream broke: the next station, so a dead one does not stop the music
            self.say(&format!("{} stopped playing", self.radio.name()), 4.0);
            self.radio.step(false);
            self.station_changed();
        }
        let restart = match &mut self.capture {
            Some(c) => c.ended() || (c.tool == "none" && self.capture_tried.elapsed() > Duration::from_secs(10)),
            None => false,
        };
        if restart && self.capture_tried.elapsed() > Duration::from_secs(2) {
            self.capture = Some(Capture::start(self.input()));
            self.capture_tried = Instant::now();
        }
        let live = self.capture.as_ref().is_some_and(|c| c.latest(&mut self.samples));
        let bins = &mut self.demo.audio.bins;
        if live {
            self.analyser.bytes(&self.samples, bins);
        } else {
            self.analyser.decay(bins);
        }
        live
    }

    // -------------------------------------------------------------- the remote

    fn connect_card(&mut self, secs: Option<f32>) {
        if let Some(server) = &self.server {
            self.pending.push(Pending::Connect(remote::fetch(server, "/api/state"), secs));
        }
    }

    fn concert(&mut self, show: Show) {
        if self.demo.concert_left() > 0.0 && self.show == Some(show) {
            let cur = self.demo.current();
            self.demo.set_demo(cur);
            self.show = None;
            self.say(&format!("{} over: back to the demos", show.name()), 3.0);
        } else {
            self.demo.concert(show as usize);
            self.show = Some(show);
            self.say(&format!("{} on! Put the music on.", show.name()), 6.0);
        }
        let s = self.current_show().map_or("null".to_string(), |s| quote(s.cmd()));
        self.report(format!("{{\"show\": {s}}}"));
    }

    fn current_show(&self) -> Option<Show> {
        self.show.filter(|_| self.demo.concert_left() > 0.0)
    }

    fn set_paused(&mut self, on: bool, now: f64) {
        match (on, self.paused_at) {
            (true, None) => {
                self.paused_at = Some(now);
                self.say("Paused", 2.0);
            }
            (false, Some(at)) => {
                self.paused_total += now - at;
                self.paused_at = None;
            }
            _ => {}
        }
    }

    fn handle(&mut self, c: remote::Command, now: f64) {
        let arg = &c.arg;
        match c.cmd.as_str() {
            "next" => self.demo.next_demo(),
            "prev" => self.demo.prev_demo(),
            "random" => self.demo.random_demo(),
            "scene" => self.demo.next_part(),
            "hud" => self.demo.toggle_hud(),
            "tv" => {
                self.tv = !self.tv;
                self.settings.tv = self.tv;
                self.settings.save();
                self.say(if self.tv { "TV mode on: steady 30 fps, no scanlines" } else { "TV mode off: 60 fps with scanlines" }, 2.5);
            }
            "panel" => self.set_paused(self.paused_at.is_none(), now),
            "start" => self.set_paused(false, now),
            "concert" => self.concert(Show::Rock),
            "ukraine" => self.concert(Show::Ukraine),
            "song-next" | "song-prev" => {
                self.set_paused(false, now);
                if self.settings.audio == "radio" && !self.audio_off {
                    self.radio.step(c.cmd == "song-prev");
                    self.station_changed();
                } else {
                    self.say("Skip songs on the phone that plays the music", 3.0);
                }
            }
            "demo" => {
                if let Some(n) = arg.num().filter(|n| *n >= 1.0) {
                    self.demo.set_demo(n as usize - 1);
                    self.set_paused(false, now);
                }
            }
            "audio" => {
                if let Some(s) = arg.str().filter(|s| ["radio", "phone", "mic"].contains(s)) {
                    let s = s.to_string();
                    self.set_audio(&s);
                }
            }
            "overlay" => {
                if self.overlay.connect.is_some() {
                    self.overlay.connect = None;
                } else {
                    self.connect_card(None);
                }
            }
            "selftest" => {
                if self.overlay.fps.is_some() {
                    self.overlay.fps = None;
                    self.say("Self-test off", 3.0);
                } else {
                    self.overlay.fps = Some(0.0);
                    self.say(&format!("Self-test: needs {MIN_FPS:.0} fps or more"), 3.0);
                }
            }
            "nowplaying" => {
                if arg.get("playing").truthy() && self.settings.audio != "phone" {
                    self.set_audio("phone");
                }
                let artist = arg.get("artist").str().unwrap_or("").to_string();
                let title = arg.get("title").str().unwrap_or("").to_string();
                let song = format!("{artist}\0{title}");
                if !(artist.is_empty() && title.is_empty()) && song != self.last_song {
                    self.last_song = song;
                    set_track(&mut self.demo, &artist, &title);
                    if self.demo.concert_left() <= 0.0 {
                        self.demo.random_demo();
                    }
                }
            }
            "pair" => {
                self.overlay.pairing = Some(
                    Card::new("Bluetooth")
                        .line("On your phone, open Bluetooth and connect to UFOSCENE.")
                        .line("Then play music: the demos dance to it.")
                        .for_secs(120.0),
                );
            }
            "bluetooth" => {
                if !arg.get("pairing").truthy() {
                    self.overlay.pairing = None;
                }
                let device = arg.get("device").str().map(str::to_string);
                if let Some(d) = &device
                    && device != self.last_device
                {
                    self.say(&format!("Connected: {d}"), 4.0);
                }
                self.last_device = device;
            }
            "lastfm" => self.say("Song names from Last.fm need the browser kiosk: here the radio and the phone send them", 6.0),
            "reload" => self.quit = true,
            "wifi-code" => {
                if let Some(server) = &self.server {
                    self.pending.push(Pending::WifiCode(remote::fetch(server, "/api/wifi/code")));
                }
            }
            "wifi" => self.wifi_state(arg.clone()),
            "connected" | "" => {}
            other => eprintln!("ufo-native: command {other} is not for this player"),
        }
    }

    fn wifi_state(&mut self, w: Json) {
        let state = w.get("state").str().unwrap_or("");
        if state == "connecting" {
            self.overlay.wifi_code = None;
        }
        if state == "setup" && w.get("setup").truthy() {
            if let Some(server) = &self.server {
                self.pending.push(Pending::WifiSetup(remote::fetch(server, "/api/wifi/setup")));
            }
        } else {
            self.overlay.wifi_setup = None;
        }
        let last = format!("{:?}", w.get("last"));
        if let Some(prev) = &self.last_wifi
            && *prev != last
            && *w.get("last") != Json::Null
        {
            let l = w.get("last");
            let msg = if l.get("result").str() == Some("joined") {
                format!("Wi-Fi: joined {}", l.get("ssid").str().unwrap_or(""))
            } else {
                format!("Wi-Fi: {}", l.get("error").str().unwrap_or("did not work"))
            };
            self.say(&msg, 8.0);
        }
        self.last_wifi = Some(last);
        if state == "connecting" {
            let msg = format!("Wi-Fi: joining {}...", w.get("ssid").str().unwrap_or(""));
            self.say(&msg, 60.0);
        }
    }

    fn poll_pending(&mut self) {
        let mut still = Vec::new();
        for p in std::mem::take(&mut self.pending) {
            let rx = match &p {
                Pending::Connect(rx, _) | Pending::WifiCode(rx) | Pending::WifiSetup(rx) | Pending::WifiPoll(rx) => rx,
            };
            let Ok(answer) = rx.try_recv() else {
                still.push(p);
                continue;
            };
            let Some(v) = answer else { continue };
            match p {
                Pending::Connect(_, secs) => {
                    let urls = v.get("urls");
                    let mdns = urls.get("mdns").str().unwrap_or("http://ufoscene.local/");
                    let ip = urls.get("ip").str();
                    let mut card = Card::new("ufoscene remote").line(&format!("On your phone, open {mdns}"));
                    if let Some(ip) = ip {
                        card = card.line(&format!("or {ip}"));
                    }
                    card = card.qr(ip.unwrap_or(mdns));
                    if let Some(ver) = v.get("version").str() {
                        card = card.line(&format!("ufoscene-pi {ver}, native player"));
                    }
                    if let Some(s) = secs {
                        card = card.for_secs(s);
                    }
                    self.overlay.connect = Some(card);
                }
                Pending::WifiCode(_) => {
                    if let Some(code) = v.get("code").str().map(str::to_string).or_else(|| v.get("code").num().map(|n| format!("{n:.0}"))) {
                        let secs = v.get("seconds").num().unwrap_or(120.0) as f32;
                        self.overlay.wifi_code = Some(Card::new("Wi-Fi change").line("Type this code on the remote:").line(&code).for_secs(secs));
                    }
                }
                Pending::WifiSetup(_) => {
                    if let (Some(ssid), Some(pw)) = (v.get("ssid").str(), v.get("password").str()) {
                        let esc = |s: &str| s.chars().fold(String::new(), |mut o, c| {
                            if "\\;,:\"".contains(c) {
                                o.push('\\');
                            }
                            o.push(c);
                            o
                        });
                        let ip = v.get("ip").str().unwrap_or("10.42.0.1");
                        let url = format!("http://{ip}/");
                        self.overlay.wifi_setup = Some(
                            Card::new("Wi-Fi setup")
                                .line(&format!("1. Join the network {ssid}, password {pw}"))
                                .line(&format!("2. Open {url} and pick your Wi-Fi"))
                                .qr(&format!("WIFI:T:WPA;S:{};P:{};;", esc(ssid), esc(pw)))
                                .qr(&url),
                        );
                        self.overlay.connect = None;
                    }
                }
                Pending::WifiPoll(_) => self.wifi_state(v.get("wifi").clone()),
            }
        }
        self.pending.extend(still);
    }
}

// ------------------------------------------------------------------ main loops

fn bench(args: &Args, secs: f32) {
    let mut demo = Demo::new();
    demo.seed(1);
    if let Some(d) = args.demo {
        demo.set_demo(d);
    }
    let (w, h) = args.size;
    let mut crt = Crt::new(w, h, args.scanlines);
    let mut out = vec![0u32; w * h];
    let mut analyser = Analyser::new();
    let frames = (secs * 60.0) as usize;
    let (mut render, mut scale) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    let mut worst = (0.0f32, String::new());
    let mut t = 10.0f32;
    // a beat-ish test signal so the effects react
    let mut samples = [0.0f32; FFT];
    for i in 0..frames {
        t += 1.0 / 60.0;
        let beat = (t * 2.0).fract() < 0.15;
        for (k, s) in samples.iter_mut().enumerate() {
            let x = k as f32 / capture::RATE as f32 + t;
            *s = if beat { 0.6 * (x * 60.0 * std::f32::consts::TAU).sin() } else { 0.0 } + 0.1 * (x * 2000.0 * std::f32::consts::TAU).sin();
        }
        analyser.bytes(&samples, &mut demo.audio.bins);
        let a = Instant::now();
        demo.frame(t, true);
        let b = Instant::now();
        crt.draw(&demo.fb.px, &mut out, w);
        let c = Instant::now();
        let ms = (b - a).as_secs_f32() * 1000.0;
        if ms > worst.0 {
            let (di, _, name, _) = demo.state();
            worst = (ms, format!("demo {} {name}", di + 1));
        }
        render.push(ms);
        scale.push((c - b).as_secs_f32() * 1000.0);
        // something new every 4 s, so a short run sees many effects: the next part, then
        // another demo (or only parts, with --demo)
        if i % 240 == 239 {
            if args.demo.is_none() && (i / 240) % 2 == 1 {
                demo.next_demo();
            } else {
                demo.next_part();
            }
        }
    }
    let stat = |v: &mut Vec<f32>| {
        v.sort_by(f32::total_cmp);
        let avg = v.iter().sum::<f32>() / v.len().max(1) as f32;
        (avg, v[v.len() * 95 / 100], v[v.len() - 1])
    };
    let (ra, r95, rmax) = stat(&mut render);
    let (sa, s95, _) = stat(&mut scale);
    println!("{frames} frames, picture {W}x{H} scaled to {w}x{h} (x{}), scanlines {}", crt.fit.scale, args.scanlines);
    println!("  demo frame   avg {ra:6.2} ms   95% {r95:6.2} ms   worst {rmax:6.2} ms ({})", worst.1);
    println!("  scale + CRT  avg {sa:6.2} ms   95% {s95:6.2} ms");
    let total = r95 + s95;
    println!(
        "  => about {:.0} fps possible at the 95th percentile ({}): 60 needs 16.7 ms, 30 needs 33 ms",
        1000.0 / total,
        if total < 16.0 { "60 fps" } else if total < 32.0 { "30 fps" } else { "too slow" }
    );
    if let Some(p) = &args.png {
        let rgba: Vec<u32> = out.iter().map(|&p| 0xFF00_0000 | ((p & 0xFF) << 16) | (p & 0xFF00) | ((p >> 16) & 0xFF)).collect();
        match std::fs::write(p, ufo_demo::png::png(&rgba, w, h, 1)) {
            Ok(()) => println!("wrote {p}"),
            Err(e) => eprintln!("{p}: {e}"),
        }
    }
}

#[cfg(target_os = "linux")]
fn run(args: Args) -> std::io::Result<()> {
    let mut screen = drm::Screen::open(args.card.as_deref(), args.mode.as_deref())?;
    eprintln!("ufo-native: {}", screen.describe());
    let mut crt_on = Crt::new(screen.width(), screen.height(), args.scanlines);
    let mut crt_tv = Crt::new(screen.width(), screen.height(), false);

    let mut settings = Settings::load();
    if let Some(a) = &args.audio
        && a != "none"
    {
        settings.audio = a.clone();
    }
    let mut p = Player::new(&args, settings);
    let commands = p.server.clone().map(remote::listen);
    p.start_audio();
    p.connect_card(Some(10.0));

    let t0 = Instant::now();
    let (mut frames, mut fps_t) = (0u32, Instant::now());
    let mut wifi_t = Instant::now() - Duration::from_secs(60);
    let mut due = 0.0f64;
    let mut flip_err = false;
    while !p.quit {
        let now = t0.elapsed().as_secs_f64() * 1000.0;
        if let Some(rx) = &commands {
            while let Ok(c) = rx.try_recv() {
                p.handle(c, now);
            }
        }
        p.poll_pending();
        if let Some(server) = &p.server
            && wifi_t.elapsed() > Duration::from_secs(15)
        {
            wifi_t = Instant::now();
            p.pending.push(Pending::WifiPoll(remote::fetch(server, "/api/state")));
        }
        let live = p.pump_audio();

        // TV mode: a steady 30 fps grid, as the page does it
        if p.tv {
            if now < due - 2.0 {
                std::thread::sleep(Duration::from_secs_f64((due - 2.0 - now) / 1000.0));
            }
            due = if now - due > 100.0 { now + 1000.0 / 30.0 } else { due + 1000.0 / 30.0 };
        }
        if p.paused_at.is_none() {
            p.demo.frame(((now - p.paused_total) / 1000.0) as f32, live);
            p.shown.px.copy_from_slice(&p.demo.fb.px);
        } else {
            // the last picture stays, with the overlay drawn afresh on top
            p.shown.px.copy_from_slice(&p.demo.fb.px);
        }
        p.overlay.ukraine = p.current_show() == Some(Show::Ukraine);
        p.overlay.draw(&mut p.shown);

        let crt = if p.tv { &mut crt_tv } else { &mut crt_on };
        let (px, pitch) = screen.back();
        crt.draw(&p.shown.px, px, pitch);
        match screen.flip() {
            Ok(()) => flip_err = false,
            Err(e) => {
                // the screen went away (another program took it, the TV was unplugged): retry
                if !flip_err {
                    eprintln!("ufo-native: {e}; trying again");
                }
                flip_err = true;
                std::thread::sleep(Duration::from_millis(200));
            }
        }

        frames += 1;
        if fps_t.elapsed() >= Duration::from_secs(5) {
            let fps = frames as f32 / fps_t.elapsed().as_secs_f32();
            frames = 0;
            fps_t = Instant::now();
            if p.overlay.fps.is_some() {
                p.overlay.fps = Some(fps);
            }
            let show = p.current_show().map_or("null".to_string(), |s| quote(s.cmd()));
            p.report(format!("{{\"fps\": {fps:.1}, \"demo\": {}, \"show\": {show}}}", p.demo.current() + 1));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn run(_args: Args) -> std::io::Result<()> {
    Err(std::io::Error::other("the TV output needs Linux (DRM/KMS); on this machine use --bench"))
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("ufo-native: {e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if let Some(secs) = args.bench {
        bench(&args, secs);
        return;
    }
    if let Err(e) = run(args) {
        eprintln!("ufo-native: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn christmas() {
        // 2026-12-23 00:00 UTC and 2026-12-29 00:00 UTC
        assert!(christmas_week(1_797_984_000));
        assert!(!christmas_week(1_797_984_000 + 6 * 86_400));
        assert!(!christmas_week(1_797_984_000 - 86_400));
    }

    #[test]
    fn tracks_fit_the_buffer() {
        let mut d = Demo::new();
        set_track(&mut d, &"Å".repeat(600), "x");
        set_track(&mut d, "Roxette", "Joyride");
    }
}
