//! Easter eggs: little surprises drawn over the demos. The page decides when (a secret
//! key code, the calendar, the clock); a UFO also flies by on its own now and then.
//! Everything here is original: the saucer sprite, the texts and the effects.

use crate::audio::Audio;
use crate::font;
use crate::fx::{self, Palette, Rng};
use crate::gfx::*;
use std::f32::consts::TAU;

/// A UFO crosses the screen and stops to beam up the music.
pub const FLYBY: u32 = 1;
/// The famous cheat code: colours swap on every beat, "CHEAT MODE ON".
pub const CHEAT: u32 = 2;
/// The 1st of April: the screen turns upside down for a while.
pub const FLIP: u32 = 3;
/// New Year at midnight: fireworks on the beat. `arg` is the new year.
pub const FIREWORKS: u32 = 4;
/// Midnight on any other night: the compo is about to start.
pub const MIDNIGHT: u32 = 5;
/// Christmas Eve.
pub const XMAS: u32 = 6;
/// Midsummer Eve.
pub const MIDSUMMER: u32 = 7;
/// Greetings to the scene (from the browser console).
pub const GREETINGS: u32 = 8;

const FLYBY_LINES: [&str; 6] = [
    "TAKE ME TO YOUR SYSOP",
    "WE COME IN PEACE. AND FOR THE MUSIC",
    "GREETINGS FROM OUTER SPACE",
    "BEAM ME UP, SCENER",
    "THEY WATCH THE DEMOS TOO",
    "ABDUCTING THE BASS LINE ...",
];

/// How long each egg lasts, in seconds.
pub fn length(kind: u32) -> f32 {
    match kind {
        FLYBY => 12.0,
        CHEAT => 20.0,
        FLIP => 12.0,
        FIREWORKS => 60.0,
        _ => 10.0,
    }
}

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    c: u32,
}

pub struct Eggs {
    kind: u32,
    arg: u32,
    since: f32,
    line: usize,
    /// Snow falls while this is on (the page turns it on in December).
    pub snow: bool,
    flakes: Vec<(f32, f32, f32)>,
    sparks: Vec<Spark>,
    rng: Rng,
}

impl Default for Eggs {
    fn default() -> Self {
        Self::new()
    }
}

impl Eggs {
    pub fn new() -> Self {
        let mut rng = Rng::new(1985);
        let flakes = (0..140).map(|_| (rng.f() * W as f32, rng.f() * H as f32, 0.4 + rng.f() * 0.6)).collect();
        Eggs { kind: 0, arg: 0, since: 0.0, line: 0, snow: false, flakes, sparks: Vec::new(), rng }
    }

    /// Start an egg at time `t`; a new one replaces the one on screen.
    pub fn start(&mut self, kind: u32, arg: u32, t: f32) {
        self.kind = kind;
        self.arg = arg;
        self.since = t;
        self.line = self.rng.below(FLYBY_LINES.len() as u32) as usize;
        self.sparks.clear();
    }

    /// The egg on screen at `t`, if any.
    pub fn active(&self, t: f32) -> Option<u32> {
        (self.kind != 0 && t >= self.since && t - self.since < length(self.kind)).then_some(self.kind)
    }

    /// Draw whatever is going on. Called after the demo and its texts, before the QR code.
    pub fn draw(&mut self, fb: &mut Fb, t: f32, dt: f32, a: &Audio, pal: &Palette) {
        if self.snow {
            self.draw_snow(fb, dt, a);
        }
        let Some(kind) = self.active(t) else {
            self.kind = 0;
            return;
        };
        let e = t - self.since;
        match kind {
            FLYBY => self.flyby(fb, e, t, a, pal),
            CHEAT => cheat(fb, e, t, a, pal),
            FLIP => flip(fb, e, t, a, pal),
            FIREWORKS => self.fireworks(fb, e, t, dt, a, pal),
            MIDNIGHT => band(fb, e, length(kind), "00:00", "MIDNIGHT! THE DEMO COMPO STARTS NOW", t, a, pal),
            XMAS => band(fb, e, length(kind), "24 DECEMBER", "GOD JUL FRÅN UFOSCENE.NET!", t, a, pal),
            MIDSUMMER => band(fb, e, length(kind), "MIDSOMMARAFTON", "GLAD MIDSOMMAR!", t, a, pal),
            GREETINGS => band(fb, e, length(kind), "YOU FOUND THE CONSOLE", "GREETINGS TO EVERY SCENER, EVERYWHERE", t, a, pal),
            _ => {}
        }
    }

    fn draw_snow(&mut self, fb: &mut Fb, dt: f32, a: &Audio) {
        let wind = 6.0 + a.bass * 30.0;
        for (x, y, z) in self.flakes.iter_mut() {
            *y += dt * (12.0 + 26.0 * *z);
            *x += dt * (wind * *z + (*y * 0.05).sin() * 8.0);
            if *y >= H as f32 {
                *y -= H as f32 + 2.0;
            }
            *x = x.rem_euclid(W as f32);
            let c = rgb(255, 255, 255);
            let (px, py) = (*x as i32, *y as i32);
            if *z > 0.75 {
                fb.fill_rect(px, py, 2, 2, c);
            } else {
                fb.blend(px, py, c, 0.4 + *z * 0.6);
            }
        }
    }

