//! A pixel rock concert, half an hour long: a four-piece band on a stage under a lighting
//! truss, a big LED wall behind them, speaker stacks, and a crowd that jumps, claps and holds
//! up lighters to whatever is playing. Started from the page (C, `ufo.concert()`, or
//! `#concert` in the address), it plays for [`SECONDS`] and then the demos carry on.
//!
//! The show runs in acts: the doors open in the dark, then a clap-along, a light show,
//! pyro, a disco with a mirror ball, a stadium wave, a guitar solo, a piano ballad and a
//! finale with fireworks and confetti, then the band takes a bow. With song names coming
//! from Spotify (or Last.fm, or the radio) every new song starts the next act; without
//! them the acts change every few minutes. The artist's photo, when there is one, shows on
//! the LED wall. The band itself is made up: any music can play this stage.
//!
//! Like the Christmas scene it is not in `all()`, so the 100 demos stay as they are.

use std::f32::consts::TAU;

use super::*;
use crate::font;

/// How long the concert lasts, in seconds.
pub const SECONDS: f32 = 30.0 * 60.0;
/// The dark minute before the lights go up (here: seconds).
const DOORS: f32 = 8.0;
/// The band takes its bow in the last seconds.
const GOODBYE: f32 = 18.0;
/// Without song names, each act lasts this long.
const SLOT: f32 = 224.0;
/// A song change in the opening seconds is the first song arriving, not a new one.
const FIRST_SONG: f32 = 25.0;
/// The finale is held back for the end of the show.
const FINALE_FROM: f32 = SECONDS - 150.0;

pub fn def() -> Def {
    Def { name: "LIVE CONCERT", flags: VECTOR | OWN_TEXT, make: |s| Box::new(Concert::new(s)) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Clap,
    Lights,
    Pyro,
    Disco,
    Stadium,
    Solo,
    Ballad,
    Finale,
}
const ACTS: [Act; 8] = [Act::Clap, Act::Lights, Act::Pyro, Act::Disco, Act::Stadium, Act::Solo, Act::Ballad, Act::Finale];
const FINALE: usize = 7;

// The stage, top to bottom.
const TRUSS_Y: i32 = 3;
const CANS: usize = 8;
const SCREEN: (i32, i32, i32, i32) = (120, 13, 144, 58); // x, y, w, h
const FLOOR_TOP: i32 = 118;
const FLOOR_EDGE: i32 = 141;
const CROWD_TOP: i32 = 147;
const RISER: (i32, i32, i32, i32) = (148, 110, 88, 9);
const FLAMES: [f32; 4] = [56.0, 100.0, 284.0, 328.0];

/// What a band member looks like. `style`: 0 short hair, 1 big curls, 2 long, 3 spiky.
#[derive(Clone, Copy)]
struct Look {
    skin: u32,
    hair: u32,
    style: u8,
    top: u32,
    legs: u32,
}

const SINGER: Look = Look { skin: 0xFF8C_ACE0, hair: 0xFF10_1420, style: 0, top: 0xFFF0_F5F5, legs: 0xFF28_1EC8 };
const GUITARIST: Look = Look { skin: 0xFF9C_BAE6, hair: 0xFF16_1C28, style: 1, top: 0xFF28_1E1E, legs: 0xFF23_1919 };
const BASSIST: Look = Look { skin: 0xFF96_B4E2, hair: 0xFF32_5478, style: 2, top: 0xFFC8_6E3C, legs: 0xFF3C_2828 };
const DRUMMER: Look = Look { skin: 0xFFA0_BEEC, hair: 0xFF6E_C8E6, style: 3, top: 0xFF3C_3CDC, legs: 0xFF28_2828 };

/// Where the left and right hand are.
type Hands = ((f32, f32), (f32, f32));

/// One person in the crowd.
struct Fan {
    x: f32,
    row: usize,
    ph: f32,
    jump: f32,
    /// Holds up a phone or a lighter in the quiet parts.
    light: bool,
}

/// kind 0: spark (falls, fades), 1: confetti (flutters down), 2: ember (rises).
struct Bit {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    col: u32,
    kind: u8,
}

pub struct Concert {
    act: usize,
    act_t0: f32,
    song: Option<u32>,
    /// Song names arrive, so they set the pace instead of the clock.
    by_song: bool,
    fans: Vec<Fan>,
    bits: Vec<Bit>,
    flames: [f32; 4],
    /// Mirror ball reflections: x phase, y, colour phase.
    dots: Vec<(f32, f32, f32)>,
    rng: Rng,
}

impl Concert {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed ^ 0x51AE);
        let mut fans = Vec::new();
        for row in 0..4 {
            let gap = 11.0 + row as f32 * 4.0;
            let mut x = -4.0 + rng.f() * gap;
            while x < W as f32 + 8.0 {
                fans.push(Fan { x, row, ph: rng.f() * TAU, jump: 0.4 + rng.f() * 0.6, light: rng.below(3) == 0 });
                x += gap * (0.8 + rng.f() * 0.4);
            }
        }
        let dots = (0..70).map(|_| (rng.f(), 6.0 + rng.f() * 200.0, rng.f())).collect();
        Concert { act: 0, act_t0: 0.0, song: None, by_song: false, fans, bits: Vec::new(), flames: [0.0; 4], dots, rng }
    }

    fn next_act(&mut self, t: f32) {
        // the finale waits for the end of the show; before that the acts go round again
        self.act = if self.act + 1 >= FINALE && t < SECONDS - 600.0 { 1 } else { (self.act + 1).min(FINALE) };
        self.act_t0 = t;
    }

    /// Which act is on at `t`, moving on with the songs (or the clock).
    fn follow(&mut self, t: f32, song: u32) {
        match self.song {
            None => self.song = Some(song),
            Some(s) if s != song => {
                self.song = Some(song);
                if t > FIRST_SONG {
                    self.by_song = true;
                    self.next_act(t);
                }
            }
            _ => {}
        }
        let age = t - self.act_t0;
        // songs set the pace; a song that never seems to end still gives way after 7 minutes
        if (!self.by_song && age > SLOT) || age > 420.0 {
            self.next_act(t);
        }
        if t > FINALE_FROM && self.act != FINALE {
            self.act = FINALE;
            self.act_t0 = t;
        }
    }
}

