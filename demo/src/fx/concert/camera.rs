//! The concert's camera crew. The stage is drawn whole into a frame of its own and the
//! camera films it: it cuts between shots on the beat (the singer, the guitarist, the
//! drums, the LED wall, the crowd, the wings), flies over the stage between them (push-ins,
//! pull-outs, a crane down from the lights, a swoop in over the crowd), and drifts a little
//! the whole time, like a camera on a shoulder, so it never stands quite still.
//!
//! Every act opens on the wide shot, the bow ends on it, and the dedication gets the LED
//! wall to itself while it shows. In between the director picks at random from what suits
//! the act: the crowd in the stadium wave, the guitarist in the solo, the horns in Wild
//! Dances, the UFO in the secret show. Loud acts cut fast, quiet ones linger and glide.

use std::f32::consts::PI;

use super::*;

/// Where the band members are this frame (x, feet), and the UFO if it is over the stage.
#[derive(Clone, Copy, Debug)]
pub(super) struct Spots {
    pub singer: (f32, f32),
    pub guitar: (f32, f32),
    pub bass: (f32, f32),
    pub ufo: Option<(f32, f32)>,
}

/// What the director needs to know about the show this frame.
pub(super) struct Cue {
    pub t: f32,
    pub beat: bool,
    /// Seconds per beat.
    pub period: f32,
    pub pulse: f32,
    pub act: Option<Act>,
    /// Changes whenever a new act starts (its index and start time).
    pub act_key: (usize, f32),
    pub goodbye: bool,
    /// The dedication is on the LED wall.
    pub note: bool,
    pub loud: bool,
    pub secret: bool,
}

/// Where the camera looks: the centre of the shot on the stage, how close (1 = the whole
/// stage) and how far it is turned (radians).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct View {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub roll: f32,
}

impl View {
    #[cfg(test)]
    pub const WIDE: View = View { x: W as f32 / 2.0, y: H as f32 / 2.0, zoom: 1.0, roll: 0.0 };

    /// Pushed in (and over) just enough that the shot never shows past the stage's edges,
    /// however it is turned.
    pub fn fit(mut self) -> View {
        let (hw, hh) = (W as f32 / 2.0, H as f32 / 2.0);
        let (s, c) = (self.roll.sin().abs(), self.roll.cos());
        let need = ((hw * c + hh * s) / hw).max((hw * s + hh * c) / hh);
        self.zoom = self.zoom.max(need * 1.0005).min(4.0);
        let (ex, ey) = ((hw * c + hh * s) / self.zoom, (hw * s + hh * c) / self.zoom);
        self.x = self.x.clamp(ex, W as f32 - ex);
        self.y = self.y.clamp(ey, H as f32 - ey);
        self
    }
}

/// What a shot points at.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Aim {
    Stage,
    Singer,
    Guitar,
    Bass,
    Drums,
    Wall,
    Lights,
    Crowd,
    LeftWing,
    RightWing,
    Floor,
    Ufo,
}

/// One end of a shot: what it points at, nudged by (dx, dy), how close, how turned.
#[derive(Clone, Copy, Debug)]
struct Frame {
    aim: Aim,
    dx: f32,
    dy: f32,
    zoom: f32,
    roll: f32,
}

impl Frame {
    const fn on(aim: Aim, zoom: f32) -> Frame {
        Frame { aim, dx: 0.0, dy: 0.0, zoom, roll: 0.0 }
    }
}

/// The camera goes from `a` to `b` in `len` seconds: slowly creeping in a held shot, eased
/// and banking into the turn (`bank`, radians) in a flyover.
#[derive(Clone, Copy, Debug)]
struct Shot {
    a: Frame,
    b: Frame,
    t0: f32,
    len: f32,
    fly: bool,
    bank: f32,
}

/// What the director is filming for: a new act, the dark before the show, the bow, or the
/// dedication. A change of moment gets its opening shot at once.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Moment {
    Doors,
    Act(usize, f32),
    Note(usize, f32),
    Goodbye,
}

pub(super) struct Camera {
    shot: Shot,
    moment: Option<Moment>,
    next_cut: f32,
    last: Aim,
    /// Phases of the hand-held drift, so no two shows sway alike.
    sway: [f32; 5],
    rng: Rng,
}

