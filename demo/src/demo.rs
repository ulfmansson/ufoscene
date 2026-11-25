//! Plays the demos: runs one recipe's parts in order, switching on the beat,
//! draws the logo, scroller, title card and "now playing" band over them, and
//! moves on to the next demo when one ends (or when the page says the song changed).
//! When the page hands over an artist picture, a picture part is slipped in at
//! the next switch, and again every few parts after that.

use crate::audio::Audio;
use crate::eggs::{self, Eggs};
use crate::font::{self, Logo, ScrollStyle, Scroller};
use crate::fx::{self, Ctx, Def, Effect, Image, Palette, Rng};
use crate::gfx::*;
use crate::recipes::{self, Recipe};

/// What happens when a demo has played all its parts.
pub const NEXT: u8 = 0;
pub const REPEAT: u8 = 1;
pub const SHUFFLE: u8 = 2;

pub const IMAGE_SIZE: usize = 256;

/// Where the code lives; shown now and then (font-safe upper case).
pub const HOME: &str = "GITHUB.COM/ULFMANSSON/UFOSCENE";
/// The website, for the QR code (Q) and share texts.
pub const SITE_URL: &str = "https://ufoscene.net";
pub const SITE: &str = "UFOSCENE.NET";
/// How long the QR code stays up after Q (seconds).
const QR_SECONDS: f32 = 10.0;
/// How often the "share UFO" band shows (seconds), and for how long.
const PROMO_EVERY: f32 = 420.0;
const PROMO_LEN: f32 = 8.0;
const SHARE_LINES: [&str; 5] = [
    "LIKE IT? SHARE UFOSCENE.NET WITH A FRIEND!",
    "TELL YOUR FRIENDS ABOUT UFOSCENE.NET!",
    "DELA UFOSCENE.NET MED DINA VÄNNER!",
    "SPREAD THE WORD - #UFOSCENE",
    "SHOW UFOSCENE.NET AT YOUR NEXT PARTY!",
];

/// The promo band at `secs` since the start: which one (0, 1, 2, ...) and how far into it,
/// or None. The first comes after 4 minutes, then one every 7 minutes; every third one shows
/// the GitHub address instead of a share line.
pub fn promo_at(secs: f32) -> Option<(u32, f32)> {
    let first = 240.0;
    if secs < first {
        return None;
    }
    let k = ((secs - first) / PROMO_EVERY).floor();
    let dt = secs - first - k * PROMO_EVERY;
    (dt < PROMO_LEN).then_some((k as u32, dt))
}
pub const TEXT_BUF: usize = 1024;