// ---------------------------------------------------------------- drawing helpers

#[inline]
fn add_px(fb: &mut Fb, x: i32, y: i32, c: u32, k: f32) {
    if x < 0 || y < 0 || x >= WI || y >= HI || k <= 0.0 {
        return;
    }
    let i = y as usize * W + x as usize;
    let p = fb.px[i];
    let ch = |s: u32| {
        let v = ((p >> s) & 0xFF) as f32 + ((c >> s) & 0xFF) as f32 * k;
        (v.min(255.0) as u32) << s
    };
    fb.px[i] = 0xFF00_0000 | ch(0) | ch(8) | ch(16);
}

fn thick(fb: &mut Fb, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, w: i32) {
    for dx in 0..w {
        for dy in 0..w {
            line(fb, x0 + dx as f32, y0 + dy as f32, x1 + dx as f32, y1 + dy as f32, c);
        }
    }
}

fn ellipse(fb: &mut Fb, cx: f32, cy: f32, rx: f32, ry: f32, c: u32) {
    for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
        let dy = (y as f32 + 0.5 - cy) / ry;
        if dy.abs() >= 1.0 {
            continue;
        }
        let hw = rx * (1.0 - dy * dy).sqrt();
        fb.hspan((cx - hw).round() as i32, (cx + hw).round() as i32, y, c);
    }
}

/// A soft additive glow.
fn glow(fb: &mut Fb, cx: f32, cy: f32, r: f32, c: u32, k: f32) {
    let ri = r.ceil() as i32;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let d = ((dx * dx + dy * dy) as f32).sqrt() / r;
            if d < 1.0 {
                add_px(fb, cx as i32 + dx, cy as i32 + dy, c, k * (1.0 - d) * (1.0 - d));
            }
        }
    }
}

/// A light beam in the haze, from a lamp at (x0, y0) to a pool of light at (x1, y1).
#[allow(clippy::too_many_arguments)]
fn beam(fb: &mut Fb, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: u32, k: f32) {
    if k <= 0.01 || y1 <= y0 {
        return;
    }
    for y in (y0.max(0.0) as i32)..=(y1.min(H as f32 - 1.0) as i32) {
        let s = (y as f32 - y0) / (y1 - y0);
        let cx = x0 + (x1 - x0) * s;
        let hw = 1.0 + w * s;
        let fade = k * (0.55 - 0.25 * s);
        for x in ((cx - hw).floor() as i32).max(0)..=((cx + hw).ceil() as i32).min(WI - 1) {
            let d = 1.0 - (x as f32 - cx).abs() / hw;
            if d > 0.0 {
                add_px(fb, x, y, c, fade * d * d);
            }
        }
    }
    // the pool where it lands
    for dy in -3..=3 {
        let hw = w * (1.0 - (dy as f32 / 3.5).powi(2)).sqrt();
        for x in (x1 - hw) as i32..=(x1 + hw) as i32 {
            add_px(fb, x, y1 as i32 + dy, c, k * 0.35 * (1.0 - (x as f32 - x1).abs() / hw.max(1.0)));
        }
    }
}

// ---------------------------------------------------------------- the band

#[derive(Clone, Copy)]
struct Pose {
    bob: f32,
    lean: f32,
    spread: f32,
    sit: bool,
}

/// Legs, shirt and head of a band member with feet at (x, feet). Returns the shoulders, so
/// an instrument can go on before the arms (`arms`).
fn body(fb: &mut Fb, x: f32, feet: f32, lk: &Look, p: Pose, k: f32) -> ((f32, f32), (f32, f32)) {
    let hip = feet - 14.0 + p.bob;
    let legs = scale(lk.legs, k);
    if p.sit {
        // on a stool, facing left (at the piano)
        fb.fill_rect(x as i32 - 3, hip as i32 + 2, 7, 2, scale(rgb(40, 30, 30), k));
        fb.fill_rect(x as i32, hip as i32 + 4, 1, (feet - hip - 4.0) as i32, scale(rgb(40, 30, 30), k));
        thick(fb, x - 1.0, hip, x - 8.0, hip, legs, 3);
        thick(fb, x - 8.0, hip, x - 8.0, feet - 1.0, legs, 3);
        fb.fill_rect(x as i32 - 11, feet as i32 - 1, 4, 2, scale(rgb(20, 16, 16), k));
    } else {
        thick(fb, x - 2.0, hip, x - p.spread - 1.0, feet - 1.0, legs, 3);
        thick(fb, x + 1.0, hip, x + p.spread, feet - 1.0, legs, 3);
        let shoe = scale(rgb(20, 16, 16), k);
        fb.fill_rect((x - p.spread - 3.0) as i32, feet as i32 - 1, 4, 2, shoe);
        fb.fill_rect((x + p.spread - 1.0) as i32, feet as i32 - 1, 4, 2, shoe);
    }
    let top = hip - 12.0;
    let shirt = scale(lk.top, k);
    for r in 0..=12 {
        let s = r as f32 / 12.0; // 0 at the shoulders
        let cx = x + p.lean * (1.0 - s);
        let half = 4.5 - s;
        fb.hspan((cx - half).round() as i32, (cx + half).round() as i32 + 1, (top + r as f32) as i32, shirt);
    }
    let sx = x + p.lean;
    head(fb, sx + p.lean * 0.3, top - 1.0, lk, k);
    ((sx - 4.0, top + 1.5), (sx + 4.0, top + 1.5))
}

