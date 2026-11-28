//! What the TV page draws in HTML on top of the demos, drawn here in the demos' own pixel font
//! straight into the 384×216 picture: short messages, the connect card with the remote's
//! address and QR code, the Wi-Fi cards, Bluetooth pairing, the self-test's fps and the
//! Ukraine show's corner.

use std::time::{Duration, Instant};

use ufo_demo::font::{self, GH};
use ufo_demo::gfx::{Fb, HI, WI, rgb};
use ufo_demo::qr::{self, Qr};

const BLUE: u32 = 0xFFAA_5500; // #0055aa, the page's panel
const WHITE: u32 = 0xFFFF_FFFF;
const ORANGE: u32 = 0xFF00_88FF; // #ff8800
const MAX_CHARS: usize = 60;

pub struct Card {
    /// (text, colour); the first line is the heading
    pub lines: Vec<(String, u32)>,
    pub qrs: Vec<Qr>,
    pub until: Option<Instant>,
}

impl Card {
    pub fn new(title: &str) -> Card {
        Card { lines: vec![(font::clean(title, MAX_CHARS), ORANGE)], qrs: Vec::new(), until: None }
    }

    pub fn line(mut self, s: &str) -> Card {
        for l in wrap(s, MAX_CHARS) {
            self.lines.push((l, WHITE));
        }
        self
    }

    pub fn qr(mut self, text: &str) -> Card {
        if let Some(q) = qr::encode(text.as_bytes()) {
            self.qrs.push(q);
        }
        self
    }

    pub fn for_secs(mut self, s: f32) -> Card {
        self.until = Some(Instant::now() + Duration::from_secs_f32(s));
        self
    }

    fn live(&self) -> bool {
        self.until.is_none_or(|t| Instant::now() < t)
    }
}

/// Upper-case and wrap at word boundaries into lines of at most `max` characters.
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in font::clean(s, 4 * max).split(' ').filter(|w| !w.is_empty()) {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word.chars().take(max).collect::<String>());
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[derive(Default)]
pub struct Overlay {
    toast: Option<(Vec<String>, Instant)>,
    pub fps: Option<f32>,
    pub ukraine: bool,
    /// the cards by importance: the first live one is drawn
    pub wifi_setup: Option<Card>,
    pub wifi_code: Option<Card>,
    pub pairing: Option<Card>,
    pub connect: Option<Card>,
}

fn panel(fb: &mut Fb, x: i32, y: i32, w: i32, h: i32) {
    fb.fill_rect(x + 3, y + 3, w, h, rgb(0, 0, 0));
    fb.fill_rect(x - 1, y - 1, w + 2, h + 2, WHITE);
    fb.fill_rect(x, y, w, h, BLUE);
}

fn draw_qr(fb: &mut Fb, q: &Qr, x: i32, y: i32, m: i32) {
    let side = (q.size as i32 + 4) * m;
    fb.fill_rect(x, y, side, side, WHITE);
    for qy in 0..q.size {
        for qx in 0..q.size {
            if q.get(qx, qy) {
                fb.fill_rect(x + (qx as i32 + 2) * m, y + (qy as i32 + 2) * m, m, m, rgb(0, 0, 0));
            }
        }
    }
}

fn draw_card(fb: &mut Fb, c: &Card) {
    const LH: i32 = GH + 3;
    let m = if c.qrs.len() > 1 { 1 } else { 2 };
    let qr_side = |q: &Qr| (q.size as i32 + 4) * m;
    let qrs_w: i32 = c.qrs.iter().map(|q| qr_side(q) + 8).sum::<i32>() - if c.qrs.is_empty() { 0 } else { 8 };
    let qrs_h = c.qrs.iter().map(qr_side).max().unwrap_or(0);
    let text_w = c.lines.iter().map(|(s, _)| font::text_width(s, 1, false)).max().unwrap_or(0);
    let w = (text_w.max(qrs_w) + 16).min(WI - 8);
    let h = (c.lines.len() as i32 * LH + if qrs_h > 0 { qrs_h + 6 } else { 0 } + 10).min(HI - 8);
    let (x, y) = ((WI - w) / 2, (HI - h) / 2);
    panel(fb, x, y, w, h);
    let mut ty = y + 6;
    for (s, col) in &c.lines {
        font::text(fb, s, x + (w - font::text_width(s, 1, false)) / 2, ty, 1, false, *col);
        ty += LH;
    }
    let mut qx = x + (w - qrs_w) / 2;
    for q in &c.qrs {
        draw_qr(fb, q, qx, ty + 2, m);
        qx += qr_side(q) + 8;
    }
}