pub struct Demo {
    pub fb: Fb,
    pub audio: Audio,
    defs: Vec<Def>,
    pub recipes: Vec<Recipe>,
    pals: Vec<Palette>,
    image_fx: Vec<usize>,
    demo: usize,
    part: usize,
    fx: Box<dyn Effect>,
    fx_idx: usize,
    pal_idx: usize,
    in_image_part: bool,
    t: f32,
    last: Option<f32>,
    start: Option<f32>,
    demo_t0: f32,
    part_t0: f32,
    part_beat0: u32,
    pub hud: bool,
    hud_until: f32,
    logo: Logo,
    title: Logo,
    float_logo: Logo,
    scroll: Scroller,
    /// `NEXT`, `REPEAT` or `SHUFFLE`.
    pub jukebox: u8,
    rng: Rng,
    /// The page writes a 256×256 RGBA picture here, then calls `image_ready`.
    pub image_buf: Vec<u32>,
    /// The page writes UTF-8 "artist\ntrack" here, then calls `set_track`.
    pub text_buf: Vec<u8>,
    image: Option<Image>,
    image_due: bool,
    parts_since_image: u32,
    image_turn: u32,
    track: Option<(String, String)>,
    track_t: f32,
    /// One line the page adds to the end of every scroll text (`set_scroll_extra`).
    scroll_extra: String,
    list: Vec<u8>,
    qr: Option<crate::qr::Qr>,
    qr_since: Option<f32>,
    /// Easter eggs: snow, a UFO flying by, fireworks and so on.
    pub eggs: Eggs,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

struct Blank;
impl Effect for Blank {
    fn draw(&mut self, fb: &mut Fb, _: &Ctx) {
        fb.clear(0);
    }
}

fn top_logo(name: &str) -> Logo {
    Logo::new(name, (330 / font::text_width(name, 1, true).max(1)).clamp(2, 4))
}

impl Demo {
    pub fn new() -> Self {
        let defs = fx::all();
        let recipes = recipes::build(&defs);
        let image_fx = (0..defs.len()).filter(|&i| defs[i].flags & fx::IMAGE != 0).collect();
        let list = recipes
            .iter()
            .map(|r| {
                let parts: Vec<&str> = r.parts.iter().map(|p| defs[p.0].name).collect();
                format!("{}\t{}\t{}\n", r.name, r.group, parts.join(", "))
            })
            .collect::<String>()
            .into_bytes();
        let mut d = Demo {
            fb: Fb::new(),
            audio: Audio::new(),
            defs,
            recipes,
            pals: (0..16).map(fx::palette).collect(),
            image_fx,
            demo: 0,
            part: 0,
            fx: Box::new(Blank),
            fx_idx: 0,
            pal_idx: 0,
            in_image_part: false,
            t: 0.0,
            last: None,
            start: None,
            demo_t0: 0.0,
            part_t0: 0.0,
            part_beat0: 0,
            hud: false,
            hud_until: 15.0,
            logo: top_logo("UFO"),
            title: Logo::new("UFO", 6),
            float_logo: Logo::new("UFO", 3),
            scroll: Scroller::new(""),
            jukebox: NEXT,
            rng: Rng::new(2024),
            image_buf: vec![0; IMAGE_SIZE * IMAGE_SIZE],
            text_buf: vec![0; TEXT_BUF],
            image: None,
            image_due: false,
            parts_since_image: 0,
            image_turn: 0,
            track: None,
            track_t: -100.0,
            scroll_extra: String::new(),
            list,
            qr: crate::qr::encode(SITE_URL.as_bytes()),
            qr_since: None,
            eggs: Eggs::new(),
        };
        d.set_demo(0);
        d
    }

    pub fn count(&self) -> usize {
        self.recipes.len()
    }

    pub fn current(&self) -> usize {
        self.demo
    }

    /// "title\tgroup\tpart, part, ...\n" for every demo.
    pub fn list(&self) -> &[u8] {
        &self.list
    }

    pub fn set_demo(&mut self, i: usize) {
        self.demo = i % self.recipes.len();
        let r = &self.recipes[self.demo];
        self.logo = top_logo(r.name);
        self.title = Logo::new(r.name, (340 / font::text_width(r.name, 1, true).max(1)).clamp(2, 6));
        self.float_logo = Logo::new(r.name, (150 / font::text_width(r.name, 1, true).max(1)).clamp(1, 3));
        self.scroll = Scroller::new(&self.scroll_text());
        self.demo_t0 = self.t;
        self.start_part(0);
    }

    pub fn next_demo(&mut self) {
        self.set_demo(self.demo + 1);
    }

    pub fn prev_demo(&mut self) {
        self.set_demo(self.demo + self.recipes.len() - 1);
    }

    pub fn random_demo(&mut self) {
        let n = self.recipes.len() as u32;
        let i = (self.demo as u32 + 1 + self.rng.below(n - 1)) % n;
        self.set_demo(i as usize);
    }

    /// Seed the random choices (shuffle, the first demo, eggs), so each start is different.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Rng::new(seed);
    }

    fn start_part(&mut self, p: usize) {
        let r = &self.recipes[self.demo];
        self.part = p.min(r.parts.len() - 1);
        let (idx, seed) = r.parts[self.part];
        self.begin(idx, seed, r.pal[self.part % 2]);
        self.in_image_part = false;
        self.parts_since_image += 1;
    }

    fn begin(&mut self, idx: usize, seed: u32, pal: usize) {
        self.fx_idx = idx;
        self.fx = (self.defs[idx].make)(seed);
        self.pal_idx = pal;
        self.part_t0 = self.t;
        self.part_beat0 = self.audio.beats;
    }

    /// Start an easter egg (`eggs::FLYBY`, ...) now.
    pub fn egg(&mut self, kind: u32, arg: u32) {
        self.eggs.start(kind, arg, self.t);
    }

