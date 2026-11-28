//! The stage of the Kite show: Dalhalla, an open-air theatre in an old limestone quarry near
//! Rättvik, where Kite played their homecoming concert on 2 September 2023. Steep pale walls
//! with pines along the rim, a lake between the stage and the audience, the September sky
//! going from dusk to night over the 87 minutes of the record. The lights live on the quarry
//! as much as on the stage: colours on the rock walls, lasers over the water, searchlights
//! into the sky, and before Glassy Eyes eight red signal flares lit on the rock face.
//!
//! The duo is drawn the way this stage draws everyone: a singer with a microphone in hand and
//! a synth player behind a rig of keyboards, pixel people rather than portraits.

use super::*;

/// The lake starts here; everything above it is mirrored in it.
const WATER: i32 = 124;
/// The top of the stage floor, where the band stands.
const DECK: i32 = 118;
/// The stage, from x to x.
const STAGE: (i32, i32) = (112, 272);
/// The lighting truss: its top and the two towers carrying it.
const TRUSS_TOP: i32 = 54;
const TOWERS: [i32; 2] = [114, 268];
/// The LED wall behind the band: x, y, w, h.
const WALL: (i32, i32, i32, i32) = (150, 66, 84, 34);
/// The synth rig, stage right.
const KEYS_X: f32 = 240.0;
/// Where the eight red signal flares burn on the rock face in Glassy Eyes, four a side.
const FLARE_SPOTS: [(f32, f32); 8] =
    [(22.0, 74.0), (50.0, 96.0), (74.0, 62.0), (96.0, 100.0), (288.0, 100.0), (310.0, 64.0), (334.0, 94.0), (362.0, 76.0)];

/// The singer: dark coat, dark hair.
const VOICE: Look = Look { skin: 0xFF8C_ACE0, hair: 0xFF10_1014, style: 0, top: 0xFF20_1A16, legs: 0xFF1A_1414 };
/// The synth player: grey shirt, brown hair.
const SYNTHS: Look = Look { skin: 0xFF96_B4E2, hair: 0xFF30_4860, style: 2, top: 0xFF3A_3434, legs: 0xFF1E_1818 };

/// A number in 0..1 from `n`, the same every frame.
fn hash(n: u32) -> f32 {
    let mut h = n.wrapping_mul(0x9E37_79B1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA77);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}

/// The quarry rim at x: high walls at the sides, lower behind the stage, a little ragged.
fn rim(x: f32) -> f32 {
    let u = ((x - 192.0) / 192.0).abs();
    46.0 - 40.0 * u.powf(2.2) + 2.0 * (x * 0.31).sin() + 1.5 * (x * 0.071 + 1.0).sin()
}

/// How much daylight is left: the show starts at dusk and it is night after 25 minutes.
fn daylight(t: f32) -> f32 {
    1.0 - smoothstep((t - 30.0) / 1500.0)
}

/// A beam from a lamp at (x0, y0) up into the sky towards (x1, y1).
#[allow(clippy::too_many_arguments)]
fn skybeam(fb: &mut Fb, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: u32, k: f32) {
    if k <= 0.01 || y1 >= y0 {
        return;
    }
    for y in (y1.max(0.0) as i32)..=(y0.min(H as f32 - 1.0) as i32) {
        let s = (y0 - y as f32) / (y0 - y1); // 0 at the lamp
        let cx = x0 + (x1 - x0) * s;
        let hw = 1.0 + w * s;
        let fade = k * (0.6 - 0.45 * s);
        for x in ((cx - hw).floor() as i32).max(0)..=((cx + hw).ceil() as i32).min(WI - 1) {
            let d = 1.0 - (x as f32 - cx).abs() / hw;
            if d > 0.0 {
                add_px(fb, x, y, c, fade * d * d);
            }
        }
    }
}