/// What the camera may cut to in each act; an aim listed twice comes up twice as often.
fn aims(act: Option<Act>) -> &'static [Aim] {
    use Aim::*;
    match act {
        None => &[Stage, Crowd],
        Some(Act::Clap) => &[Singer, Crowd, Crowd, Guitar, Bass, Drums, Stage],
        Some(Act::Lights) => &[Lights, Lights, Stage, Singer, Guitar, Bass, Wall, Crowd],
        Some(Act::Pyro) => &[Floor, LeftWing, RightWing, Stage, Singer, Drums, Guitar],
        Some(Act::Disco) => &[Lights, Lights, Crowd, Singer, Bass, Guitar, Stage],
        Some(Act::Stadium) => &[Crowd, Crowd, Crowd, Stage, Singer, Wall, Drums],
        Some(Act::Solo) => &[Guitar, Guitar, Guitar, Stage, Crowd, Drums],
        Some(Act::Ballad) => &[Singer, Singer, Crowd, Crowd, Wall, Stage],
        Some(Act::WildDances) => &[LeftWing, RightWing, Guitar, Bass, Drums, Floor, Stage],
        Some(Act::Tree) => &[Stage, Stage, Wall, Singer, Crowd],
        Some(Act::Stefania) => &[Singer, Singer, Guitar, LeftWing, Wall, Crowd, Stage],
        Some(Act::Finale) => &[Stage, Stage, Lights, Crowd, Floor, Singer, Wall, Drums],
    }
}

/// How close each aim is filmed (a range to pick from).
fn closeness(aim: Aim) -> (f32, f32) {
    match aim {
        Aim::Stage => (1.0, 1.15),
        Aim::Singer => (2.2, 2.8),
        Aim::Guitar | Aim::Bass => (2.0, 2.6),
        Aim::Drums => (1.8, 2.2),
        Aim::Wall => (1.7, 2.0),
        Aim::Lights => (1.4, 1.8),
        Aim::Crowd => (1.5, 2.0),
        Aim::LeftWing | Aim::RightWing => (1.6, 2.1),
        Aim::Floor => (1.5, 1.9),
        Aim::Ufo => (2.0, 2.5),
    }
}