    /// Next part (the N key). Slips in a picture part when one is due.
    pub fn next_part(&mut self) {
        // now and then, after the first couple of minutes, a UFO flies by on its own
        let running = self.t - self.start.unwrap_or(self.t);
        if running > 120.0 && self.eggs.active(self.t).is_none() && self.rng.below(36) == 0 {
            self.eggs.start(eggs::FLYBY, 0, self.t);
        }
        let has_image = self.image.is_some() && !self.image_fx.is_empty();
        if has_image && !self.in_image_part && (self.image_due || self.parts_since_image >= 3) {
            let idx = self.image_fx[self.image_turn as usize % self.image_fx.len()];
            self.image_turn += 1;
            let pal = self.recipes[self.demo].pal[0];
            self.begin(idx, self.image_turn, pal);
            self.in_image_part = true;
            self.image_due = false;
            self.parts_since_image = 0;
            return;
        }
        let next = self.part + 1;
        if next < self.recipes[self.demo].parts.len() {
            self.start_part(next);
        } else {
            match self.jukebox {
                REPEAT => self.set_demo(self.demo),
                SHUFFLE => self.random_demo(),
                _ => self.next_demo(),
            }
        }
    }

    /// Show the QR code to the website for 10 seconds (Q); Q again hides it.
    pub fn toggle_qr(&mut self) {
        self.qr_since = if self.qr_since.is_some() { None } else { Some(self.t) };
    }

    pub fn toggle_hud(&mut self) {
        self.hud = !self.hud;
        self.hud_until = 0.0;
    }

    /// The page wrote "artist\ntrack" (UTF-8, `len` bytes) into `text_buf`.
    pub fn set_track(&mut self, len: usize) {
        let s = String::from_utf8_lossy(&self.text_buf[..len.min(TEXT_BUF)]).to_string();
        let mut it = s.splitn(2, '\n');
        let artist = font::clean(it.next().unwrap_or(""), 40);
        let track = font::clean(it.next().unwrap_or(""), 48);
        self.track = Some((artist, track));
        self.track_t = self.t;
    }

    /// The page wrote a new 256×256 picture into `image_buf`.
    /// The page wrote one UTF-8 line into `text_buf` to add to every scroll text
    /// (a website's "support" line); an empty line removes it. Restarts the scroller.
    pub fn set_scroll_extra(&mut self, len: usize) {
        let s = String::from_utf8_lossy(&self.text_buf[..len.min(TEXT_BUF)]).to_string();
        self.scroll_extra = font::clean(s.trim(), 120);
        self.scroll = Scroller::new(&self.scroll_text());
    }

    fn scroll_text(&self) -> String {
        let text = &self.recipes[self.demo].text;
        if self.scroll_extra.is_empty() { text.clone() } else { format!("{text}     {}", self.scroll_extra) }
    }

    /// `credit_len` bytes of `text_buf` hold the picture's credit line (UTF-8), or 0 for none.
    pub fn image_ready(&mut self, credit_len: usize) {
        let id = self.image.as_ref().map_or(1, |i| i.id + 1);
        let caption = self.track.as_ref().map(|t| t.0.clone()).unwrap_or_default();
        let credit = font::clean(&String::from_utf8_lossy(&self.text_buf[..credit_len.min(TEXT_BUF)]), 62);
        self.image = Some(Image { px: self.image_buf.clone(), id, caption, credit });
        self.image_due = true;
    }