/// A thin laser line, fading towards its end.
fn laser(fb: &mut Fb, x0: f32, y0: f32, x1: f32, y1: f32, c: u32, k: f32) {
    let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1.0) as i32;
    for i in 0..=n {
        let s = i as f32 / n as f32;
        add_px(fb, (x0 + (x1 - x0) * s) as i32, (y0 + (y1 - y0) * s) as i32, c, k * (1.0 - 0.6 * s));
    }
}

impl Concert {
    pub(super) fn dalhalla(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.t, c.a);
        let plan = self.show.plan();
        let act = (t >= DOORS).then(|| plan.acts[self.act]);
        let age = t - self.act_t0;
        let goodbye = t > plan.length - GOODBYE;
        let wash = self.wash(act, c.pal, t);
        let day = daylight(t);
        let hue = self.act as f32 * 0.137; // every song its own colours

        self.sky(fb, c, day);
        self.quarry(fb, c, act, wash, day, hue);
        self.stage(fb, c, act, wash, goodbye);
        self.kite_lights(fb, c, act, hue, goodbye);
        self.duo(fb, c, act, goodbye);
        self.quarry_fx(fb, c, act, age, goodbye);
        self.lake(fb, t);
        self.crowd(fb, c, act, wash, goodbye);
        if act == Some(Act::Strobe) && !goodbye && a.pulse > 0.5 && c.beats.is_multiple_of(2) {
            fb.flash(0.22 * a.pulse);
        }

