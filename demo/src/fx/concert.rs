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
//! A second show, UKRAINE LIVE (E on the page, `#ukraine`), is ten minutes of Ukraine's
//! three Eurovision winners, one act each, staged after their Eurovision performances: fire,
//! big drums and trembita horns for Wild Dances (2004), a tree growing across the dark stage
//! for 1944 (2016), embroidery, a flute, a breakdancer and a pink bucket hat for Stefania
//! (2022), and a blue and yellow finale. The performers stay the same made-up pixel band.
//!
//! Like the Christmas scene neither is in `all()`, so the 100 demos stay as they are.

use std::f32::consts::TAU;

use super::*;
use crate::font;

/// How long the rock concert lasts, in seconds.
pub const SECONDS: f32 = 30.0 * 60.0;
/// How long the Ukraine show lasts: three songs, about three minutes each.
pub const UKRAINE_SECONDS: f32 = 10.0 * 60.0;
/// The shows the page can start (`Demo::concert`): the scene's name and its length.
pub const SHOWS: [(&str, f32); 2] = [("LIVE CONCERT", SECONDS), ("UKRAINE LIVE", UKRAINE_SECONDS)];
/// The dark minute before the lights go up (here: seconds).
const DOORS: f32 = 8.0;
/// The band takes its bow in the last seconds.
const GOODBYE: f32 = 18.0;
/// A song change in the opening seconds is the first song arriving, not a new one.
const FIRST_SONG: f32 = 25.0;

pub fn def() -> Def {
    Def { name: SHOWS[0].0, flags: VECTOR | OWN_TEXT, make: |s| Box::new(Concert::new(s, Show::Rock)) }
}

pub fn ukraine_def() -> Def {
    Def { name: SHOWS[1].0, flags: VECTOR | OWN_TEXT, make: |s| Box::new(Concert::new(s, Show::Ukraine)) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Show {
    Rock,
    Ukraine,
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
    /// Ukraine 2004: fire, big drums and trembita horns.
    WildDances,
    /// Ukraine 2016: a tree grows across the dark stage.
    Tree,
    /// Ukraine 2022: embroidery, a flute, a breakdancer, a pink bucket hat.
    Stefania,
    Finale,
}

/// How a show runs: its acts (the finale last), how long it lasts, how long an act lasts
/// without song names, and when the finale starts whatever plays.
struct Plan {
    acts: &'static [Act],
    length: f32,
    slot: f32,
    finale_from: f32,
}

const ROCK: Plan = Plan {
    acts: &[Act::Clap, Act::Lights, Act::Pyro, Act::Disco, Act::Stadium, Act::Solo, Act::Ballad, Act::Finale],
    length: SECONDS,
    slot: 224.0,
    finale_from: SECONDS - 150.0,
};

const UKRAINE: Plan = Plan {
    acts: &[Act::WildDances, Act::Tree, Act::Stefania, Act::Finale],
    length: UKRAINE_SECONDS,
    slot: 184.0,
    finale_from: UKRAINE_SECONDS - 40.0,
};

impl Show {
    fn plan(self) -> &'static Plan {
        match self {
            Show::Rock => &ROCK,
            Show::Ukraine => &UKRAINE,
        }
    }
}

/// The colours of Ukraine's flag.
const BLUE: u32 = 0xFFBB_5B00;
const YELLOW: u32 = 0xFF00_D5FF;

/// Who won Eurovision for Ukraine with each act's song: artist, song, year, points. All
/// three came first. Shown beside the LED wall while the act plays.
fn winner(act: Act) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    match act {
        Act::WildDances => Some(("RUSLANA", "WILD DANCES", "2004", "280 POINTS")),
        Act::Tree => Some(("JAMALA", "1944", "2016", "534 POINTS")),
        Act::Stefania => Some(("KALUSH ORCHESTRA", "STEFANIA", "2022", "631 POINTS")),
        _ => None,
    }
}

/// The width of a caption column beside the LED wall, between it and a speaker stack.
const CAPTION_W: i32 = 72;