    /// Render one frame. `t` is in seconds; `has_audio` says whether `audio.bins` holds live data.
    pub fn frame(&mut self, t: f32, has_audio: bool) {
        let dt = self.last.map_or(0.0, |l| (t - l).clamp(0.0, 0.1));
        self.last = Some(t);
        if self.start.is_none() {
            self.start = Some(t);
            self.demo_t0 = t;
            self.part_t0 = t;
            self.hud_until = t + 15.0;
        }
        self.t = t;
        self.audio.update(t, dt, has_audio);

        let st = t - self.part_t0;
        let sb = self.audio.beats.wrapping_sub(self.part_beat0);
        // picture parts are shorter; normal parts last 12 s and 32 beats, at most 45 s
        let (min_t, max_t) = if self.in_image_part { (8.0, 20.0) } else { (12.0, 45.0) };
        if (self.audio.beat && st > min_t && sb >= 32) || st > max_t || (self.in_image_part && st > min_t && self.audio.beat && sb >= 16) {
            self.next_part();
        }
        let st = t - self.part_t0;
        let flags = self.defs[self.fx_idx].flags;
        let r = &self.recipes[self.demo];
        let ctx = Ctx {
            t: st,
            time: t - self.demo_t0,
            dt,
            a: &self.audio,
            pal: &self.pals[self.pal_idx],
            beats: self.audio.beats.wrapping_sub(self.part_beat0),
            phase: self.audio.phase(t),
            name: r.name,
            text: &r.text,
            image: self.image.as_ref(),
        };
        self.fx.draw(&mut self.fb, &ctx);

        self.scroll.advance(dt, 75.0 + self.audio.pulse * 90.0, 3);
        if flags & fx::OWN_TEXT == 0 {
            self.overlays(flags);
        }
        let dtime = t - self.demo_t0;
        if dtime < 5.0 && self.part == 0 && !self.in_image_part {
            self.title_card(dtime);
        }
        let playing = self.track.is_some() && !self.in_image_part && self.now_playing(t - self.track_t);
        if !playing {
            if let Some((k, dt)) = promo_at(t - self.start.unwrap_or(t)) {
                self.promo(k, dt);
            }
        }
        if st < 0.6 {
            self.transition(st / 0.6);
        }
        self.eggs.draw(&mut self.fb, t, dt, &self.audio, &self.pals[self.pal_idx]);
        if let Some(t0) = self.qr_since {
            if t - t0 < QR_SECONDS && t >= t0 {
                self.draw_qr(t - t0);
            } else {
                self.qr_since = None;
            }
        }
        let since_start = t - self.start.unwrap_or(t);
        if since_start < 1.5 {
            self.fb.darken(since_start / 1.5);
        }
        if self.hud || t < self.hud_until {
            self.draw_hud();
        }
        self.fb.quantize_ocs();
    }

    fn overlays(&mut self, flags: u32) {
        let (t, a) = (self.t, &self.audio);
        let r = &self.recipes[self.demo];
        if r.logo == 0 && flags & fx::LOGO_TOP != 0 {
            self.logo.draw(&mut self.fb, WI / 2, 6 - (a.pulse * 3.0) as i32, t, 1.0 + a.bass * 3.0);
        } else if r.logo == 1 && self.part % 2 == 0 {
            let x = WI / 2 + ((t * 1.3).cos() * 100.0) as i32;
            let y = HI / 2 - self.float_logo.h / 2 + ((t * 2.0).sin() * 50.0) as i32;
            self.float_logo.draw(&mut self.fb, x, y, t, 1.0 + a.bass * 3.0);
        }
        if flags & fx::SCROLL_BOTTOM != 0 {
            let st = ScrollStyle { y: H as f32 - 27.0, sc: 3, amp: 4.0 + a.mid * 2.0, freq: 0.035, speed: 3.0 };
            self.scroll.draw(&mut self.fb, &st, t);
        } else if self.part % 2 == 1 {
            let st = ScrollStyle { y: H as f32 / 2.0 - 10.0, sc: 3, amp: 24.0 + a.mid * 10.0, freq: 0.03, speed: 2.5 };
            self.scroll.draw(&mut self.fb, &st, t);
        }
    }

    /// "<GROUP> PRESENTS" and the title, over the first part of each demo.
    fn title_card(&mut self, dt: f32) {
        let r = &self.recipes[self.demo];
        let k = (dt / 0.4).min(1.0).min((5.0 - dt) / 0.6).clamp(0.0, 1.0);
        let half = (k * 44.0) as i32;
        let cy = HI / 2;
        for y in cy - half..cy + half {
            self.fb.hspan_blend(0, WI, y, rgb(0, 0, 0), 0.7);
        }
        if k < 0.9 {
            return;
        }
        let pres = format!("{} PRESENTS", r.group);
        let w = font::text_width(&pres, 1, false);
        font::text(&mut self.fb, &pres, (WI - w) / 2, cy - 38, 1, false, self.pals[r.pal[0]].hi);
        let num = format!("A HYMN TO THE SWEDISH DEMO SCENE - DEMO {} OF {}", self.demo + 1, self.recipes.len());
        let w = font::text_width(&num, 1, false);
        font::text(&mut self.fb, &num, (WI - w) / 2, cy + 32, 1, false, rgb(200, 200, 200));
        self.title.draw(&mut self.fb, WI / 2, cy - self.title.h / 2 + 2, self.t, 1.0 + self.audio.bass * 3.0);
    }