fn head(fb: &mut Fb, cx: f32, neck: f32, lk: &Look, k: f32) {
    let skin = scale(lk.skin, k);
    let hair = scale(lk.hair, k);
    let (x0, y0) = ((cx - 2.5).round() as i32, (neck - 7.0).round() as i32);
    match lk.style {
        1 => ellipse(fb, cx + 0.5, neck - 4.5, 6.5, 6.0, hair),
        2 => fb.fill_rect(x0 - 1, y0, 8, 10, hair),
        _ => {}
    }
    fb.fill_rect(x0 + 2, y0 + 6, 2, 2, skin);
    fb.fill_rect(x0, y0 + 1, 6, 5, skin);
    fb.fill_rect(x0 + 1, y0 + 6, 4, 1, skin);
    match lk.style {
        0 => {
            fb.fill_rect(x0, y0, 6, 2, hair);
            fb.put(x0, y0 + 2, hair);
            fb.put(x0 + 5, y0 + 2, hair);
        }
        1 => fb.fill_rect(x0 - 1, y0 - 1, 8, 3, hair),
        2 => fb.fill_rect(x0, y0, 6, 2, hair),
        _ => {
            fb.fill_rect(x0, y0, 6, 2, hair);
            for i in 0..3 {
                fb.put(x0 + i * 2, y0 - 1, hair);
            }
        }
    }
    let eye = scale(rgb(30, 20, 20), k);
    fb.put(x0 + 1, y0 + 3, eye);
    fb.put(x0 + 4, y0 + 3, eye);
}

/// Both arms, from the shoulders to the hands.
fn arms(fb: &mut Fb, sh: ((f32, f32), (f32, f32)), hands: ((f32, f32), (f32, f32)), lk: &Look, k: f32) {
    let skin = scale(lk.skin, k);
    for (s, h) in [(sh.0, hands.0), (sh.1, hands.1)] {
        thick(fb, s.0, s.1, h.0, h.1, skin, 2);
        fb.fill_rect(h.0 as i32, h.1 as i32, 2, 2, skin);
    }
}

/// A guitar (or with `long` a bass) at the hip. Returns where the strumming and the
/// fretting hand go.
fn guitar(fb: &mut Fb, x: f32, hip: f32, col: u32, long: bool, tilt: f32, k: f32) -> ((f32, f32), (f32, f32)) {
    let (bx, by) = (x + 2.0, hip - 1.0);
    let len = if long { 20.0 } else { 16.0 };
    let (ex, ey) = (bx - len, by - len * 0.45 - tilt * 6.0);
    thick(fb, bx, by, ex, ey, scale(rgb(110, 70, 40), k), 2);
    fb.fill_rect(ex as i32 - 2, ey as i32 - 1, 3, 3, scale(rgb(60, 40, 30), k));
    ellipse(fb, bx, by + 0.5, 5.5, 4.0, scale(col, k));
    ellipse(fb, bx + 2.0, by + 1.5, 3.5, 3.0, scale(col, k * 0.8));
    fb.fill_rect(bx as i32 - 1, by as i32 - 1, 3, 2, scale(rgb(20, 20, 20), k));
    ((bx + 1.0, by - 1.0), (bx + (ex - bx) * 0.6, by + (ey - by) * 0.6))
}

// ---------------------------------------------------------------- the scene

impl Concert {
    fn colour(&self, act: Option<Act>, pal: &Palette, t: f32, i: usize) -> u32 {
        match act {
            None => rgb(70, 90, 220),
            Some(Act::Clap) => if i.is_multiple_of(2) { rgb(255, 40, 40) } else { rgb(255, 235, 220) },
            Some(Act::Lights) => mix(pal.at(i as f32 * 0.12 + t * 0.05), rgb(255, 255, 255), 0.2),
            Some(Act::Pyro) => mix(rgb(255, 110, 20), rgb(255, 220, 90), (i % 3) as f32 / 2.0),
            Some(Act::Disco) => [rgb(255, 40, 200), rgb(40, 220, 255), rgb(160, 60, 255), rgb(255, 255, 255)][i % 4],
            Some(Act::Stadium) => rgb(255, 205, 130),
            Some(Act::Solo) => rgb(170, 100, 255),
            Some(Act::Ballad) => rgb(255, 190, 110),
            Some(Act::Finale) => rainbow(i as f32 / 8.0 + t * 0.3),
        }
    }

    fn wash(&self, act: Option<Act>, pal: &Palette, t: f32) -> u32 {
        match act {
            None | Some(Act::Ballad) => rgb(40, 60, 170),
            Some(Act::Clap) => rgb(200, 30, 40),
            Some(Act::Lights) => pal.at(t * 0.03),
            Some(Act::Pyro) => rgb(230, 90, 20),
            Some(Act::Disco) => rgb(150, 40, 200),
            Some(Act::Stadium) => rgb(230, 160, 70),
            Some(Act::Solo) => rgb(90, 40, 170),
            Some(Act::Finale) => rainbow(t * 0.1),
        }
    }

    fn spawn(&mut self, x: f32, y: f32, n: usize, speed: f32, col: u32, kind: u8) {
        if self.bits.len() > 900 {
            return;
        }
        for _ in 0..n {
            let ang = self.rng.f() * TAU;
            let v = speed * (0.3 + self.rng.f() * 0.7);
            let life = 0.6 + self.rng.f() * 0.8;
            self.bits.push(Bit { x, y, vx: ang.cos() * v, vy: ang.sin() * v, age: 0.0, life, col, kind });
        }
    }