    fn flyby(&mut self, fb: &mut Fb, e: f32, t: f32, a: &Audio, pal: &Palette) {
        // in from the left, hover in the middle from 4 s to 8 s, out to the right
        let len = length(FLYBY);
        let x = if e < 4.0 {
            -30.0 + smoothstep(e / 4.0) * (W as f32 / 2.0 + 30.0)
        } else if e < 8.0 {
            W as f32 / 2.0 + ((e - 4.0) * 3.0).sin() * 6.0
        } else {
            W as f32 / 2.0 + smoothstep((e - 8.0) / (len - 8.0)) * (W as f32 / 2.0 + 40.0)
        };
        let y = 34.0 + (t * 2.3).sin() * 6.0;
        if (4.3..7.8).contains(&e) {
            let open = smoothstep((e - 4.3) / 0.4).min(smoothstep((7.8 - e) / 0.4));
            let half = (10.0 + 34.0 * open) * (1.0 + a.bass * 0.3);
            let beam = pal.at(t * 0.2);
            fb.tri([(x - 6.0, y + 6.0), (x + 6.0, y + 6.0), (x + half, H as f32 - 30.0)], beam, 0.22 + a.pulse * 0.15);
            fb.tri([(x - 6.0, y + 6.0), (x - half, H as f32 - 30.0), (x + half, H as f32 - 30.0)], beam, 0.22 + a.pulse * 0.15);
            if open > 0.9 {
                let line = FLYBY_LINES[self.line];
                let sc = ((W as f32 - 24.0) / (line.chars().count() as f32 * 7.0)).clamp(1.0, 2.0);
                let w = line.chars().count() as f32 * 7.0 * sc;
                fx::dancing_text(fb, line, (W as f32 - w) / 2.0, H as f32 - 62.0, sc, t, a, a.beats, pal);
            }
        }
        saucer(fb, x as i32, y as i32, t, a, pal);
    }

    fn fireworks(&mut self, fb: &mut Fb, e: f32, t: f32, dt: f32, a: &Audio, pal: &Palette) {
        fb.darken(1.0 - 0.35 * smoothstep(e / 1.5).min(smoothstep((length(FIREWORKS) - e) / 1.5)));
        if (a.beat || self.sparks.is_empty()) && e < length(FIREWORKS) - 3.0 {
            let (cx, cy) = (40.0 + self.rng.f() * (W as f32 - 80.0), 30.0 + self.rng.f() * 90.0);
            let base = self.rng.f();
            for i in 0..48 {
                let ang = i as f32 / 48.0 * TAU + self.rng.f() * 0.2;
                let sp = 40.0 + self.rng.f() * 50.0;
                let c = if i % 3 == 0 { rgb(255, 255, 255) } else { pal.at(base + i as f32 * 0.004) };
                self.sparks.push(Spark { x: cx, y: cy, vx: ang.cos() * sp, vy: ang.sin() * sp, life: 1.4 + self.rng.f() * 0.6, c });
            }
        }
        for s in self.sparks.iter_mut() {
            s.vy += 45.0 * dt;
            s.vx *= 1.0 - dt * 0.8;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.life -= dt;
            if s.life > 0.0 {
                fb.blend(s.x as i32, s.y as i32, s.c, s.life.min(1.0));
                fb.blend(s.x as i32 + 1, s.y as i32, s.c, (s.life * 0.6).min(1.0));
            }
        }
        self.sparks.retain(|s| s.life > 0.0);
        let year = if self.arg > 0 { format!("GOTT NYTT {}!", self.arg) } else { "GOTT NYTT ÅR!".to_string() };
        let w = year.chars().count() as f32 * 7.0 * 3.0;
        fx::dancing_text(fb, &year, (W as f32 - w) / 2.0, H as f32 / 2.0 + 20.0, 3.0, t, a, a.beats, pal);
        let sub = "HAPPY NEW YEAR FROM UFOSCENE.NET";
        let sw = font::text_width(sub, 1, false);
        font::text(fb, sub, (WI - sw) / 2, HI / 2 + 50, 1, false, rgb(230, 230, 230));
    }
}

/// The saucer: a grey disc with a glass dome and lights that run round on the beat. The
/// secret show (fx/concert.rs) has it hover over the stage.
pub(crate) fn saucer(fb: &mut Fb, cx: i32, cy: i32, t: f32, a: &Audio, pal: &Palette) {
    // dome
    for dy in -7..0 {
        let half = ((49 - dy * dy) as f32).sqrt() as i32;
        let c = mix(rgb(120, 200, 255), rgb(230, 250, 255), (-dy) as f32 / 9.0);
        fb.hspan(cx - half, cx + half + 1, cy - 3 + dy, c);
    }
    // disc: rows wider in the middle, lighter on top
    let rows = [(-3, 12), (-2, 17), (-1, 20), (0, 21), (1, 20), (2, 16), (3, 10)];
    for (dy, half) in rows {
        let k = (3 - dy) as f32 / 6.0;
        fb.hspan(cx - half, cx + half + 1, cy + dy, mix(rgb(60, 64, 80), rgb(200, 204, 220), k));
    }
    fb.hspan(cx - 20, cx + 21, cy - 1, rgb(240, 240, 250));
    // lights
    let run = (t * 6.0) as i32 + a.beats as i32;
    for i in 0..7 {
        let x = cx - 15 + i * 5;
        let on = (i + run) % 3 == 0;
        let c = if on { pal.hi } else { rgb(40, 40, 50) };
        fb.fill_rect(x, cy + 1, 2, 2, c);
    }
}