/// The lines of a caption column: (text, bold, colour). Long names go one word a line.
fn winner_lines(act: Act) -> [Vec<(&'static str, bool, u32)>; 2] {
    let Some((artist, song, year, points)) = winner(act) else { return [vec![], vec![]] };
    let white = rgb(255, 255, 255);
    let mut who: Vec<_> = if font::text_width(artist, 1, true) <= CAPTION_W {
        vec![(artist, true, YELLOW)]
    } else {
        artist.split(' ').map(|w| (w, true, YELLOW)).collect()
    };
    who.push((song, false, white));
    let how = vec![("1ST PLACE", true, YELLOW), ("EUROVISION", false, white), (year, false, white), (points, false, white)];
    [who, how]
}

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
/// Wild Dances: fur and red, long dark hair.
const WARRIOR: Look = Look { skin: 0xFF96_B4E0, hair: 0xFF0F_141E, style: 2, top: 0xFF28_4678, legs: 0xFF1E_1EA0 };
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
    show: Show,
    act: usize,
    act_t0: f32,
    song: Option<u32>,
    skips: Option<u32>,
    /// Song names arrive, so they set the pace instead of the clock.
    by_song: bool,
    fans: Vec<Fan>,
    bits: Vec<Bit>,
    flames: [f32; 4],
    /// Mirror ball reflections: x phase, y, colour phase.
    dots: Vec<(f32, f32, f32)>,
    /// Yellow to white, for the Ukraine show's dancing letters over the blue.
    gold: Palette,
    rng: Rng,
}