    fn crowd(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, wash: u32, goodbye: bool) {
        let (t, a) = (c.t, c.a);
        // haze lit by the stage behind the crowd, so the people stand out against it
        let haze = mix(scale(wash, 0.55 + 0.2 * a.pulse), rgb(90, 80, 100), 0.25);
        for y in CROWD_TOP..HI {
            let k = (y - CROWD_TOP) as f32 / (HI - CROWD_TOP) as f32;
            fb.row(y, mix(haze, rgb(6, 5, 10), k.sqrt()));
        }
        let clap = c.beats % 3 == 2 && c.phase < 0.5;
        let sway = (t * TAU / (c.a.period * 4.0)).sin();
        for f in &self.fans {
            let r = 3.0 + f.row as f32;
            let base = 160.0 + f.row as f32 * 15.0;
            let jumpy = !matches!(act, None | Some(Act::Ballad));
            let bounce = if jumpy { a.pulse * (1.5 + f.row as f32) * f.jump * 1.6 } else { (t * 1.3 + f.ph).sin() * 0.6 };
            let hy = base - bounce;
            let shade = rgb(5, 4, 9);
            let rim = mix(mix(shade, wash, 0.7 + a.pulse * 0.3), rgb(255, 255, 255), 0.15);
            // shoulders, then the head with stage light on top
            ellipse(fb, f.x, hy + r * 2.2, r * 2.0, r * 1.4, shade);
            fb.fill_rect((f.x - r * 2.0) as i32, (hy + r * 2.2) as i32, (r * 4.0) as i32 + 1, 18, shade);
            ellipse(fb, f.x, hy, r, r, shade);
            fb.hspan((f.x - r * 0.6) as i32, (f.x + r * 0.6) as i32 + 1, (hy - r + 0.5) as i32, rim);
            fb.put((f.x - r * 0.85) as i32, (hy - r * 0.5) as i32, rim);
            fb.put((f.x + r * 0.85) as i32, (hy - r * 0.5) as i32, rim);
            let sh = (hy + r * 1.4, f.x - r * 1.4, f.x + r * 1.4);
            let reach = r * 2.6 + 4.0;
            let up = |dx: f32| (sh.1 + dx, sh.0 - reach);
            let (hands, light): (Option<Hands>, bool) = match act {
                _ if goodbye => (Some(((sh.1 - 3.0 + sway * 3.0, sh.0 - reach), (sh.2 + 3.0 + sway * 3.0, sh.0 - reach))), false),
                None => (f.light.then_some(((sh.1, sh.0 + 2.0), (sh.2 + 1.0, sh.0 - reach + 2.0))), f.light),
                Some(Act::Ballad) => {
                    let s = (t * 1.1 + f.ph * 0.3).sin() * 4.0;
                    (f.light.then_some(((sh.1, sh.0 + 2.0), (sh.2 + s, sh.0 - reach))), f.light)
                }
                Some(Act::Clap) => {
                    let pos = if clap { ((f.x - 1.0, sh.0 - reach - 1.0), (f.x + 1.0, sh.0 - reach - 1.0)) } else { (up(-3.0), (sh.2 + 3.0, sh.0 - reach + 3.0)) };
                    (Some(pos), false)
                }
                Some(Act::Stadium) | Some(Act::Finale) => {
                    // most wave their arms in time, the rest jump on the beat
                    let s = sway * 5.0;
                    let waving = f.jump > 0.55 || a.pulse * f.jump > 0.3;
                    (waving.then_some(((sh.1 - 2.0 + s, sh.0 - reach), (sh.2 + 2.0 + s, sh.0 - reach))), false)
                }
                _ => {
                    let raised = a.pulse * f.jump > 0.35 || (f.jump > 0.85 && f.row % 2 == 0);
                    (raised.then_some(((sh.1 - 2.0, sh.0 - reach + 2.0), (sh.2 + 2.0, sh.0 - reach))), false)
                }
            };
            if let Some((l, rr)) = hands {
                let w = if f.row < 2 { 1 } else { 2 };
                thick(fb, sh.1, sh.0, l.0, l.1, shade, w);
                thick(fb, sh.2, sh.0, rr.0, rr.1, shade, w);
                fb.put(l.0 as i32, l.1 as i32, rim);
                fb.put(rr.0 as i32, rr.1 as i32, rim);
                if light {
                    if act.is_none() {
                        // a phone, filming
                        fb.fill_rect(rr.0 as i32, rr.1 as i32 - 3, 2, 3, rgb(210, 225, 255));
                        glow(fb, rr.0 + 1.0, rr.1 - 2.0, 4.0, rgb(150, 170, 255), 0.5);
                    } else {
                        let fl = 0.7 + 0.3 * (t * 17.0 + f.ph * 5.0).sin();
                        fb.fill_rect(rr.0 as i32, rr.1 as i32 - 2, 1, 2, rgb(255, 220, 120));
                        glow(fb, rr.0, rr.1 - 2.0, 4.0, rgb(255, 160, 60), 0.6 * fl);
                    }
                }
            }
        }
        // stomp, stomp, clap: the clap lights up the crowd
        if act == Some(Act::Clap) && clap {
            for y in CROWD_TOP..HI {
                fb.hspan_blend(0, WI, y, rgb(255, 255, 255), 0.06 * (1.0 - c.phase * 2.0));
            }
        }
    }