/// Swap the colour channels round on every beat, with a band that says what happened.
fn cheat(fb: &mut Fb, e: f32, t: f32, a: &Audio, pal: &Palette) {
    let step = a.beats % 3;
    for p in fb.px.iter_mut() {
        let (r, g, b) = (*p & 0xFF, (*p >> 8) & 0xFF, (*p >> 16) & 0xFF);
        let (r, g, b) = match step {
            0 => (g, b, r),
            1 => (b, r, g),
            _ => (255 - r, 255 - g, 255 - b),
        };
        *p = 0xFF00_0000 | (b << 16) | (g << 8) | r;
    }
    band(fb, e, length(CHEAT), "CHEAT MODE ON", "INFINITE DEMOS - ALL 100 UNLOCKED", t, a, pal);
}

/// Upside down for most of the time, then back, then "APRIL APRIL!".
fn flip(fb: &mut Fb, e: f32, t: f32, a: &Audio, pal: &Palette) {
    if e < 8.0 {
        for y in 0..H / 2 {
            let (top, bottom) = fb.px.split_at_mut((H - 1 - y) * W);
            top[y * W..y * W + W].swap_with_slice(&mut bottom[..W]);
        }
    } else {
        band(fb, e - 8.0, 4.0, "1 APRIL", "APRIL, APRIL!", t, a, pal);
    }
}

/// A band across the middle: a small label and a line in dancing letters.
#[allow(clippy::too_many_arguments)]
fn band(fb: &mut Fb, e: f32, len: f32, label: &str, line: &str, t: f32, a: &Audio, pal: &Palette) {
    let k = smoothstep(e / 0.4).min(smoothstep((len - e) / 0.5));
    if k <= 0.0 {
        return;
    }
    let sc = ((W as f32 - 24.0) / (line.chars().count() as f32 * 7.0)).clamp(1.0, 3.0);
    let h = 16.0 + 7.0 * sc + 10.0;
    let cy = H as f32 * 0.62;
    let half = h / 2.0 * k;
    for y in (cy - half) as i32..(cy + half) as i32 {
        fb.hspan_blend(0, WI, y, rgb(0, 0, 0), 0.72);
    }
    fb.hspan(0, WI, (cy - half) as i32 - 1, pal.hi);
    fb.hspan(0, WI, (cy + half) as i32, pal.hi);
    if k < 0.9 {
        return;
    }
    let lw = font::text_width(label, 1, false);
    font::text(fb, label, (WI - lw) / 2, (cy - h / 2.0) as i32 + 4, 1, false, rgb(170, 170, 170));
    let w = line.chars().count() as f32 * 7.0 * sc;
    fx::dancing_text(fb, line, (W as f32 - w) / 2.0, cy - h / 2.0 + 15.0, sc, t, a, a.beats, pal);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_egg_draws_and_ends() {
        let mut eggs = Eggs::new();
        let a = Audio::new();
        let pal = fx::palette(3);
        for kind in FLYBY..=GREETINGS {
            let mut fb = Fb::new();
            fb.clear(rgb(10, 20, 30));
            let before = fb.px.clone();
            eggs.start(kind, 2027, 100.0);
            let mut t = 100.0;
            let mut changed = false;
            while t < 100.0 + length(kind) - 0.5 {
                fb.px.copy_from_slice(&before);
                eggs.draw(&mut fb, t, 1.0 / 30.0, &a, &pal);
                changed |= fb.px != before;
                t += 1.0 / 30.0;
            }
            assert!(changed, "egg {kind} should draw something");
            assert_eq!(eggs.active(100.0 + length(kind) + 0.1), None, "egg {kind} should end");
        }
    }

    #[test]
    fn flip_turns_the_screen_upside_down() {
        let mut fb = Fb::new();
        fb.put(3, 0, rgb(255, 0, 0));
        let mut eggs = Eggs::new();
        eggs.start(FLIP, 0, 0.0);
        eggs.draw(&mut fb, 1.0, 0.0, &Audio::new(), &fx::palette(0));
        assert_eq!(fb.get(3, HI - 1), rgb(255, 0, 0));
    }

    #[test]
    fn snow_falls() {
        let mut eggs = Eggs::new();
        eggs.snow = true;
        let mut fb = Fb::new();
        eggs.draw(&mut fb, 1.0, 0.03, &Audio::new(), &fx::palette(0));
        assert!(fb.px.iter().filter(|&&p| p & 0xFFFFFF != 0).count() > 50);
    }
}