impl Camera {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed ^ 0xCA3E);
        let sway = std::array::from_fn(|_| rng.f() * TAU);
        let hold = Frame::on(Aim::Stage, 1.0);
        Camera { shot: Shot { a: hold, b: hold, t0: 0.0, len: 1.0, fly: false, bank: 0.0 }, moment: None, next_cut: 0.0, last: Aim::Stage, sway, rng }
    }

    /// Where `f` points on the stage this frame.
    fn point(f: &Frame, s: &Spots) -> (f32, f32) {
        let (x, y) = match f.aim {
            Aim::Stage => (192.0, 108.0),
            Aim::Singer => (s.singer.0, s.singer.1 - 22.0),
            Aim::Guitar => (s.guitar.0, s.guitar.1 - 20.0),
            Aim::Bass => (s.bass.0, s.bass.1 - 20.0),
            Aim::Drums => (192.0, 88.0),
            Aim::Wall => (192.0, 42.0),
            Aim::Lights => (192.0, 34.0),
            Aim::Crowd => (192.0, 182.0),
            Aim::LeftWing => (64.0, 100.0),
            Aim::RightWing => (320.0, 100.0),
            Aim::Floor => (192.0, 128.0),
            Aim::Ufo => s.ufo.unwrap_or((192.0, 42.0)),
        };
        (x + f.dx, y + f.dy)
    }

    /// A frame on `aim`, at a random closeness, sometimes a little off-centre or tilted.
    fn frame(&mut self, aim: Aim) -> Frame {
        let (lo, hi) = closeness(aim);
        let r = &mut self.rng;
        let mut f = Frame::on(aim, r.range(lo, hi));
        f.dx = match aim {
            Aim::Crowd => r.range(-110.0, 110.0),
            Aim::Stage => 0.0,
            _ => r.range(-8.0, 8.0),
        };
        if aim != Aim::Stage && r.below(5) == 0 {
            // a Dutch angle now and then
            f.roll = r.range(0.04, 0.08) * if r.below(2) == 0 { 1.0 } else { -1.0 };
        }
        f
    }

    fn start(&mut self, t: f32, a: Frame, b: Frame, len: f32, fly: bool, bank: f32) {
        self.shot = Shot { a, b, t0: t, len: len.max(0.5), fly, bank };
        self.next_cut = t + len;
        self.last = b.aim;
    }

    /// The next shot, picked from what suits the act.
    fn pick(&mut self, cue: &Cue) {
        let list = aims(cue.act);
        let mut aim = list[self.rng.below(list.len() as u32) as usize];
        for _ in 0..3 {
            if aim != self.last {
                break;
            }
            aim = list[self.rng.below(list.len() as u32) as usize];
        }
        if cue.secret && self.rng.below(5) == 0 {
            aim = Aim::Ufo;
        }
        let close = self.frame(aim);
        let quiet = !cue.loud;
        let beat = cue.period.clamp(0.3, 1.0);
        let fly = self.rng.f() < if quiet { 0.5 } else { 0.35 };
        if !fly {
            // held: 4 to 16 beats (twice that in the quiet acts), creeping in or out a little
            let beats = [4.0, 8.0, 8.0, 16.0][self.rng.below(4) as usize] * if quiet { 2.0 } else { 1.0 };
            let mut b = close;
            b.zoom *= 1.0 + self.rng.range(-0.08, 0.12);
            b.dx += self.rng.range(-10.0, 10.0);
            self.start(cue.t, close, b, (beats * beat).clamp(2.5, 14.0), false, 0.0);
            return;
        }
        let from = match self.rng.below(4) {
            0 => Frame::on(Aim::Stage, 1.05),
            1 => Frame::on(Aim::Lights, 1.6),
            2 => {
                // in low over the crowd
                let mut f = Frame::on(Aim::Crowd, 2.1);
                f.dx = self.rng.range(-120.0, 120.0);
                f
            }
            _ => {
                let other = list[self.rng.below(list.len() as u32) as usize];
                self.frame(other)
            }
        };
        // mostly into the subject, sometimes pulling back out of it
        let (a, b) = if self.rng.below(4) == 0 { (close, from) } else { (from, close) };
        let len = self.rng.range(3.0, 6.0) * if quiet { 1.6 } else { 1.0 };
        let bank = self.rng.range(0.03, 0.07);
        self.start(cue.t, a, b, len, true, bank);
        // and stay a moment where it lands
        self.next_cut += self.rng.range(0.5, 2.5);
    }

    /// What the director does this frame; returns where the camera looks.
    pub fn direct(&mut self, cue: &Cue, spots: &Spots) -> View {
        let t = cue.t;
        let moment = if cue.goodbye {
            Moment::Goodbye
        } else if cue.act.is_none() {
            Moment::Doors
        } else if cue.note {
            Moment::Note(cue.act_key.0, cue.act_key.1)
        } else {
            Moment::Act(cue.act_key.0, cue.act_key.1)
        };
        if self.moment != Some(moment) {
            let was = self.moment.replace(moment);
            match moment {
                // the dark before the show: in from the back of the crowd
                Moment::Doors => self.start(t, Frame::on(Aim::Crowd, 2.2), Frame::on(Aim::Stage, 1.15), super::DOORS, true, 0.03),
                // the bow: pull back from the singer to the whole stage
                Moment::Goodbye => self.start(t, Frame::on(Aim::Singer, 1.8), Frame::on(Aim::Stage, 1.0), 8.0, true, 0.0),
                // the dedication: the LED wall, close
                Moment::Note(..) => self.start(t, Frame::on(Aim::Wall, 2.0), Frame::on(Aim::Wall, 2.15), super::NOTE_FOR - 1.0, false, 0.0),
                // after the dedication, back to the show with the wide shot of the stage
                Moment::Act(..) if matches!(was, Some(Moment::Note(..))) => self.pick(cue),
                // a new act opens on the wide shot, slowly pushing in
                Moment::Act(..) => self.start(t, Frame::on(Aim::Stage, 1.0), Frame::on(Aim::Stage, 1.12), 6.0, false, 0.0),
            }
        } else if matches!(moment, Moment::Act(..)) && t >= self.next_cut && (cue.beat || t > self.next_cut + 1.0) {
            self.pick(cue);
        }
        self.view(cue, spots)
    }

    /// Where the camera is this frame: along the shot, plus the hand-held drift.
    fn view(&self, cue: &Cue, spots: &Spots) -> View {
        let s = &self.shot;
        let u = ((cue.t - s.t0) / s.len).clamp(0.0, 1.0);
        let e = if s.fly { smoothstep(smoothstep(u)) } else { u };
        let (pa, pb) = (Self::point(&s.a, spots), Self::point(&s.b, spots));
        let mut v = View {
            x: lerp(pa.0, pb.0, e),
            y: lerp(pa.1, pb.1, e),
            zoom: s.a.zoom * (s.b.zoom / s.a.zoom).powf(e),
            roll: lerp(s.a.roll, s.b.roll, e),
        };
        if s.fly {
            // fly up and over, not straight across: rise and widen in the middle, bank into the turn
            let arc = (PI * e).sin();
            v.zoom /= 1.0 + 0.3 * arc;
            v.y -= 14.0 * arc;
            v.roll += s.bank * (PI * u).sin() * (pb.0 - pa.0).signum();
        }
        // hand-held: a slow, uneven drift, more of it the closer the shot
        let t = cue.t;
        let p = &self.sway;
        let amp = (1.5 + 2.0 * (v.zoom - 1.0)) / v.zoom;
        v.x += amp * (0.6 * (t * 0.37 + p[0]).sin() + 0.3 * (t * 0.91 + p[1]).sin() + 0.1 * (t * 2.3 + p[2]).sin());
        v.y += amp * (0.6 * (t * 0.29 + p[3]).sin() + 0.3 * (t * 0.83 + p[4]).sin() + 0.1 * (t * 2.1 + p[0]).sin());
        v.roll += 0.006 * ((t * 0.53 + p[2]).sin() + 0.5 * (t * 1.7 + p[4]).sin());
        if cue.loud {
            // a little punch on the beat
            v.zoom *= 1.0 + 0.015 * cue.pulse;
        }
        v.fit()
    }
}