    /// "NOW PLAYING" band at the top, the artist and song in dancing letters. It shows for
    /// 10 s when the song changes, then for 7 s about every 50 s.
    /// Returns whether the band is on screen.
    fn now_playing(&mut self, since: f32) -> bool {
        let (start, len) = if since < 10.0 { (0.0, 10.0) } else { (50.0 + ((since - 50.0) / 50.0).floor() * 50.0, 7.0) };
        let dt = since - start;
        if !(0.0..len).contains(&dt) {
            return false;
        }
        let Some((artist, track)) = &self.track else { return false };
        let fit = |s: &str, max: f32| ((W as f32 - 24.0) / (s.chars().count().max(1) as f32 * 7.0)).clamp(1.0, max);
        let (sa, ss) = (fit(artist, 3.0), fit(track, 2.0));
        let h = 12.0 + 7.0 * sa + 8.0 + if track.is_empty() { 0.0 } else { 7.0 * ss + 6.0 };
        let slide = smoothstep(dt / 0.6).min(smoothstep((len - dt) / 0.6));
        let y0 = -h + slide * (h + 4.0);
        for y in y0 as i32..(y0 + h) as i32 {
            self.fb.hspan_blend(0, WI, y, rgb(0, 0, 0), 0.72);
        }
        let pal = &self.pals[self.pal_idx];
        self.fb.hspan(0, WI, (y0 + h) as i32, pal.hi);
        font::text(&mut self.fb, "NOW PLAYING", 10, y0 as i32 + 4, 1, false, rgb(170, 170, 170));
        let (t, beats) = (self.t, self.audio.beats);
        let wa = artist.chars().count() as f32 * 7.0 * sa;
        fx::dancing_text(&mut self.fb, artist, (W as f32 - wa) / 2.0, y0 + 14.0, sa, t, &self.audio, beats, pal);
        if !track.is_empty() {
            let ws = track.chars().count() as f32 * 7.0 * ss;
            // the song dances half a beat behind the artist
            fx::dancing_text(&mut self.fb, track, (W as f32 - ws) / 2.0, y0 + 14.0 + 7.0 * sa + 8.0, ss, t + 0.3, &self.audio, beats + 3, pal);
        }
        true
    }

    /// Now and then, a band at the top asks people to share UFO; every third time it
    /// shows where the code is instead.
    fn promo(&mut self, k: u32, dt: f32) {
        let github = k % 3 == 2;
        let (label, line) = if github {
            ("FREE AND OPEN SOURCE", HOME)
        } else {
            ("PSST ...", SHARE_LINES[(k as usize) % SHARE_LINES.len()])
        };
        let sc = ((W as f32 - 24.0) / (line.chars().count() as f32 * 7.0)).clamp(1.0, 2.0);
        let h = 12.0 + 7.0 * sc + 10.0;
        let slide = smoothstep(dt / 0.6).min(smoothstep((PROMO_LEN - dt) / 0.6));
        let y0 = -h + slide * (h + 4.0);
        for y in y0 as i32..(y0 + h) as i32 {
            self.fb.hspan_blend(0, WI, y, rgb(0, 0, 0), 0.72);
        }
        let pal = &self.pals[self.pal_idx];
        self.fb.hspan(0, WI, (y0 + h) as i32, pal.hi);
        font::text(&mut self.fb, label, 10, y0 as i32 + 4, 1, false, rgb(170, 170, 170));
        let w = line.chars().count() as f32 * 7.0 * sc;
        fx::dancing_text(&mut self.fb, line, (W as f32 - w) / 2.0, y0 + 14.0, sc, self.t, &self.audio, self.audio.beats, pal);
    }