    fn drums(&self, fb: &mut Fb, c: &Ctx, k: f32) {
        let (cx, base) = (192.0, RISER.1 as f32);
        let a = c.a;
        // the drummer sits behind the kit
        let bob = -a.pulse * 1.5;
        let sh = body(fb, cx, base - 10.0 + bob, &DRUMMER, Pose { bob: 0.0, lean: 0.0, spread: 3.0, sit: false }, k);
        // cymbals and their stands
        let stand = scale(rgb(120, 120, 130), k);
        let shine = scale(rgb(230, 200, 90), (k + a.pulse * 0.5).min(1.0));
        for (x, y, d) in [(cx - 21.0, base - 30.0, 1.0), (cx + 22.0, base - 33.0, -1.0)] {
            line(fb, x, y, x, base, stand);
            let tilt = a.pulse * 2.0 * d;
            thick(fb, x - 7.0, y + tilt, x + 7.0, y - tilt, shine, 1);
            fb.hspan((x - 6.0) as i32, (x + 7.0) as i32, (y + 1.0) as i32, shine);
        }
        // toms and the bass drum, with the band's name on it
        let shell = scale(rgb(180, 30, 40), k);
        ellipse(fb, cx - 8.0, base - 22.0, 5.5, 3.5, shell);
        ellipse(fb, cx + 8.0, base - 22.0, 5.5, 3.5, shell);
        ellipse(fb, cx - 8.0, base - 24.0, 5.0, 1.5, scale(rgb(230, 230, 220), k));
        ellipse(fb, cx + 8.0, base - 24.0, 5.0, 1.5, scale(rgb(230, 230, 220), k));
        ellipse(fb, cx, base - 11.0, 12.0, 11.0, shell);
        ellipse(fb, cx, base - 11.0, 10.0, 9.5, scale(rgb(235, 235, 228), k));
        font::text(fb, "UFO", cx as i32 - 9, base as i32 - 15, 1, true, scale(rgb(180, 30, 40), k));
        // sticks: one hand on the beat, the other on the off-beat
        let hit = |on: bool| if on { 1.0 - c.phase } else { 0.0 };
        let even = c.beats.is_multiple_of(2);
        let hands = [
            (sh.0, (cx - 9.0, base - 27.0 + 4.0 * hit(even)), (cx - 15.0, base - 29.0 + 7.0 * hit(even))),
            (sh.1, (cx + 9.0, base - 27.0 + 4.0 * hit(!even)), (cx + 17.0, base - 33.0 + 7.0 * hit(!even))),
        ];
        let skin = scale(DRUMMER.skin, k);
        for (s, h, tip) in hands {
            thick(fb, s.0, s.1, h.0, h.1, skin, 2);
            line(fb, h.0, h.1, tip.0, tip.1, scale(rgb(230, 200, 150), k));
        }
    }

    fn piano(&self, fb: &mut Fb, k: f32) {
        let black = scale(rgb(14, 12, 16), k.max(0.5));
        let edge = scale(rgb(90, 80, 100), k);
        fb.tri([(138.0, 120.0), (178.0, 120.0), (144.0, 104.0)], black, 1.0);
        line(fb, 138.0, 120.0, 144.0, 104.0, edge);
        line(fb, 144.0, 104.0, 178.0, 120.0, edge);
        fb.fill_rect(136, 120, 46, 8, black);
        fb.hspan(136, 182, 120, edge);
        for x in [140, 176] {
            fb.fill_rect(x, 128, 2, 9, black);
        }
        fb.fill_rect(176, 119, 9, 3, scale(rgb(235, 235, 225), k));
        for x in (177..185).step_by(2) {
            fb.put(x, 119, black);
        }
    }

    fn led_wall(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, wash: u32, goodbye: bool) {
        let (sx, sy, sw, sh) = SCREEN;
        fb.fill_rect(sx - 2, sy - 2, sw + 4, sh + 4, rgb(28, 28, 34));
        let lit = 0.75 + c.a.pulse * 0.25;
        match c.image {
            Some(img) if act.is_some() && !goodbye => {
                for y in 0..sh {
                    let row = if y % 2 == 1 { lit * 0.7 } else { lit };
                    let iy = (74 + y * 256 / sw) as usize;
                    for x in 0..sw {
                        let ix = (x * 256 / sw) as usize;
                        fb.px[(sy + y) as usize * W + (sx + x) as usize] = scale(img.px[iy * 256 + ix], row);
                    }
                }
            }
            _ => {
                // a spectrum on the wall, the stage colours behind it
                fb.fill_rect(sx, sy, sw, sh, scale(wash, 0.12));
                let bars = 24;
                for b in 0..bars {
                    let lo = 2 + b * 7;
                    let v = c.a.bins[lo..lo + 7].iter().map(|&v| v as f32).sum::<f32>() / (7.0 * 255.0);
                    let v = if c.a.autopilot { 0.3 + 0.5 * c.a.pulse * (0.6 + 0.4 * ((b as f32 * 0.7 + c.t * 3.0).sin())) } else { v };
                    let h = (v * sh as f32 * 0.9) as i32;
                    let col = self.colour(act, c.pal, c.t, b);
                    fb.fill_rect(sx + 2 + b as i32 * 6, sy + sh - h, 4, h, scale(col, 0.85));
                }
                for y in (sy..sy + sh).step_by(2) {
                    fb.hspan_blend(sx, sx + sw, y, rgb(0, 0, 0), 0.35);
                }
            }
        }
        if act.is_none() && !goodbye {
            dancing_text(fb, "LIVE", (sx + (sw - 82) / 2) as f32, sy as f32 + 6.0, 3.0, c.t, c.a, c.beats, c.pal);
            let w = font::text_width("TONIGHT", 2, true);
            font::text(fb, "TONIGHT", sx + (sw - w) / 2, sy + 38, 2, true, rgb(255, 255, 255));
        }
    }
}