/// Mix two pixels, `w` (0..=256) of the way from `a` to `b`.
#[inline]
fn blend(a: u32, b: u32, w: u32) -> u32 {
    let rb = (((a & 0x00FF_00FF) * (256 - w) + (b & 0x00FF_00FF) * w) >> 8) & 0x00FF_00FF;
    let g = (((a & 0x0000_FF00) * (256 - w) + (b & 0x0000_FF00) * w) >> 8) & 0x0000_FF00;
    0xFF00_0000 | rb | g
}

/// Film `stage` into `out` as `v` sees it. Pixels are scaled "sharp bilinear": crisp
/// blocks with soft edges only where one pixel meets the next, so the pixel art stays
/// pixel art up close and still glides when the camera moves.
pub(super) fn film(stage: &Fb, out: &mut Fb, v: View) {
    if v.zoom <= 1.0005 && v.roll == 0.0 {
        out.px.copy_from_slice(&stage.px);
        return;
    }
    let (s, c) = v.roll.sin_cos();
    let inv = 1.0 / v.zoom;
    // a step right on the screen, and a step down, in stage pixels
    let (ux, uy) = (c * inv, s * inv);
    let (vx, vy) = (-s * inv, c * inv);
    let (hw, hh) = (W as f32 / 2.0, H as f32 / 2.0);
    let sharp = |f: f32| (((f - 0.5) * v.zoom + 0.5).clamp(0.0, 1.0) * 256.0) as u32;
    let (xmax, ymax) = (W as f32 - 1.001, H as f32 - 1.001);
    for oy in 0..H {
        let dy = oy as f32 + 0.5 - hh;
        let dx = 0.5 - hw;
        let mut sx = v.x + dx * ux + dy * vx - 0.5;
        let mut sy = v.y + dx * uy + dy * vy - 0.5;
        let row = &mut out.px[oy * W..(oy + 1) * W];
        for px in row.iter_mut() {
            let (x, y) = (sx.clamp(0.0, xmax), sy.clamp(0.0, ymax));
            let (x0, y0) = (x as usize, y as usize);
            let (wx, wy) = (sharp(x - x0 as f32), sharp(y - y0 as f32));
            let i = y0 * W + x0;
            let top = blend(stage.px[i], stage.px[i + 1], wx);
            let bot = blend(stage.px[i + W], stage.px[i + W + 1], wx);
            *px = blend(top, bot, wy);
            sx += ux;
            sy += uy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPOTS: Spots = Spots { singer: (192.0, 139.0), guitar: (118.0, 133.0), bass: (266.0, 133.0), ufo: Some((300.0, 26.0)) };

    fn cue(t: f32, act: Option<Act>, beat: bool) -> Cue {
        Cue { t, beat, period: 0.5, pulse: 0.5, act, act_key: (1, 8.0), goodbye: false, note: false, loud: true, secret: true }
    }

    #[test]
    fn a_fitted_view_never_shows_past_the_stage() {
        let mut rng = Rng::new(7);
        for _ in 0..2000 {
            let v = View { x: rng.range(-50.0, 450.0), y: rng.range(-50.0, 260.0), zoom: rng.range(0.5, 3.0), roll: rng.range(-0.2, 0.2) }.fit();
            let (s, c) = v.roll.sin_cos();
            for (cx, cy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let (dx, dy) = (cx * W as f32 / 2.0 / v.zoom, cy * H as f32 / 2.0 / v.zoom);
                let (x, y) = (v.x + dx * c - dy * s, v.y + dx * s + dy * c);
                assert!((-0.01..=W as f32 + 0.01).contains(&x) && (-0.01..=H as f32 + 0.01).contains(&y), "{v:?} shows ({x}, {y})");
            }
        }
        assert_eq!(View::WIDE.fit(), View { zoom: 1.0005, ..View::WIDE });
    }

    #[test]
    fn the_director_cuts_on_the_beat_and_flies_between_many_shots() {
        let mut cam = Camera::new(3);
        let mut aims = Vec::new();
        let (mut cuts, mut flights, mut moved) = (0, 0, 0.0f32);
        let mut prev = cam.direct(&cue(8.0, Some(Act::Disco), false), &SPOTS);
        let mut t: f32 = 8.0;
        while t < 300.0 {
            t += 1.0 / 30.0;
            let beat = (t / 0.5).fract() < 1.0 / 15.0;
            let t0 = cam.shot.t0;
            let v = cam.direct(&cue(t, Some(Act::Disco), beat), &SPOTS);
            if cam.shot.t0 != t0 {
                assert!(beat || t > cam.shot.t0 - 1e-3, "cuts land on a beat");
                cuts += 1;
                flights += cam.shot.fly as u32;
                if !aims.contains(&cam.shot.b.aim) {
                    aims.push(cam.shot.b.aim);
                }
            }
            moved = moved.max((v.x - prev.x).abs() + (v.y - prev.y).abs());
            prev = v;
        }
        assert!(cuts > 30, "{cuts} cuts in five minutes");
        assert!(flights > 5, "{flights} flyovers");
        assert!(aims.len() >= 5, "{aims:?}");
        assert!(moved > 0.0, "the camera moves");
    }

    #[test]
    fn a_new_act_opens_wide_and_the_bow_ends_wide() {
        let mut cam = Camera::new(9);
        cam.direct(&cue(20.0, Some(Act::Clap), false), &SPOTS);
        let mut c = cue(200.0, Some(Act::Pyro), false);
        c.act_key = (2, 200.0);
        let v = cam.direct(&c, &SPOTS);
        assert_eq!(cam.shot.a.aim, Aim::Stage);
        assert!(v.zoom < 1.1, "{v:?}");
        let mut c = cue(1790.0, Some(Act::Finale), false);
        c.goodbye = true;
        cam.direct(&c, &SPOTS);
        c.t += 9.0;
        let v = cam.direct(&c, &SPOTS);
        assert!(v.zoom < 1.08, "{v:?}");
    }

    #[test]
    fn filming_the_wide_shot_copies_the_stage() {
        let mut stage = Fb::new();
        for (i, p) in stage.px.iter_mut().enumerate() {
            *p = 0xFF00_0000 | (i as u32 * 2_654_435_761 >> 8);
        }
        let mut out = Fb::new();
        film(&stage, &mut out, View::WIDE);
        assert!(out.px == stage.px);
        // at 2x with no turn, every stage pixel becomes a crisp 2x2 block in the middle
        film(&stage, &mut out, View { zoom: 2.0, ..View::WIDE });
        let (x, y) = (W / 2 + 10, H / 2 + 10);
        let src = stage.px[(H / 2 + 5) * W + W / 2 + 5];
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(out.px[(y + dy) * W + x + dx], src);
        }
    }
}