    /// The QR code to the site on a white card, "#UFOSCENE" dancing under it.
    fn draw_qr(&mut self, dt: f32) {
        let Some(qr) = &self.qr else { return };
        let k = smoothstep(dt / 0.4).min(smoothstep((QR_SECONDS - dt) / 0.4));
        self.fb.darken(1.0 - 0.82 * k);
        if k < 0.05 {
            return;
        }
        let quiet = 3; // light border around the code, in modules
        let m = 4i32; // pixels per module
        let side = (qr.size as i32 + quiet * 2) * m;
        let x0 = (WI - side) / 2;
        let y0 = 10 + ((1.0 - k) * -40.0) as i32;
        self.fb.fill_rect(x0 - 2, y0 - 2, side + 4, side + 4, self.pals[self.pal_idx].hi);
        self.fb.fill_rect(x0, y0, side, side, rgb(255, 255, 255));
        for y in 0..qr.size {
            for x in 0..qr.size {
                if qr.get(x, y) {
                    self.fb.fill_rect(x0 + (quiet + x as i32) * m, y0 + (quiet + y as i32) * m, m, m, rgb(0, 0, 0));
                }
            }
        }
        let ty = (y0 + side + 8) as f32;
        let tag = "#UFOSCENE";
        let w = tag.len() as f32 * 7.0 * 3.0;
        fx::dancing_text(&mut self.fb, tag, (W as f32 - w) / 2.0, ty, 3.0, self.t, &self.audio, self.audio.beats, &self.pals[self.pal_idx]);
        let line = format!("SCAN ME - {SITE} - SHARE WITH #UFOSCENE");
        let lw = font::text_width(&line, 1, false);
        font::text(&mut self.fb, &line, (WI - lw) / 2, ty as i32 + 28, 1, false, rgb(230, 230, 230));
    }

    fn transition(&mut self, p: f32) {
        let style = if self.in_image_part { 1 } else { self.recipes[self.demo].transition };
        let fb = &mut self.fb;
        match style {
            1 => {
                fb.darken(smoothstep(p));
                fb.flash((1.0 - p) * 0.2);
            }
            2 => {
                for (i, px) in fb.px.iter_mut().enumerate() {
                    let h = (i as u32).wrapping_mul(2_654_435_761) >> 24;
                    if h as f32 / 255.0 > p {
                        *px = rgb(0, 0, 0);
                    }
                }
            }
            3 => {
                let edge = p * (W as f32 + 80.0);
                for y in 0..HI {
                    let e = (edge - y as f32 * 0.35) as i32;
                    fb.hspan(e.max(0), WI, y, rgb(0, 0, 0));
                    fb.hspan(e - 2, e, y, rgb(255, 255, 255));
                }
            }
            4 => {
                let b = ((1.0 - p) * 24.0) as usize + 1;
                if b > 1 {
                    for by in (0..H).step_by(b) {
                        for bx in (0..W).step_by(b) {
                            let c = fb.px[by * W + bx];
                            for y in by..(by + b).min(H) {
                                fb.px[y * W + bx..y * W + (bx + b).min(W)].fill(c);
                            }
                        }
                    }
                }
            }
            _ => {
                fb.flash((1.0 - p) * 0.5);
                for y in 0..HI {
                    if (y % 12) as f32 >= p * 12.0 {
                        fb.row(y, rgb(0, 0, 0));
                    }
                }
            }
        }
    }

    fn draw_hud(&mut self) {
        let a = &self.audio;
        let r = &self.recipes[self.demo];
        let mode = if a.autopilot { "AUTOPILOT" } else { "LIVE" };
        let jb = ["NEXT", "REPEAT", "SHUFFLE"][self.jukebox as usize % 3];
        let part = if self.in_image_part { "PICTURE".to_string() } else { format!("PART {}/{}", self.part + 1, r.parts.len()) };
        let l1 = format!("DEMO {}/{} {} BY {}", self.demo + 1, self.recipes.len(), r.name, r.group);
        let l2 = format!("{part} {}  {}  {} BPM  {mode}  THEN {jb}", self.defs[self.fx_idx].name, fx::PALETTE_NAMES[self.pal_idx], (60.0 / a.period).round());
        for (i, s) in [l1, l2].iter().enumerate() {
            let y = HI - 22 + i as i32 * 10;
            let w = font::text_width(s, 1, false);
            self.fb.fill_rect(2, y - 2, w + 6, 10, rgb(0, 0, 0));
            font::text(&mut self.fb, s, 5, y, 1, false, rgb(255, 255, 255));
        }
        let levels = [(a.bass, rgb(255, 80, 80)), (a.mid, rgb(255, 220, 80)), (a.treble, rgb(80, 200, 255))];
        for (i, (v, c)) in levels.iter().enumerate() {
            let x = WI - 30 + i as i32 * 9;
            self.fb.fill_rect(x, HI - 42, 7, 40, rgb(0, 0, 0));
            let h = (v * 38.0) as i32;
            self.fb.fill_rect(x + 1, HI - 3 - h, 5, h, *c);
        }
        if a.pulse > 0.5 {
            self.fb.fill_rect(WI - 42, HI - 10, 8, 8, rgb(255, 255, 255));
        }
    }