impl Effect for Concert {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a, dt) = (c.t, c.a, c.dt);
        self.follow(t, c.song);
        let act = (t >= DOORS).then(|| ACTS[self.act]);
        let age = t - self.act_t0;
        let goodbye = t > SECONDS - GOODBYE;
        let wash = self.wash(act, c.pal, t);
        let loud = !matches!(act, None | Some(Act::Ballad | Act::Solo)) && !goodbye;

        // back wall and truss
        for y in 0..FLOOR_TOP {
            let k = y as f32 / FLOOR_TOP as f32;
            fb.row(y, mix(rgb(4, 3, 10), scale(wash, 0.2 + 0.12 * a.bass * loud as u8 as f32), k * k));
        }
        let metal = rgb(70, 70, 80);
        fb.hspan(0, WI, TRUSS_Y, metal);
        fb.hspan(0, WI, TRUSS_Y + 5, metal);
        for x in (0..WI).step_by(6) {
            line(fb, x as f32, TRUSS_Y as f32, x as f32 + 3.0, TRUSS_Y as f32 + 5.0, rgb(50, 50, 58));
        }
        let cans: [f32; CANS] = std::array::from_fn(|i| 24.0 + i as f32 * 48.0);
        for (i, &x) in cans.iter().enumerate() {
            fb.fill_rect(x as i32 - 3, TRUSS_Y + 6, 6, 5, rgb(36, 36, 44));
            let on = self.colour(act, c.pal, t, i);
            fb.fill_rect(x as i32 - 2, TRUSS_Y + 10, 4, 2, scale(on, 0.6 + a.pulse * 0.4));
        }

        // speaker stacks, cones pumping with the bass
        for x0 in [4, 344] {
            for b in 0..3 {
                let y0 = 60 + b * 19;
                fb.fill_rect(x0, y0, 36, 18, rgb(20, 20, 24));
                fb.fill_rect(x0 + 1, y0 + 1, 34, 1, rgb(46, 46, 52));
                for cx in [x0 + 10, x0 + 26] {
                    let r = 5.0 + a.bass * 1.6;
                    ellipse(fb, cx as f32, y0 as f32 + 9.0, r, r, rgb(40, 40, 46));
                    ellipse(fb, cx as f32, y0 as f32 + 9.0, r * 0.45, r * 0.45, rgb(70, 70, 80));
                }
            }
        }
        self.led_wall(fb, c, act, wash, goodbye);

        // stage floor and its front edge, with footlights
        for y in FLOOR_TOP..FLOOR_EDGE {
            let plank = if (y - FLOOR_TOP) % 5 == 0 { 30 } else { 22 };
            fb.row(y, mix(rgb(plank, plank - 2, plank + 4), scale(wash, 0.3), 0.15));
        }
        fb.fill_rect(0, FLOOR_EDGE, WI, CROWD_TOP - FLOOR_EDGE, rgb(10, 10, 12));
        for x in (8..WI).step_by(16) {
            fb.fill_rect(x, FLOOR_EDGE + 1, 3, 2, scale(wash, 0.6 + a.pulse * 0.4));
        }
        fb.fill_rect(RISER.0, RISER.1, RISER.2, RISER.3, rgb(30, 28, 34));
        for x in (RISER.0 + 4..RISER.0 + RISER.2).step_by(8) {
            fb.fill_rect(x, RISER.1 + 4, 3, 2, scale(wash, 0.5 + a.pulse * 0.5));
        }

        // where the band is
        let solo = act == Some(Act::Solo);
        let ballad = act == Some(Act::Ballad) && !goodbye;
        let s_in = smoothstep(age / 2.0);
        let gx = if solo { lerp(118.0, 192.0, s_in) } else { 118.0 };
        let g_feet = if solo { lerp(133.0, 139.0, s_in) } else { 133.0 };
        let fx = if ballad {
            196.0
        } else if solo {
            lerp(192.0, 300.0, s_in)
        } else {
            192.0 + 64.0 * (t * 0.23).sin()
        };

        // the lights
        for (i, &x) in cans.iter().enumerate() {
            let col = self.colour(act, c.pal, t, i);
            let fi = i as f32;
            let flash = 0.45 + 0.55 * a.pulse;
            let (tx, ty, w, k): (f32, f32, f32, f32) = match act {
                _ if goodbye => (x + (192.0 - x) * 0.3, 136.0, 16.0, 0.6),
                None => {
                    if i == 1 || i == 6 {
                        (192.0 + 170.0 * (t * 0.5 + fi).sin(), 205.0, 22.0, 0.35)
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                }
                Some(Act::Clap) => {
                    let side = if c.beats.is_multiple_of(2) ^ i.is_multiple_of(2) { 60.0 } else { -60.0 };
                    (x + side, 134.0, 14.0, (if c.beats % 3 == 2 { 1.0 } else { 0.5 }) * flash)
                }
                Some(Act::Lights) => (192.0 + 150.0 * (t * 0.7 + fi * 0.8).sin(), 136.0, 14.0, flash),
                Some(Act::Pyro) => (x + 20.0 * (t * 0.6 + fi).sin(), 136.0, 18.0, 0.4 + 0.4 * a.pulse),
                Some(Act::Disco) => (192.0 + 180.0 * (t * 1.6 + fi * 1.3).sin(), 130.0 + 6.0 * (t * 2.0 + fi).sin(), 10.0, flash),
                Some(Act::Stadium) => (x + (x - 192.0) * 0.7 * (t * 0.5).sin(), 136.0, 16.0, 0.4 + 0.4 * a.pulse),
                Some(Act::Solo) => {
                    if i == 3 || i == 4 {
                        (gx, g_feet, 12.0, 0.8)
                    } else {
                        (x, 134.0, 10.0, 0.15 + 0.2 * a.pulse)
                    }
                }
                Some(Act::Ballad) => {
                    if i == 4 {
                        (fx - 10.0, 132.0, 16.0, 0.7)
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                }
                Some(Act::Finale) => (192.0 + 170.0 * (t * 1.1 + fi * 0.9).sin(), 136.0, 14.0, flash),
            };
            beam(fb, x, TRUSS_Y as f32 + 12.0, tx, ty, w, col, k);
        }
        if act == Some(Act::Disco) {
            let (bx, by) = (192.0, 22.0);
            line(fb, bx, TRUSS_Y as f32 + 5.0, bx, by - 7.0, rgb(90, 90, 100));
            for dy in -7..=7 {
                for dx in -7..=7 {
                    if dx * dx + dy * dy <= 49 {
                        let facet = ((dx + 8 + (t * 8.0) as i32) / 2 + (dy + 8) / 2) % 3;
                        fb.put(bx as i32 + dx, by as i32 + dy, [rgb(200, 200, 215), rgb(120, 120, 140), rgb(250, 250, 255)][facet as usize]);
                    }
                }
            }
            for &(px, py, ph) in &self.dots {
                let x = fract(px + t * 0.06) * W as f32;
                let col = mix(rainbow(ph + t * 0.2), rgb(255, 255, 255), 0.4);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    add_px(fb, x as i32 + dx, py as i32 + dy, col, 0.7);
                }
            }
        }

        // the band; dimmer in the dark and the quiet parts
        let k = if act.is_none() { 0.3 } else if ballad || solo { 0.45 } else { 0.78 + 0.22 * a.pulse };
        self.drums(fb, c, k);
        let bang = if loud { (c.phase * TAU).sin() * 1.8 } else { 0.0 };
        // guitarist
        {
            let kk = if solo { 1.0 } else { k };
            let p = Pose { bob: if loud || solo { -a.pulse * 1.5 } else { 0.0 }, lean: if goodbye { 3.0 } else { bang }, spread: if solo { 5.0 } else { 3.0 }, sit: false };
            let sh = body(fb, gx, g_feet, &GUITARIST, p, kk);
            let hip = g_feet - 14.0 + p.bob;
            let tilt = if solo { 0.8 + 0.3 * (t * 3.0).sin() } else { 0.0 };
            let (strum, fret) = guitar(fb, gx, hip, rgb(200, 30, 40), false, tilt, kk);
            let s = (t * TAU / (a.period * 0.5)).sin() * 2.0;
            arms(fb, sh, (fret, (strum.0, strum.1 + s)), &GUITARIST, kk);
            if solo && a.beat {
                self.spawn(strum.0, strum.1, 18, 60.0, rgb(255, 200, 80), 0);
            }
        }
        // bassist
        {
            let p = Pose { bob: 0.0, lean: if goodbye { 3.0 } else { (t * 2.0).sin() * 1.2 }, spread: 3.0, sit: false };
            let sh = body(fb, 266.0, 133.0, &BASSIST, p, k);
            let (strum, fret) = guitar(fb, 266.0, 119.0, rgb(30, 90, 200), true, 0.0, k);
            let s = (t * TAU / a.period).sin() * 1.5;
            arms(fb, sh, (fret, (strum.0, strum.1 + s)), &BASSIST, k);
        }
        // singer, at the piano in the ballad
        if ballad {
            self.piano(fb, 0.6);
            let p = Pose { bob: 0.0, lean: -1.0 + (t * 1.5).sin() * 0.8, spread: 0.0, sit: true };
            let sh = body(fb, fx, 136.0, &SINGER, p, 1.0);
            let play = |i: f32| (183.0 - i * 4.0 + (t * 3.0 + i).sin(), 118.0 + (t * 5.0 + i * 2.0).sin().abs() * 1.5);
            arms(fb, sh, (play(1.0), play(0.0)), &SINGER, 1.0);
        } else {
            let kk = if solo { 0.6 } else { k.max(0.85) };
            let walk = 2.0 + 1.5 * (t * 5.0).sin();
            let p = Pose { bob: -a.pulse * 2.0 * loud as u8 as f32, lean: if goodbye { 3.0 } else { 0.0 }, spread: walk, sit: false };
            let feet = 139.0;
            let sh = body(fb, fx, feet, &SINGER, p, kk);
            let (l, r) = sh;
            let pole = scale(rgb(200, 200, 210), kk);
            let hands = if goodbye {
                ((l.0 - 1.0, l.1 + 10.0), (r.0 + 1.0, r.1 + 10.0))
            } else if act == Some(Act::Clap) && c.beats % 3 == 2 {
                ((fx - 1.0, l.1 - 13.0), (fx + 1.0, r.1 - 13.0))
            } else {
                match (c.beats / 2) % 4 {
                    0 => ((l.0 - 3.0, l.1 + 9.0), (r.0 - 2.0, r.1 - 5.0)),
                    1 => ((l.0 - 3.0, l.1 + 7.0), (r.0 + 4.0, r.1 - 12.0 - a.pulse * 2.0)),
                    2 => ((l.0 - 10.0, l.1 - 4.0), (r.0 + 10.0, r.1 - 4.0)),
                    _ => ((l.0 - 2.0, l.1 - 11.0 - a.pulse * 3.0), (r.0 + 3.0, r.1 + 8.0)),
                }
            };
            arms(fb, sh, hands, &SINGER, kk);
            // the microphone stand, held like a sceptre
            let (hx, hy) = hands.1;
            let (ex, ey) = match (c.beats / 2) % 4 {
                _ if goodbye => (hx + 2.0, hy + 12.0),
                1 => (hx + 3.0, hy - 14.0),
                2 => (hx + 13.0, hy - 3.0),
                3 => (hx + 10.0, hy - 8.0),
                _ => (hx + 4.0, hy + 13.0),
            };
            thick(fb, hx, hy - 1.0, ex, ey, pole, 1);
            fb.fill_rect(hx as i32 - 1, hy as i32 - 3, 3, 2, scale(rgb(40, 40, 40), kk));
        }
        if solo || ballad {
            // a follow spot from the front of house
            let (sx, sy) = if solo { (gx, g_feet) } else { (fx - 6.0, 136.0) };
            glow(fb, sx, sy - 18.0, 26.0, rgb(255, 240, 210), 0.35);
        }

        // pyro: columns of fire on every other beat
        let pyro = matches!(act, Some(Act::Pyro | Act::Finale)) && !goodbye;
        if pyro && a.beat && c.beats.is_multiple_of(2) {
            let pair = (c.beats / 2) % 2;
            for (i, f) in self.flames.iter_mut().enumerate() {
                if act == Some(Act::Finale) || i % 2 == pair as usize {
                    *f = 1.0;
                }
            }
        }
        for i in 0..4 {
            let f = self.flames[i];
            if f <= 0.0 {
                continue;
            }
            let x = FLAMES[i];
            let h = 74.0 * f.sqrt();
            for s in 0..h as i32 {
                let u = s as f32 / h;
                let half = (6.0 * (1.0 - u) + 1.5) * (0.6 + 0.4 * f) + (t * 40.0 + s as f32 * 0.7).sin();
                let col = if u < 0.3 { mix(rgb(255, 250, 210), rgb(255, 200, 60), u / 0.3) } else { mix(rgb(255, 170, 40), rgb(200, 40, 10), (u - 0.3) / 0.7) };
                for px in (x - half) as i32..=(x + half) as i32 {
                    add_px(fb, px, FLOOR_EDGE - s, col, (1.0 - u * u) * f.min(1.0));
                }
            }
            glow(fb, x, FLOOR_EDGE as f32 - 20.0, 30.0, rgb(255, 140, 40), 0.4 * f);
            if self.rng.f() < f * 0.5 {
                self.spawn(x, FLOOR_EDGE as f32 - h, 1, 30.0, rgb(255, 180, 60), 2);
            }
            self.flames[i] -= dt / 0.8;
        }
        if matches!(act, Some(Act::Pyro | Act::Finale)) && a.pulse > 0.2 {
            fb.flash(0.04 * a.pulse * self.flames.iter().cloned().fold(0.0, f32::max));
        }

        // fireworks and confetti for the finale and the bow
        let finale = act == Some(Act::Finale) || goodbye;
        if finale {
            if a.beat && self.rng.below(2) == 0 {
                let (x, y) = (60.0 + self.rng.f() * 264.0, 18.0 + self.rng.f() * 40.0);
                let col = rainbow(self.rng.f());
                self.spawn(x, y, 40, 70.0, col, 0);
            }
            for _ in 0..3 {
                if self.bits.len() < 900 {
                    let col = rainbow(self.rng.f());
                    let (x, ph) = (self.rng.f() * W as f32, self.rng.f() * TAU);
                    self.bits.push(Bit { x, y: -2.0, vx: 0.0, vy: 18.0 + self.rng.f() * 14.0, age: ph, life: 30.0, col, kind: 1 });
                }
            }
        }
        for b in self.bits.iter_mut() {
            b.age += dt;
            match b.kind {
                0 => b.vy += 60.0 * dt,
                2 => b.vy -= 20.0 * dt,
                _ => b.x += (b.age * 5.0).sin() * 12.0 * dt,
            }
            b.x += b.vx * dt;
            b.y += b.vy * dt;
        }
        self.bits.retain(|b| (b.kind == 1 || b.age < b.life) && b.y < H as f32 + 2.0 && b.y > -40.0);
        for b in &self.bits {
            if b.kind == 1 {
                let w = if (b.age * 7.0).sin() > 0.0 { 2 } else { 1 };
                fb.fill_rect(b.x as i32, b.y as i32, w, 1, b.col);
            } else {
                add_px(fb, b.x as i32, b.y as i32, b.col, 1.0 - b.age / b.life);
            }
        }

        self.crowd(fb, c, act, wash, goodbye);

        // the lights go up: one big flash when the show starts, a small one on each new act
        if (DOORS..DOORS + 0.5).contains(&t) {
            fb.flash(1.0 - (t - DOORS) / 0.5);
        } else if act.is_some() && age < 0.25 && t > DOORS + 1.0 {
            fb.flash((0.25 - age) * 2.0);
        }
        if goodbye {
            let w = 7.0 * 10.0 * 3.0;
            dancing_text(fb, "THANK YOU!", (W as f32 - w) / 2.0, 16.0, 3.0, t, a, c.beats, c.pal);
            let w = font::text_width("GOOD NIGHT", 2, true);
            font::text(fb, "GOOD NIGHT", (WI - w) / 2, 46, 2, true, rgb(255, 255, 255));
        }

        // the show clock
        let left = t.clamp(0.0, SECONDS) as u32;
        let clock = format!("LIVE {:02}:{:02} / {:02}:00", left / 60, left % 60, SECONDS as u32 / 60);
        let w = font::text_width(&clock, 1, false);
        let (x, y) = (WI - w - 8, HI - 11);
        fb.fill_rect(x - 10, y - 2, w + 14, 11, rgb(0, 0, 0));
        if ((t * 2.0) as u32).is_multiple_of(2) {
            fb.fill_rect(x - 7, y + 1, 4, 4, rgb(255, 40, 40));
        }
        font::text(fb, &clock, x, y, 1, false, rgb(230, 230, 230));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Audio;

    fn frame(fx: &mut Concert, fb: &mut Fb, a: &mut Audio, t: f32, song: u32) {
        a.update(t, 1.0 / 30.0, false);
        let pal = palette(3);
        let c = Ctx { t, time: t, dt: 1.0 / 30.0, a, pal: &pal, beats: a.beats, phase: 0.3, name: "", text: "", image: None, song };
        fx.draw(fb, &c);
    }

    #[test]
    fn acts_follow_the_clock_without_songs() {
        let (mut fx, mut fb, mut a) = (Concert::new(1), Fb::new(), Audio::new());
        let mut seen = Vec::new();
        let mut t = 0.0;
        while t < SECONDS {
            frame(&mut fx, &mut fb, &mut a, t, 0);
            if seen.last() != Some(&fx.act) {
                seen.push(fx.act);
            }
            t += 2.0;
        }
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5, 6, 7], "every act once, the finale last");
    }

    #[test]
    fn songs_set_the_pace_but_the_first_one_does_not_skip_the_opener() {
        let (mut fx, mut fb, mut a) = (Concert::new(2), Fb::new(), Audio::new());
        frame(&mut fx, &mut fb, &mut a, 1.0, 0);
        frame(&mut fx, &mut fb, &mut a, 5.0, 1); // the first song arrives
        assert_eq!(fx.act, 0);
        frame(&mut fx, &mut fb, &mut a, 130.0, 2);
        assert_eq!(fx.act, 1, "a new song, a new act");
        // a long song: no clock change after the usual slot once songs are known
        frame(&mut fx, &mut fb, &mut a, 130.0 + SLOT + 10.0, 2);
        assert_eq!(fx.act, 1);
        frame(&mut fx, &mut fb, &mut a, FINALE_FROM + 1.0, 2);
        assert_eq!(fx.act, FINALE, "the finale closes the show whatever plays");
    }
}