impl Overlay {
    pub fn say(&mut self, text: &str, secs: f32) {
        eprintln!("ufo-native: {text}");
        self.toast = Some((wrap(text, MAX_CHARS), Instant::now() + Duration::from_secs_f32(secs)));
    }

    pub fn draw(&mut self, fb: &mut Fb) {
        let now = Instant::now();
        for slot in [&mut self.wifi_setup, &mut self.wifi_code, &mut self.pairing, &mut self.connect] {
            if slot.as_ref().is_some_and(|c| !c.live()) {
                *slot = None;
            }
        }
        if let Some(c) = [&self.wifi_setup, &self.wifi_code, &self.pairing, &self.connect].into_iter().flatten().next() {
            draw_card(fb, c);
        }
        if self.ukraine {
            let s = "UKRAINE.UFOSCENE.NET";
            let w = font::text_width(s, 1, false);
            fb.fill_rect(WI - w - 26, 4, 12, 4, rgb(0x00, 0x57, 0xB7));
            fb.fill_rect(WI - w - 26, 8, 12, 4, rgb(0xFF, 0xD7, 0x00));
            font::text(fb, s, WI - w - 8, 5, 1, false, WHITE);
        }
        if let Some(fps) = self.fps {
            let s = format!("{fps:.0} FPS");
            let col = if fps >= 25.0 { rgb(0x66, 0xFF, 0x66) } else if fps >= 20.0 { rgb(0xFF, 0xCC, 0x33) } else { rgb(0xFF, 0x44, 0x44) };
            fb.fill_rect(4, 4, font::text_width(&s, 2, false) + 6, GH * 2 + 6, rgb(0, 0, 0));
            font::text(fb, &s, 7, 7, 2, false, col);
        }
        if let Some((lines, until)) = &self.toast {
            if now >= *until {
                self.toast = None;
                return;
            }
            let lh = GH + 3;
            let w = lines.iter().map(|s| font::text_width(s, 1, false)).max().unwrap_or(0) + 12;
            let h = lines.len() as i32 * lh + 6;
            let (x, y) = ((WI - w) / 2, HI - h - 10);
            for yy in y..y + h {
                fb.hspan_blend(x, x + w, yy, rgb(0, 0, 0), 0.75);
            }
            for (i, s) in lines.iter().enumerate() {
                font::text(fb, s, x + (w - font::text_width(s, 1, false)) / 2, y + 4 + i as i32 * lh, 1, false, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("Radio: Heart 80s. Space = next station", 20), vec!["RADIO: HEART 80S.", "SPACE = NEXT STATION"]);
        assert_eq!(wrap("", 10), Vec::<String>::new());
        assert_eq!(wrap("abcdefghijkl", 5), vec!["ABCDE"]);
    }

    #[test]
    fn cards_and_toasts_draw_inside_the_picture() {
        let mut fb = Fb::new();
        let mut o = Overlay::default();
        o.connect = Some(Card::new("Remote").line("Open http://ufoscene.local/ on your phone").line("or http://192.168.1.23/").qr("http://192.168.1.23/"));
        o.say("Internet radio: Heart 80s", 3.0);
        o.fps = Some(57.0);
        o.ukraine = true;
        o.draw(&mut fb);
        assert!(fb.px.iter().any(|&p| p == BLUE));
        o.wifi_setup = Some(Card::new("Wi-Fi setup").line("Join UFOSCENE-SETUP").qr("WIFI:T:WPA;S:UFOSCENE-SETUP;P:secret;;").qr("http://10.42.0.1/"));
        o.draw(&mut fb);
        o.connect = Some(Card::new("gone").for_secs(0.0));
        o.wifi_setup = None;
        std::thread::sleep(Duration::from_millis(5));
        o.draw(&mut fb);
        assert!(o.connect.is_none());
    }
}