        // the lights go up: one big flash when the show starts, a small one on each new song
        if (DOORS..DOORS + 0.5).contains(&t) {
            fb.flash(1.0 - (t - DOORS) / 0.5);
        } else if act.is_some() && age < 0.25 && t > DOORS + 1.0 {
            fb.flash((0.25 - age) * 2.0);
        }
        self.kite_texts(fb, c, act, age, goodbye);
        show_clock(fb, t, plan.length);
    }

    /// The September sky over the quarry, dusk to night, the stars coming out.
    fn sky(&self, fb: &mut Fb, c: &Ctx, day: f32) {
        let top = mix(rgb(2, 3, 12), rgb(40, 52, 112), day);
        let horizon = mix(rgb(16, 18, 40), rgb(236, 132, 82), day);
        for y in 0..WATER {
            let k = (y as f32 / 52.0).clamp(0.0, 1.0).powf(1.3);
            fb.row(y, mix(top, horizon, k));
        }
        for &(px, py, ph) in &self.dots {
            let (x, y) = (px * W as f32, 1.0 + py * 0.25);
            if y < rim(x) - 3.0 {
                let twinkle = 0.55 + 0.45 * (c.t * 2.0 + ph * 40.0).sin();
                add_px(fb, x as i32, y as i32, rgb(220, 230, 255), (1.0 - day) * twinkle);
            }
        }
    }

    /// The limestone walls and the pines on the rim, lit by the show.
    #[allow(clippy::too_many_arguments)]
    fn quarry(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, wash: u32, day: f32, hue: f32) {
        let (t, a) = (c.t, c.a);
        let stone = rgb(152, 148, 138);
        let base = 0.1 + 0.3 * day;
        let pulse = 0.7 + 0.3 * a.pulse;
        for x in 0..WI {
            let xf = x as f32;
            let top = rim(xf) as i32;
            let crack = hash(x as u32 / 2 + 77) > 0.93;
            // what lights this strip of rock: the stage, coloured floods, or nothing much
            let (lit, k) = match act {
                Some(Act::Walls) => {
                    let band = 0.6 + 0.4 * (xf * 0.08 + t * 0.5).sin();
                    (rainbow(xf / 150.0 + t * 0.03 + hue), 0.55 * band * pulse)
                }
                Some(Act::Tape) => {
                    // gobos drifting over the rock on their own
                    let g = ((xf * 0.05 + t * 0.4).sin() * (xf * 0.021 - t * 0.27).sin()).max(0.0);
                    (rainbow(xf / 300.0 + t * 0.05 + hue), 0.6 * g)
                }
                Some(Act::Strobe) => (rgb(220, 225, 255), 0.45 * a.pulse * a.pulse),
                Some(Act::Lasers) => (wash, 0.12 * pulse),
                Some(Act::Fireworks) => (rainbow(t * 0.1 + xf / 400.0), 0.18 * pulse),
                None => (wash, 0.0),
                _ => (wash, 0.22 * pulse),
            };
            for y in top..WATER {
                // limestone in layers: long thin streaks, a little grain, the odd crack
                let layer = hash((y as u32 / 2) * 131 + (x as u32 + y as u32 * 7) / 14);
                let grain = hash(x as u32 / 2 + (y as u32 / 2) * 57);
                let n = 0.7 * layer + 0.3 * grain;
                let rock = scale(stone, base * (0.82 + 0.18 * n) * if crack { 0.65 } else { 1.0 });
                // the light falls off away from the stage and down into the dark
                let near = 1.0 - ((xf - 192.0).abs() / 260.0).min(0.85);
                fb.px[y as usize * W + x as usize] = add(rock, scale(lit, k * near * (0.75 + 0.25 * n)));
            }
            // the pine forest on the rim, black against the sky
            let pine = rgb(5, 8, 9);
            let h = 3.0 + hash(x as u32 / 4 + 9) * 8.0;
            if x % 4 == 0 {
                fb.tri([(xf - 3.0, top as f32 + 1.0), (xf + 3.0, top as f32 + 1.0), (xf, top as f32 - h)], pine, 1.0);
            }
            fb.hspan(x, x + 1, top, pine);
            fb.hspan(x, x + 1, top + 1, pine);
        }
    }

    /// The stage: its floor at the water's edge, the truss on two towers, the LED wall.
    fn stage(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, wash: u32, goodbye: bool) {
        let a = c.a;
        let (x0, x1) = STAGE;
        fb.fill_rect(x0, DECK, x1 - x0, WATER - DECK, rgb(24, 22, 28));
        fb.hspan(x0, x1, DECK, scale(wash, 0.5 + 0.3 * a.pulse));
        // towers and the truss across
        let metal = rgb(64, 64, 74);
        for &tx in &TOWERS {
            fb.fill_rect(tx - 2, TRUSS_TOP, 1, DECK - TRUSS_TOP, metal);
            fb.fill_rect(tx + 2, TRUSS_TOP, 1, DECK - TRUSS_TOP, metal);
            for y in (TRUSS_TOP..DECK).step_by(5) {
                line(fb, (tx - 2) as f32, y as f32, (tx + 2) as f32, y as f32 + 5.0, rgb(46, 46, 54));
            }
        }
        fb.hspan(TOWERS[0] - 2, TOWERS[1] + 3, TRUSS_TOP, metal);
        fb.hspan(TOWERS[0] - 2, TOWERS[1] + 3, TRUSS_TOP + 4, metal);
        for x in (TOWERS[0]..TOWERS[1]).step_by(5) {
            line(fb, x as f32, TRUSS_TOP as f32, x as f32 + 3.0, TRUSS_TOP as f32 + 4.0, rgb(46, 46, 54));
        }
        // the LED wall: a spectrum in the act's colours, KITE in the dark before the show
        let (sx, sy, sw, sh) = WALL;
        fb.fill_rect(sx - 2, sy - 2, sw + 4, sh + 4, rgb(24, 24, 30));
        fb.fill_rect(sx, sy, sw, sh, scale(wash, 0.1));
        let dark = act.is_none() || goodbye || matches!(act, Some(Act::Spot));
        if !dark {
            let bars = 14;
            for b in 0..bars {
                let lo = 2 + b * 10;
                let v = c.a.bins[lo..lo + 10].iter().map(|&v| v as f32).sum::<f32>() / (10.0 * 255.0);
                let v = if c.a.autopilot { 0.3 + 0.5 * a.pulse * (0.6 + 0.4 * (b as f32 * 0.7 + c.t * 3.0).sin()) } else { v };
                let h = (v * sh as f32 * 0.9) as i32;
                fb.fill_rect(sx + 1 + b as i32 * 6, sy + sh - h, 4, h, scale(self.colour(act, c.pal, c.t, b), 0.85));
            }
        }
        if act.is_none() || (act == Some(Act::Dusk) && c.t - self.act_t0 < 30.0) {
            let w = font::text_width("KITE", 2, true);
            font::text(fb, "KITE", sx + (sw - w) / 2, sy + 10, 2, true, scale(rgb(235, 240, 255), 0.6 + 0.4 * a.pulse));
        }
        for y in (sy..sy + sh).step_by(2) {
            fb.hspan_blend(sx, sx + sw, y, rgb(0, 0, 0), 0.35);
        }
    }

    /// The lights: beams on the stage, beams and lasers out over the quarry.
    fn kite_lights(&mut self, fb: &mut Fb, c: &Ctx, act: Option<Act>, hue: f32, goodbye: bool) {
        let (t, a) = (c.t, c.a);
        let flash = 0.45 + 0.55 * a.pulse;
        let cans: [f32; 6] = std::array::from_fn(|i| 130.0 + i as f32 * 25.0);
        for (i, &x) in cans.iter().enumerate() {
            fb.fill_rect(x as i32 - 2, TRUSS_TOP + 5, 5, 4, rgb(34, 34, 42));
            let col = self.colour(act, c.pal, t, i);
            let fi = i as f32;
            let (tx, w, k) = match act {
                _ if goodbye => (x + (192.0 - x) * 0.4, 14.0, 0.5),
                None => (0.0, 0.0, 0.0),
                Some(Act::Dusk) => (x, 10.0, 0.25 * smoothstep((t - DOORS) / 20.0)),
                Some(Act::Spot) => (0.0, 0.0, 0.0),
                Some(Act::Strobe) => (x, 12.0, if c.beats.is_multiple_of(2) { flash } else { 0.15 }),
                Some(Act::Tape) => (192.0 + 70.0 * (t * 0.3 + fi * 1.1).sin(), 12.0, 0.4),
                Some(Act::Flares) => (x + 10.0 * (t * 0.4 + fi).sin(), 12.0, 0.2 + 0.15 * a.pulse),
                _ => (192.0 + 60.0 * (t * 0.6 + fi * 0.9).sin(), 11.0, 0.3 * flash),
            };
            fb.fill_rect(x as i32 - 1, TRUSS_TOP + 8, 3, 2, scale(col, 0.4 + 0.6 * k.min(1.0)));
            beam(fb, x, TRUSS_TOP as f32 + 10.0, tx, DECK as f32 + 2.0, w, col, k);
        }
        let floor = DECK as f32;
        match act {
            // the opener: four white beams fanning slowly up into the evening
            Some(Act::Dusk) if !goodbye => {
                let up = smoothstep((t - DOORS) / 25.0);
                for (i, x0) in [128.0, 160.0, 224.0, 256.0].into_iter().enumerate() {
                    let x1 = x0 + (x0 - 192.0) * 1.2 + 30.0 * (t * 0.12 + i as f32).sin();
                    skybeam(fb, x0, floor, x1, -20.0, 9.0, rgb(230, 236, 255), 0.5 * up);
                }
            }
            // searchlights from both towers and the deck, sweeping the night
            Some(Act::Sky) | Some(Act::Fireworks) if !goodbye => {
                let n = if act == Some(Act::Sky) { 6 } else { 2 };
                for i in 0..n {
                    let x0 = 120.0 + i as f32 * (144.0 / (n - 1).max(1) as f32);
                    let x1 = x0 + 150.0 * (t * 0.35 + i as f32 * 0.9).sin();
                    let col = mix(rgb(225, 235, 255), rainbow(hue + i as f32 * 0.05), 0.25);
                    skybeam(fb, x0, floor, x1, -30.0, 7.0, col, 0.45 + 0.2 * a.pulse);
                }
            }
            // fans of laser lines from above the LED wall, over the lake, the crowd and the rock
            Some(Act::Lasers) if !goodbye => {
                let (lx, ly) = (192.0, (WALL.1 - 6) as f32);
                let open = 0.35 + 0.25 * (t * 0.8).sin() + 0.15 * a.pulse;
                let spin = t * 0.5;
                for i in 0..11 {
                    let s = i as f32 / 10.0 - 0.5;
                    let ang = std::f32::consts::FRAC_PI_2 + s * open * 2.6 + 0.4 * spin.sin();
                    let (ex, ey) = (lx + ang.cos() * 420.0, ly + ang.sin() * 420.0);
                    let col = if i % 2 == 0 { rgb(40, 255, 120) } else { rgb(60, 220, 255) };
                    laser(fb, lx, ly, ex, ey, col, 0.7 + 0.3 * a.pulse);
                }
                // and a sheet sweeping up over the rim on the beat
                if c.beats % 8 >= 4 {
                    for i in 0..7 {
                        let ang = -std::f32::consts::FRAC_PI_2 + (i as f32 - 3.0) * 0.25 + 0.3 * (t * 1.3).sin();
                        laser(fb, lx, ly, lx + ang.cos() * 300.0, ly + ang.sin() * 300.0, rgb(40, 255, 120), 0.5);
                    }
                }
            }
            _ => {}
        }
    }

    /// The duo: the singer at the front, microphone in hand; the synth player behind the rig.
    /// From tape the stage is empty; at the end they come to the front to say goodbye.
    fn duo(&mut self, fb: &mut Fb, c: &Ctx, act: Option<Act>, goodbye: bool) {
        let (t, a) = (c.t, c.a);
        let tape = act == Some(Act::Tape) && !goodbye;
        let quiet = matches!(act, None | Some(Act::Dusk | Act::Spot));
        let k = if act.is_none() { 0.3 } else if act == Some(Act::Spot) { 0.5 } else { 0.78 + 0.22 * a.pulse };
        let feet = DECK as f32 + 3.0;
        // the synth rig on its riser, stage right
        fb.fill_rect(KEYS_X as i32 - 18, DECK - 4, 36, 4, rgb(30, 28, 34));
        let player = !tape && !goodbye;
        let sh = player.then(|| {
            let p = Pose { bob: if quiet { 0.0 } else { -a.pulse * 1.2 }, lean: (t * 0.9).sin() * 0.6, spread: 2.0, sit: false };
            body(fb, KEYS_X + 3.0, (DECK - 4) as f32, &SYNTHS, p, k)
        });
        let stand = scale(rgb(90, 90, 100), k.max(0.5));
        line(fb, KEYS_X - 11.0, (DECK - 4) as f32, KEYS_X + 9.0, 104.0, stand);
        line(fb, KEYS_X + 9.0, (DECK - 4) as f32, KEYS_X - 11.0, 104.0, stand);
        for (y, x0, w) in [(103, KEYS_X as i32 - 15, 28), (97, KEYS_X as i32 - 12, 24)] {
            fb.fill_rect(x0, y, w, 3, rgb(14, 14, 18));
            for x in (x0 + 1..x0 + w - 1).step_by(2) {
                fb.put(x, y, scale(rgb(225, 225, 220), k.max(0.4)));
            }
            // the synths' little lights, blinking with the music
            for i in 0..4 {
                let on = (c.beats + i as u32 + y as u32).is_multiple_of(3);
                let col = if on { rainbow(i as f32 * 0.2 + t * 0.05) } else { rgb(40, 40, 50) };
                fb.put(x0 + 3 + i * 6, y + 2, col);
            }
        }
        if let Some(sh) = sh {
            let play = |i: f32, y: f32| (KEYS_X - 9.0 + i * 13.0 + 2.0 * (t * (3.0 + i) + i).sin(), y);
            arms(fb, sh, (play(0.0, 102.0), play(1.0, 96.0)), &SYNTHS, k);
        }
        if tape {
            return;
        }
        if goodbye {
            // both at the front, waving, then a bow
            let bow = smoothstep((t - (self.show.plan().length - GOODBYE) - 9.0) / 3.0);
            for (x, lk) in [(178.0, &VOICE), (206.0, &SYNTHS)] {
                let p = Pose { bob: 0.0, lean: 3.0 * bow, spread: 2.5, sit: false };
                let sh = body(fb, x, feet, lk, p, 1.0);
                let sway = (t * 3.0).sin() * 3.0 * (1.0 - bow);
                let (l, r) = sh;
                let up = ((l.0 - 3.0 + sway, l.1 - 12.0 + 18.0 * bow), (r.0 + 3.0 + sway, r.1 - 12.0 + 18.0 * bow));
                arms(fb, sh, up, lk, 1.0);
            }
            return;
        }
        let spot = act == Some(Act::Spot);
        let fx = if spot { 186.0 } else { 186.0 + 22.0 * (t * 0.19).sin() };
        let kk = if act.is_none() { 0.35 } else { k.max(0.85) };
        let p = Pose { bob: if quiet { 0.0 } else { -a.pulse * 1.5 }, lean: (t * 0.8).sin() * 0.8, spread: 2.5, sit: false };
        let sh = body(fb, fx, feet, &VOICE, p, kk);
        let (l, r) = sh;
        // the microphone at the mouth; the other hand reaches out slowly, or up on the loud beats
        let mic = (r.0 - 3.0, r.1 - 6.0);
        let reach = if quiet {
            (l.0 - 9.0 + 3.0 * (t * 0.7).sin(), l.1 + 1.0 - 6.0 * (t * 0.5).sin().abs())
        } else {
            match (c.beats / 4) % 3 {
                0 => (l.0 - 3.0, l.1 - 12.0 - a.pulse * 2.0),
                1 => (l.0 - 11.0, l.1 - 3.0),
                _ => (l.0 - 2.0, l.1 + 9.0),
            }
        };
        arms(fb, sh, (reach, mic), &VOICE, kk);
        fb.fill_rect(mic.0 as i32 - 1, mic.1 as i32 - 2, 2, 3, rgb(40, 40, 40));
        if spot {
            glow(fb, fx, feet - 18.0, 26.0, rgb(255, 240, 215), 0.4);
        }
    }

    /// What happens on the rock and in the sky: the red flares, the fireworks, smoke and sparks.
    fn quarry_fx(&mut self, fb: &mut Fb, c: &Ctx, act: Option<Act>, age: f32, goodbye: bool) {
        let (t, a, dt) = (c.t, c.a, c.dt);
        if act == Some(Act::Flares) && !goodbye {
            // lit one after the other as the song begins, then burning to its end
            for (i, &(x, y)) in FLARE_SPOTS.iter().enumerate() {
                let on = smoothstep((age - 1.0 - i as f32 * 0.7) / 0.6);
                if on <= 0.0 {
                    continue;
                }
                let flick = 0.8 + 0.2 * (t * 23.0 + i as f32 * 3.1).sin() * (t * 7.0 + i as f32).sin();
                glow(fb, x, y, 46.0, rgb(255, 30, 20), 0.35 * on * flick);
                glow(fb, x, y, 7.0, rgb(255, 120, 90), 0.9 * on * flick);
                add_px(fb, x as i32, y as i32, rgb(255, 240, 220), on);
                if self.rng.f() < 0.5 * on {
                    // red smoke drifting up the rock
                    self.spawn(x, y - 2.0, 1, 8.0, rgb(150, 40, 45), 2);
                }
            }
        }
        if (act == Some(Act::Fireworks) || goodbye) && a.beat {
            // a rocket on every beat over the rim, two at a time on the loud ones
            for _ in 0..1 + (a.pulse > 0.8) as usize {
                let (x, y) = (40.0 + self.rng.f() * 304.0, 6.0 + self.rng.f() * 34.0);
                let col = mix(rainbow(self.rng.f()), rgb(255, 255, 255), 0.25);
                self.spawn(x, y, 60, 75.0, col, 0);
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
        self.bits.retain(|b| b.age < b.life * if b.kind == 2 { 3.0 } else { 1.0 } && b.y < WATER as f32 && b.y > -40.0);
        for b in &self.bits {
            if b.kind == 2 {
                // smoke: soft and slow
                glow(fb, b.x, b.y, 3.0 + b.age * 2.0, b.col, 0.25 * (1.0 - b.age / (b.life * 3.0)));
            } else {
                add_px(fb, b.x as i32, b.y as i32, b.col, 1.0 - b.age / b.life);
            }
        }
    }

    /// The lake between the stage and the audience: everything above it, upside down,
    /// darker and rippling.
    fn lake(&self, fb: &mut Fb, t: f32) {
        // seen from the terraces the water is foreshortened: each row mirrors two
        for y in WATER..CROWD_TOP {
            let d = y - WATER;
            let src = (WATER - 1 - d * 2).max(0);
            let ripple = ((y as f32 * 0.9 + t * 2.2).sin() * 1.3 + (y as f32 * 0.37 - t * 1.3).sin()) as i32;
            let deep = 0.22 + 0.015 * d as f32;
            for x in 0..WI {
                let sx = (x + ripple).clamp(0, WI - 1);
                let p = fb.px[src as usize * W + sx as usize];
                fb.px[y as usize * W + x as usize] = mix(scale(p, 0.7), rgb(4, 8, 18), deep.min(0.7));
            }
        }
    }

    /// The words on the screen: KITE before the show, each song as it starts, the goodbye.
    fn kite_texts(&self, fb: &mut Fb, c: &Ctx, act: Option<Act>, age: f32, goodbye: bool) {
        let t = c.t;
        if goodbye {
            let (big, small) = ("TACK DALHALLA!", "GOD NATT");
            let w = 7.0 * big.len() as f32 * 3.0;
            dancing_text(fb, big, (W as f32 - w) / 2.0, 10.0, 3.0, t, c.a, c.beats, c.pal);
            let w = font::text_width(small, 2, true);
            font::text(fb, small, (WI - w) / 2, 40, 2, true, rgb(255, 255, 255));
            return;
        }
        if act.is_none() || (act == Some(Act::Dusk) && age < 25.0) {
            let k = if act.is_none() { smoothstep(t / 2.0) } else { smoothstep((25.0 - age) / 4.0) };
            let line = "LIVE AT DALHALLA";
            let w = font::text_width(line, 1, true);
            font::text(fb, line, (WI - w) / 2, 6, 1, true, scale(rgb(255, 225, 190), k));
        }
        if act.is_none() {
            return;
        }
        // the song that starts: its number and name, then "from tape" where it was
        let k = smoothstep(age / 1.0) * smoothstep((10.0 - age) / 1.5);
        if k <= 0.0 {
            return;
        }
        for (i, line) in song_caption(self.act).iter().enumerate() {
            let bold = i == 0;
            let w = font::text_width(line, 1, bold);
            let y = 18 + i as i32 * 10;
            for row in y - 2..y + font::GH + 1 {
                fb.hspan_blend(6, 10 + w, row, rgb(0, 0, 0), 0.55 * k);
            }
            font::text(fb, line, 8, y, 1, bold, scale(if bold { rgb(255, 225, 190) } else { rgb(230, 230, 230) }, k));
        }
    }
}

/// The caption as song `i` of the record starts: "3/17 TRUE COLOURS", and "FROM TAPE" for
/// the two songs played from tape.
pub(super) fn song_caption(i: usize) -> Vec<String> {
    let i = i.min(KITE_SONGS.len() - 1);
    let mut lines = vec![format!("{}/{} {}", i + 1, KITE_SONGS.len(), KITE_SONGS[i].0)];
    if KITE.acts[i] == Act::Tape {
        lines.push("FROM TAPE".to_string());
    }
    lines
}