    /// For tests and tools: which demo / part / effect is on screen.
    pub fn state(&self) -> (usize, usize, &'static str, bool) {
        (self.demo, self.part, self.defs[self.fx_idx].name, self.in_image_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Demo, t: &mut f32, secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            d.frame(*t, false);
            *t += 1.0 / 60.0;
        }
    }

    #[test]
    fn every_demo_starts_and_switches() {
        let mut d = Demo::new();
        let mut t = 10.0;
        for i in 0..d.count() {
            d.set_demo(i);
            run(&mut d, &mut t, 0.3);
            assert_eq!(d.state().0, i);
            d.next_part();
            run(&mut d, &mut t, 0.2);
            assert_eq!(d.state().1, 1, "demo {i} should be on part 2");
        }
    }

    #[test]
    fn demo_ends_into_next_and_pictures_slip_in() {
        let mut d = Demo::new();
        let mut t = 10.0;
        run(&mut d, &mut t, 0.5);
        let n = d.recipes[0].parts.len();
        for _ in 0..n {
            d.next_part();
        }
        assert_eq!(d.state().0, 1, "after the last part comes the next demo");
        // a picture arrives: the next switch shows it
        let len = "ABBA\nSOS".len();
        d.text_buf[..len].copy_from_slice(b"ABBA\nSOS");
        d.set_track(len);
        d.image_buf.fill(rgb(200, 40, 40));
        d.image_ready(0);
        d.next_part();
        assert!(d.state().3, "a new picture should get its own part");
        run(&mut d, &mut t, 1.0);
        d.next_part();
        assert!(!d.state().3);
    }

    #[test]
    fn promo_is_seldom_and_github_rarer() {
        assert_eq!(promo_at(10.0), None, "nothing in the first minutes");
        assert!(promo_at(242.0).is_some());
        let secs = (0..3600).filter(|&s| promo_at(s as f32).is_some()).count();
        assert!(secs <= 3600 / 50, "on screen at most 2% of an hour, got {secs} s");
        let starts = (0..3600).filter_map(|s| promo_at(s as f32)).filter(|p| p.1 < 1.0);
        let github = starts.filter(|p| p.0 % 3 == 2).count();
        assert_eq!(github, 2, "the GitHub address about every 20 minutes");
        assert!(HOME.chars().all(|c| crate::font::glyph(c) != [0; 7]));
        assert!(SHARE_LINES.iter().flat_map(|l| l.chars()).all(|c| c == ' ' || crate::font::glyph(c) != [0; 7]));
    }

    #[test]
    fn qr_shows_for_ten_seconds() {
        let mut d = Demo::new();
        let mut t = 10.0;
        run(&mut d, &mut t, 0.5);
        assert!(d.qr.as_ref().is_some_and(|q| q.size == 25));
        d.toggle_qr();
        run(&mut d, &mut t, 2.0);
        // the card is white in the middle of the top area
        assert_eq!(d.fb.px[(14 * W) + W / 2] & 0x00FF_FFFF, 0x00FF_FFFF);
        run(&mut d, &mut t, 9.0);
        assert!(d.qr_since.is_none(), "gone after 10 s");
    }

    #[test]
    fn scroll_extra_line_is_added_and_removed() {
        let mut d = Demo::new();
        let line = b"LIKE IT? BUY THE CODER A COFFEE AT KO-FI.COM/UFOSCENE";
        d.text_buf[..line.len()].copy_from_slice(line);
        d.set_scroll_extra(line.len());
        assert!(d.scroll_text().ends_with("KO-FI.COM/UFOSCENE"));
        d.set_scroll_extra(0);
        assert_eq!(d.scroll_text(), d.recipes[d.current()].text);
    }

    #[test]
    fn parts_switch_on_their_own() {
        let mut d = Demo::new();
        let mut t = 10.0;
        run(&mut d, &mut t, 50.0);
        assert!(d.state().1 >= 1, "the first part should have ended within 45 s");
    }
}