impl Concert {
    fn new(seed: u32, show: Show) -> Self {
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
        let gold = Palette::cosine([0.95, 0.85, 0.45], [0.05, 0.15, 0.4], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        Concert { show, act: 0, act_t0: 0.0, song: None, skips: None, by_song: false, fans, bits: Vec::new(), flames: [0.0; 4], dots, gold, rng }
    }

    fn next_act(&mut self, t: f32) {
        // the finale waits for the end of the show; in a long show the acts go round again
        let p = self.show.plan();
        let finale = p.acts.len() - 1;
        self.act = if self.act + 1 >= finale && t < p.length - 600.0 { 1 } else { (self.act + 1).min(finale) };
        self.act_t0 = t;
    }

    /// Which act is on at `t`, moving on with the songs (or the clock) and on every N.
    fn follow(&mut self, t: f32, song: u32, skips: u32) {
        if self.skips.is_some_and(|s| s != skips) {
            self.next_act(t);
        }
        self.skips = Some(skips);
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
        let p = self.show.plan();
        let finale = p.acts.len() - 1;
        let age = t - self.act_t0;
        // songs set the pace; a song that never seems to end still gives way after 7 minutes
        if (!self.by_song && age > p.slot) || age > 420.0 {
            self.next_act(t);
        }
        if t > p.finale_from && self.act != finale {
            self.act = finale;
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

/// A big frame drum held in front of the body. Returns the hand that beats it (on the
/// beat it is on the skin, then it swings up) and the hand that holds the rim.
fn frame_drum(fb: &mut Fb, x: f32, hip: f32, c: &Ctx, k: f32) -> ((f32, f32), (f32, f32)) {
    let (cx, cy) = (x, hip - 4.0);
    ellipse(fb, cx, cy, 5.0, 8.0, scale(rgb(90, 50, 25), k));
    ellipse(fb, cx, cy, 4.0, 7.0, scale(rgb(225, 200, 150), (k + c.a.pulse * 0.3).min(1.0)));
    fb.hspan(cx as i32 - 3, cx as i32 + 4, cy as i32, scale(rgb(170, 40, 30), k));
    ((cx + 3.0, cy - 2.0 - 9.0 * c.phase), (cx - 4.0, cy + 5.0))
}

/// A bucket hat on a band member whose shoulders are `sh` (see `body`).
fn bucket_hat(fb: &mut Fb, sh: Hands, lean: f32, col: u32, k: f32) {
    let hx = (sh.0 .0 + sh.1 .0) / 2.0 + lean * 0.3;
    let top = sh.0 .1 - 1.5;
    let (x0, y0) = ((hx - 2.5).round() as i32, (top - 8.0).round() as i32);
    let col = scale(col, k);
    fb.fill_rect(x0 - 1, y0 - 2, 8, 4, col);
    fb.fill_rect(x0 - 3, y0 + 2, 12, 1, col);
    fb.hspan(x0, x0 + 6, y0 - 2, scale(mix(col, rgb(255, 255, 255), 0.4), 1.0));
}

/// A breakdancer spinning on their head at (x, floor), legs circling in the air.
fn breakdancer(fb: &mut Fb, x: f32, floor: f32, t: f32, c: &Ctx, k: f32) {
    let spin = t * TAU * (0.8 + c.a.bass * 0.6);
    let skin = scale(rgb(220, 170, 135), k);
    let (hx, hy) = (x, floor - 2.0);
    let hip = (x, floor - 15.0);
    thick(fb, x - 1.0, hy - 4.0, hip.0 - 1.0, hip.1, scale(rgb(245, 245, 245), k), 3);
    ellipse(fb, hx, hy, 2.5, 2.5, skin);
    for off in [0.0, std::f32::consts::PI] {
        let a = spin + off;
        let foot = (hip.0 + a.cos() * 12.0, hip.1 - 5.0 + a.sin() * 3.0);
        let leg = if a.sin() > 0.0 { scale(rgb(30, 30, 40), k) } else { scale(rgb(60, 60, 80), k) };
        thick(fb, hip.0, hip.1, foot.0, foot.1, leg, 2);
    }
    for side in [-1.0, 1.0] {
        thick(fb, hx + side, hy - 5.0, hx + side * 6.0, floor, skin, 1);
    }
}

/// One branch of the 1944 tree and everything that grows out of it. `grow` counts the
/// levels that have grown so far (fractional: the newest level is still growing).
#[allow(clippy::too_many_arguments)]
fn branch(fb: &mut Fb, x: f32, y: f32, ang: f32, len: f32, depth: u32, grow: f32, t: f32, pulse: f32, seed: u32) {
    let shown = (grow - depth as f32).clamp(0.0, 1.0);
    if shown <= 0.0 {
        return;
    }
    let ang = ang + (t * 0.6 + depth as f32).sin() * 0.015 * depth as f32;
    let l = len * shown;
    let (ex, ey) = (x + ang.cos() * l, y + ang.sin() * l);
    let col = scale(mix(rgb(255, 205, 130), rgb(170, 20, 40), depth as f32 / 8.0), 0.7 + 0.3 * pulse);
    thick(fb, x, y, ex, ey, col, (4 - depth as i32 / 2).max(1));
    if depth >= 8 {
        // blossom at the tips, glowing with the music
        glow(fb, ex, ey, 3.0, rgb(255, 80, 110), 0.5 + 0.5 * pulse);
        return;
    }
    if shown < 1.0 {
        return;
    }
    let h = |n: u32| ((seed.wrapping_mul(2_654_435_761).wrapping_add(n * 40_503)) >> 16 & 0xFF) as f32 / 255.0;
    let spread = 0.32 + 0.3 * h(1);
    let ratio = 0.72 + 0.1 * h(2);
    branch(fb, ex, ey, ang - spread, len * ratio, depth + 1, grow, t, pulse, seed * 2);
    branch(fb, ex, ey, ang + spread * (0.8 + 0.3 * h(3)), len * ratio, depth + 1, grow, t, pulse, seed * 2 + 1);
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
            Some(Act::WildDances) => if i.is_multiple_of(2) { rgb(255, 140, 30) } else { rgb(255, 50, 30) },
            Some(Act::Tree) => rgb(255, 170, 90),
            Some(Act::Stefania) => [rgb(255, 90, 180), YELLOW, rgb(40, 130, 255)][i % 3],
            Some(Act::Finale) if self.show == Show::Ukraine => if i.is_multiple_of(2) { BLUE } else { YELLOW },
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
            Some(Act::WildDances) => rgb(200, 70, 20),
            Some(Act::Tree) => rgb(50, 20, 90),
            Some(Act::Stefania) => rgb(200, 50, 140),
            Some(Act::Finale) if self.show == Show::Ukraine => BLUE,
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
            let jumpy = !matches!(act, None | Some(Act::Ballad | Act::Tree));
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
                Some(Act::Ballad | Act::Tree) => {
                    let s = (t * 1.1 + f.ph * 0.3).sin() * 4.0;
                    (f.light.then_some(((sh.1, sh.0 + 2.0), (sh.2 + s, sh.0 - reach))), f.light)
                }
                Some(Act::Clap) => {
                    let pos = if clap { ((f.x - 1.0, sh.0 - reach - 1.0), (f.x + 1.0, sh.0 - reach - 1.0)) } else { (up(-3.0), (sh.2 + 3.0, sh.0 - reach + 3.0)) };
                    (Some(pos), false)
                }
                Some(Act::Stadium | Act::Finale | Act::Stefania) => {
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

    /// The LED wall in the Ukraine show: an ornament for Wild Dances, dark for 1944 (the
    /// tree grows over it), embroidery for Stefania, the flag in the finale.
    fn ukraine_wall(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>) {
        let (sx, sy, sw, sh) = SCREEN;
        let (t, pulse) = (c.t, c.a.pulse);
        let lit = 0.7 + 0.3 * pulse;
        for y in 0..sh {
            for x in 0..sw {
                let col = match act {
                    Some(Act::WildDances) => {
                        // diamonds in red, orange and black, sliding sideways
                        let u = (x as f32 + t * 18.0) as i32;
                        let d = ((u.rem_euclid(16)) - 8).abs() + ((y.rem_euclid(16)) - 8).abs();
                        match d {
                            0..=2 => rgb(255, 190, 60),
                            3..=4 => rgb(200, 30, 20),
                            7..=8 => rgb(255, 120, 30),
                            _ => rgb(20, 8, 6),
                        }
                    }
                    Some(Act::Stefania) => {
                        // cross-stitch embroidery: red and black motifs on linen, two-pixel stitches
                        let cy = (y + (t * 6.0) as i32).rem_euclid(48) / 2;
                        let (dx, dy) = (((x / 2) % 12 - 6).abs(), (cy % 12 - 6).abs());
                        if dx + dy == 4 || (dx == dy && dx <= 2) {
                            rgb(200, 20, 30)
                        } else if dx + dy == 6 || (dx == 0 && dy == 0) {
                            rgb(20, 20, 20)
                        } else {
                            rgb(235, 228, 210)
                        }
                    }
                    Some(Act::Finale) => {
                        // the flag, waving
                        let w = (x as f32 * 0.09 - t * 3.0 + y as f32 * 0.04).sin();
                        let band = if (y as f32) < sh as f32 / 2.0 + w * 3.0 { BLUE } else { YELLOW };
                        scale(band, 0.8 + 0.2 * w)
                    }
                    _ => rgb(3, 3, 6),
                };
                let row = if y % 2 == 1 { lit * 0.75 } else { lit };
                fb.px[(sy + y) as usize * W + (sx + x) as usize] = scale(col, row);
            }
        }
        if act == Some(Act::Finale) && t - self.act_t0 < 12.0 {
            dancing_text(fb, "12 POINTS", (sx + (sw - 124) / 2) as f32, sy as f32 + 10.0, 2.0, t, c.a, c.beats, &self.gold);
        }
    }

    fn led_wall(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, wash: u32, goodbye: bool) {
        let (sx, sy, sw, sh) = SCREEN;
        fb.fill_rect(sx - 2, sy - 2, sw + 4, sh + 4, rgb(28, 28, 34));
        let lit = 0.75 + c.a.pulse * 0.25;
        let staged = self.show == Show::Ukraine && !goodbye;
        match c.image {
            _ if staged && act.is_some() => self.ukraine_wall(fb, c, act),
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
            let line = if self.show == Show::Ukraine { "UKRAINE" } else { "TONIGHT" };
            let w = font::text_width(line, 2, true);
            font::text(fb, line, sx + (sw - w) / 2, sy + 38, 2, true, rgb(255, 255, 255));
        }
    }

    /// The Ukraine show, beside the LED wall: who is on (left) and how they did at
    /// Eurovision (right), fading in as the act starts.
    fn winner_captions(&self, fb: &mut Fb, act: Act, age: f32) {
        let (sx, sy, sw, _) = SCREEN;
        let k = smoothstep(age / 1.5);
        let [who, how] = winner_lines(act);
        for (col, lines) in [(0, who), (1, how)] {
            for (i, (s, bold, c)) in lines.into_iter().enumerate() {
                let w = font::text_width(s, 1, bold);
                // the left column hugs the wall from the left, the right one from the right
                let x = if col == 0 { sx - 6 - w } else { sx + sw + 6 };
                let y = sy + 3 + i as i32 * 10;
                for row in y - 2..y + font::GH + 1 {
                    fb.hspan_blend(x - 2, x + w + 2, row, rgb(0, 0, 0), 0.55 * k);
                }
                font::text(fb, s, x, y, 1, bold, scale(c, k));
            }
        }
    }
}

impl Effect for Concert {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a, dt) = (c.t, c.a, c.dt);
        self.follow(t, c.song, c.skips);
        let plan = self.show.plan();
        let ukraine = self.show == Show::Ukraine;
        let act = (t >= DOORS).then(|| plan.acts[self.act]);
        let age = t - self.act_t0;
        let goodbye = t > plan.length - GOODBYE;
        let wash = self.wash(act, c.pal, t);
        let loud = !matches!(act, None | Some(Act::Ballad | Act::Solo | Act::Tree)) && !goodbye;

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
        let tree = act == Some(Act::Tree) && !goodbye;
        if tree {
            // grows out of the stage behind the singer and spreads over the whole back wall
            let grow = smoothstep(age / 45.0) * 9.0;
            branch(fb, 192.0, FLOOR_TOP as f32, -std::f32::consts::FRAC_PI_2, 25.0, 0, grow, t, a.pulse, 1);
        }

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
        } else if tree {
            192.0
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
                Some(Act::WildDances) => (x + 40.0 * (t * 1.2 + fi).sin(), 136.0, 16.0, 0.4 + 0.5 * a.pulse),
                Some(Act::Tree) => {
                    if i == 3 || i == 4 {
                        (192.0, 137.0, 12.0, 0.45)
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                }
                Some(Act::Stefania) => (192.0 + 150.0 * (t * 0.9 + fi * 0.8).sin(), 136.0, 14.0, flash),
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
        let k = if act.is_none() { 0.3 } else if tree { 0.15 } else if ballad || solo { 0.45 } else { 0.78 + 0.22 * a.pulse };
        self.drums(fb, c, k);
        let bang = if loud { (c.phase * TAU).sin() * 1.8 } else { 0.0 };
        let wild = act == Some(Act::WildDances) && !goodbye;
        let stefania = act == Some(Act::Stefania) && !goodbye;
        if wild {
            // big frame drums in front, trembita horns on both sides
            for (x, side) in [(118.0, 1.0), (266.0, -1.0)] {
                let p = Pose { bob: -a.pulse * 2.0, lean: bang, spread: 4.0, sit: false };
                let sh = body(fb, x, 133.0, &WARRIOR, p, k);
                let (hit, rest) = frame_drum(fb, x + side * 2.0, 119.0 + p.bob, c, k);
                let hands = if side > 0.0 { (rest, hit) } else { (hit, rest) };
                arms(fb, sh, hands, &WARRIOR, k);
            }
            for (x, side) in [(24.0, 1.0), (360.0, -1.0)] {
                let sh = body(fb, x, 134.0, &WARRIOR, Pose { bob: 0.0, lean: -side, spread: 3.0, sit: false }, k);
                let mouth = ((sh.0 .0 + sh.1 .0) / 2.0 + side * 3.0, sh.0 .1 - 6.0);
                let bell = (mouth.0 + side * 34.0, mouth.1 - 46.0);
                let blow = c.beats.is_multiple_of(4);
                thick(fb, mouth.0, mouth.1, bell.0, bell.1, scale(rgb(150, 100, 50), k), 2);
                fb.fill_rect(bell.0 as i32 - 1, bell.1 as i32 - 1, 4, 3, scale(rgb(210, 170, 90), k));
                let along = |s: f32| (mouth.0 + (bell.0 - mouth.0) * s, mouth.1 + (bell.1 - mouth.1) * s);
                arms(fb, sh, (along(0.15), along(0.3)), &WARRIOR, k);
                if blow {
                    glow(fb, bell.0, bell.1, 10.0, rgb(255, 210, 120), 0.6 * (1.0 - c.phase));
                }
            }
        }
        // guitarist; the flute player in Stefania
        if stefania {
            let p = Pose { bob: -a.pulse * 1.5, lean: bang * 0.5, spread: 3.0, sit: false };
            let sh = body(fb, gx, g_feet, &GUITARIST, p, k);
            let top = sh.0 .1 - 1.5;
            let hx = (sh.0 .0 + sh.1 .0) / 2.0;
            let (fx0, fy0, fx1, fy1) = (hx + 2.0, top - 3.0, hx + 17.0, top + 1.0);
            thick(fb, fx0, fy0, fx1, fy1, scale(rgb(225, 190, 120), k), 1);
            let at = |s: f32| (fx0 + (fx1 - fx0) * s - 1.0, fy0 + (fy1 - fy0) * s);
            arms(fb, sh, (at(0.35), at(0.7)), &GUITARIST, k);
            if a.beat {
                self.spawn(fx1, fy1, 4, 25.0, rgb(255, 230, 160), 0);
            }
        } else if !wild && !tree {
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
        if !wild && !tree {
            let p = Pose { bob: 0.0, lean: if goodbye { 3.0 } else { (t * 2.0).sin() * 1.2 }, spread: 3.0, sit: false };
            let sh = body(fb, 266.0, 133.0, &BASSIST, p, k);
            let (strum, fret) = guitar(fb, 266.0, 119.0, rgb(30, 90, 200), true, 0.0, k);
            let s = (t * TAU / a.period).sin() * 1.5;
            arms(fb, sh, (fret, (strum.0, strum.1 + s)), &BASSIST, k);
        }
        // singer: under the tree in 1944, at the piano in the ballad
        if tree {
            let p = Pose { bob: 0.0, lean: (t * 0.7).sin() * 0.6, spread: 2.0, sit: false };
            let sh = body(fb, fx, 139.0, &SINGER, p, 1.0);
            let rise = smoothstep(age / 30.0);
            let mic = (sh.1 .0 - 3.0, sh.1 .1 - 5.0);
            let reach = (sh.0 .0 - 4.0 - rise * 6.0, sh.0 .1 + 9.0 - rise * 20.0);
            arms(fb, sh, (reach, mic), &SINGER, 1.0);
            fb.fill_rect(mic.0 as i32 - 1, mic.1 as i32 - 2, 2, 3, rgb(40, 40, 40));
            glow(fb, fx, 120.0, 26.0, rgb(255, 220, 190), 0.3);
        } else if ballad {
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
            if stefania || (ukraine && goodbye) {
                bucket_hat(fb, sh, p.lean, rgb(255, 110, 190), kk);
            }
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

        if stefania {
            glow(fb, 72.0, FLOOR_EDGE as f32 - 10.0, 18.0, rgb(255, 150, 210), 0.35 + 0.3 * a.pulse);
            breakdancer(fb, 72.0, FLOOR_EDGE as f32 - 2.0, t, c, 1.0);
        }
        if ukraine && (act == Some(Act::Finale) || goodbye) {
            // sunflowers along the front of the stage, nodding on the beat
            for (i, x) in (14..WI).step_by(30).enumerate() {
                let nod = if (i as u32 + c.beats).is_multiple_of(2) { a.pulse * 2.0 } else { 0.0 };
                let (x, y) = (x as f32, FLOOR_EDGE as f32 - 9.0 + nod);
                line(fb, x, y, x, FLOOR_EDGE as f32 + 4.0, rgb(40, 140, 40));
                fb.fill_rect(x as i32 + 1, y as i32 + 6, 3, 2, rgb(50, 160, 50));
                for k in 0..10 {
                    let ang = k as f32 * TAU / 10.0 + t * 0.3;
                    fb.fill_rect((x + ang.cos() * 4.0) as i32, (y + ang.sin() * 4.0) as i32, 2, 2, YELLOW);
                }
                ellipse(fb, x + 0.5, y + 0.5, 2.5, 2.5, rgb(110, 60, 20));
            }
        }

        // pyro: columns of fire on every other beat
        let pyro = matches!(act, Some(Act::Pyro | Act::Finale | Act::WildDances)) && !goodbye;
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
        if matches!(act, Some(Act::Pyro | Act::Finale | Act::WildDances)) && a.pulse > 0.2 {
            fb.flash(0.04 * a.pulse * self.flames.iter().cloned().fold(0.0, f32::max));
        }

        // fireworks and confetti for the finale and the bow
        let finale = act == Some(Act::Finale) || goodbye;
        if finale {
            if a.beat && self.rng.below(2) == 0 {
                let (x, y) = (60.0 + self.rng.f() * 264.0, 18.0 + self.rng.f() * 40.0);
                let col = if ukraine { [BLUE, YELLOW][self.rng.below(2) as usize] } else { rainbow(self.rng.f()) };
                self.spawn(x, y, 40, 70.0, col, 0);
            }
            for _ in 0..3 {
                if self.bits.len() < 900 {
                    let col = if ukraine { [BLUE, YELLOW][self.rng.below(2) as usize] } else { rainbow(self.rng.f()) };
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
            // "thank you" in Ukrainian (the font has no Cyrillic), then in English
            let (big, small) = if ukraine { ("DYAKUYU!", "THANK YOU!") } else { ("THANK YOU!", "GOOD NIGHT") };
            let w = 7.0 * big.len() as f32 * 3.0;
            let pal = if ukraine { &self.gold } else { c.pal };
            dancing_text(fb, big, (W as f32 - w) / 2.0, 16.0, 3.0, t, a, c.beats, pal);
            let w = font::text_width(small, 2, true);
            font::text(fb, small, (WI - w) / 2, 46, 2, true, rgb(255, 255, 255));
        }

        // the Ukraine show: who is on and how they did at Eurovision
        if let (true, false, Some(act)) = (ukraine, goodbye, act) {
            self.winner_captions(fb, act, age);
        }

        // the show clock
        let left = t.clamp(0.0, plan.length) as u32;
        let clock = format!("LIVE {:02}:{:02} / {:02}:00", left / 60, left % 60, plan.length as u32 / 60);
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
        let skips = fx.skips.unwrap_or(0);
        let c = Ctx { t, time: t, dt: 1.0 / 30.0, a, pal: &pal, beats: a.beats, phase: 0.3, name: "", text: "", image: None, song, skips };
        fx.draw(fb, &c);
    }

    #[test]
    fn acts_follow_the_clock_without_songs() {
        let (mut fx, mut fb, mut a) = (Concert::new(1, Show::Rock), Fb::new(), Audio::new());
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
        let (mut fx, mut fb, mut a) = (Concert::new(2, Show::Rock), Fb::new(), Audio::new());
        frame(&mut fx, &mut fb, &mut a, 1.0, 0);
        frame(&mut fx, &mut fb, &mut a, 5.0, 1); // the first song arrives
        assert_eq!(fx.act, 0);
        // N moves on at once, even in the opening seconds
        let pal = palette(3);
        let c = Ctx { t: 6.0, time: 6.0, dt: 1.0 / 30.0, a: &a, pal: &pal, beats: 0, phase: 0.0, name: "", text: "", image: None, song: 1, skips: 1 };
        fx.draw(&mut fb, &c);
        assert_eq!(fx.act, 1, "N skips to the next act");
        frame(&mut fx, &mut fb, &mut a, 7.0, 1);
        assert_eq!(fx.act, 1);
        fx.act = 0;
        frame(&mut fx, &mut fb, &mut a, 130.0, 2);
        assert_eq!(fx.act, 1, "a new song, a new act");
        // a long song: no clock change after the usual slot once songs are known
        frame(&mut fx, &mut fb, &mut a, 130.0 + ROCK.slot + 10.0, 2);
        assert_eq!(fx.act, 1);
        frame(&mut fx, &mut fb, &mut a, ROCK.finale_from + 1.0, 2);
        assert_eq!(fx.act, 7, "the finale closes the show whatever plays");
    }

    #[test]
    fn the_ukraine_show_plays_its_three_winners_then_the_finale() {
        let (mut fx, mut fb, mut a) = (Concert::new(3, Show::Ukraine), Fb::new(), Audio::new());
        let mut seen = Vec::new();
        let mut t = 0.0;
        while t < UKRAINE_SECONDS {
            frame(&mut fx, &mut fb, &mut a, t, 0);
            let act = UKRAINE.acts[fx.act];
            if seen.last() != Some(&act) {
                seen.push(act);
            }
            t += 1.0;
        }
        assert_eq!(seen, vec![Act::WildDances, Act::Tree, Act::Stefania, Act::Finale]);
        // with songs: the three songs set the pace and the finale still comes at the end
        let (mut fx, mut fb, mut a) = (Concert::new(4, Show::Ukraine), Fb::new(), Audio::new());
        frame(&mut fx, &mut fb, &mut a, 3.0, 1);
        frame(&mut fx, &mut fb, &mut a, 180.0, 2);
        frame(&mut fx, &mut fb, &mut a, 360.0, 3);
        assert_eq!(UKRAINE.acts[fx.act], Act::Stefania);
        frame(&mut fx, &mut fb, &mut a, 545.0, 3);
        assert_eq!(UKRAINE.acts[fx.act], Act::Stefania, "Stefania plays to its end");
        frame(&mut fx, &mut fb, &mut a, UKRAINE.finale_from + 1.0, 3);
        assert_eq!(UKRAINE.acts[fx.act], Act::Finale);
    }

    #[test]
    fn each_winner_is_named_with_its_eurovision_placement_beside_the_wall() {
        let (sx, sy, sw, _) = SCREEN;
        for &act in &UKRAINE.acts[..3] {
            let [who, how] = winner_lines(act);
            assert!(who.len() >= 2 && how.len() == 4, "{act:?}");
            assert_eq!(how[0].0, "1ST PLACE");
            for (s, bold, _) in who.iter().chain(&how) {
                assert!(font::text_width(s, 1, *bold) <= CAPTION_W, "{s} is too wide");
                assert_eq!(font::clean(s, 40), *s, "{s} has letters the font cannot draw");
            }
            // both columns clear the speaker stacks (x 4..40 and 344..380) and their tops (y 60)
            let widest = |lines: &[(&str, bool, u32)]| lines.iter().map(|(s, b, _)| font::text_width(s, 1, *b)).max().unwrap();
            assert!(sx - 6 - widest(&who) - 2 > 40 && sx + sw + 6 + widest(&how) + 2 < 344);
            assert!(sy + 3 + who.len().max(how.len()) as i32 * 10 < 60);
        }
        assert_eq!(winner(Act::WildDances).map(|w| w.0), Some("RUSLANA"));
        assert_eq!(winner(Act::Stefania).map(|w| w.2), Some("2022"));
        assert!(winner(Act::Finale).is_none() && winner(Act::Clap).is_none());
    }
}
