//! The seven parts of the original UFO demo, as library effects. With palette 0
//! (RAINBOW) they look exactly like the first version; other palettes recolour them.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::font::{Logo, ScrollStyle, Scroller};
use crate::zx::Zx;

const FOV: f32 = 190.0;

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "COPPER", flags: RASTER | OWN_TEXT, make: |s| Box::new(Copper::new(s)) });
    v.push(Def { name: "GLENZ", flags: VECTOR | SCROLL_BOTTOM, make: |s| Box::new(Glenz::new(s)) });
    v.push(Def { name: "VECTOR BALLS", flags: VECTOR | LOGO_TOP, make: |s| Box::new(Balls::new(s)) });
    v.push(Def { name: "TWISTER", flags: RASTER | SCROLL_BOTTOM, make: |s| Box::new(Twister::new(s)) });
    v.push(Def { name: "TUNNEL", flags: DEFORM, make: |s| Box::new(Tunnel::new(s)) });
    v.push(Def { name: "KEFRENS", flags: RASTER | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(Kefrens::new(s)) });
    v.push(Def { name: "SPECTRUM", flags: RETRO8 | OWN_TEXT, make: |_| Box::new(Spectrum { zx: Zx::new() }) });
}

// ---------------------------------------------------------------- copper bars, logo, scroller, water

struct Copper {
    logo: Option<(String, Logo)>,
    scroll: Option<Scroller>,
    bars: usize,
}

impl Copper {
    fn new(seed: u32) -> Self {
        Copper { logo: None, scroll: None, bars: 6 + Rng::new(seed).below(5) as usize }
    }
}

impl Effect for Copper {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        if self.logo.as_ref().is_none_or(|(n, _)| n != c.name) {
            let scale = (330 / crate::font::text_width(c.name, 1, true).max(1)).clamp(3, 10);
            self.logo = Some((c.name.to_string(), Logo::new(c.name, scale)));
        }
        let scroll = self.scroll.get_or_insert_with(|| Scroller::new(c.text));
        scroll.advance(c.dt, 75.0 + a.pulse * 90.0, 3);

        let (top, bottom) = (scale(c.pal.bg, 0.6), mix(c.pal.bg, rgb(30, 0, 50), 0.5));
        for y in 0..HI {
            fb.row(y, mix(top, bottom, y as f32 / HI as f32));
        }
        // copper bars circling in depth, drawn back to front
        let n = self.bars;
        let mut bars: Vec<(f32, f32, u32)> = (0..n)
            .map(|i| {
                let ph = t * 1.6 + i as f32 * 3.6 / n as f32;
                (ph.cos(), 64.0 + ph.sin() * 50.0, c.pal.at(i as f32 / n as f32 + t * 0.05))
            })
            .collect();
        bars.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (z, y, col) in bars {
            fb.copper_bar(y, 6.0 + 4.0 * z + a.bass * 6.0, scale(col, 0.6 + 0.4 * (z + 1.0) / 2.0));
        }
        if let Some((_, logo)) = &self.logo {
            logo.draw(fb, WI / 2, 12 + ((t * 1.1).sin() * 4.0 - a.pulse * 4.0) as i32, t, 2.0 + a.bass * 8.0);
        }
        scroll.draw(fb, &ScrollStyle { y: 126.0, sc: 3, amp: 12.0 + a.mid * 6.0, freq: 0.035, speed: 3.0 }, t);
        // horizon line, then the water reflection below it
        for x in 0..WI {
            let col = c.pal.at(x as f32 / WI as f32 + t * 0.3);
            fb.put(x, 170, col);
            fb.put(x, 171, scale(col, 0.6));
        }
        let ym = 172;
        let water = mix(rgb(0, 20, 70), c.pal.bg, 0.3);
        for y in ym..HI {
            let d = (y - ym) as f32;
            let off = ((d * 0.35 - t * 5.0).sin() * (1.0 + d * 0.06)) as i32;
            let sy = ym - 3 - (d * 1.1) as i32;
            for x in 0..WI {
                let src = fb.get(x + off, sy);
                fb.put(x, y, mix(src, water, 0.45 + d / (HI - ym) as f32 * 0.35));
            }
        }
    }
}

// ---------------------------------------------------------------- shared starfield

struct Stars {
    s: Vec<V3>,
    rng: Rng,
}

impl Stars {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let s = (0..260).map(|_| [rng.range(-2.0, 2.0), rng.range(-1.2, 1.2), rng.range(0.2, 4.0)]).collect();
        Stars { s, rng }
    }
    fn draw(&mut self, fb: &mut Fb, speed: f32, dt: f32) {
        for st in self.s.iter_mut() {
            st[2] -= speed * dt;
            if st[2] < 0.15 {
                *st = [self.rng.range(-2.0, 2.0), self.rng.range(-1.2, 1.2), 4.0];
            }
            let (x, y) = (W as f32 / 2.0 + st[0] / st[2] * 110.0, H as f32 / 2.0 + st[1] / st[2] * 110.0);
            let b = (1.0 - st[2] / 4.0).clamp(0.0, 1.0);
            let col = rgbf(b, b, b * 1.1 + 0.1);
            let (x, y) = (x as i32, y as i32);
            fb.put(x, y, col);
            if st[2] < 1.2 {
                fb.put(x + 1, y, col);
                fb.put(x, y + 1, col);
                fb.put(x + 1, y + 1, col);
            }
        }
    }
}

// ---------------------------------------------------------------- glenz star-cube

struct Glenz {
    v: Vec<V3>,
    t: Vec<[usize; 3]>,
    rot: V3,
    stars: Stars,
}

/// A star-cube: every face of a cube gets a pyramid, giving 24 triangles.
fn glenz_model(spike: f32) -> (Vec<V3>, Vec<[usize; 3]>) {
    let mut v: Vec<V3> = (0..8)
        .map(|i| {
            let s = |b: usize| if i & b != 0 { 1.0 } else { -1.0 };
            [s(1), s(2), s(4)]
        })
        .collect();
    let idx = |p: V3| (p[0] > 0.0) as usize | ((p[1] > 0.0) as usize) << 1 | ((p[2] > 0.0) as usize) << 2;
    let mut t = Vec::new();
    for axis in 0..3 {
        for s in [-1.0f32, 1.0] {
            let mut apex = [0.0; 3];
            apex[axis] = s * spike;
            v.push(apex);
            let a = v.len() - 1;
            let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
            let quad = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            for k in 0..4 {
                let corner = |(u, w): (f32, f32)| {
                    let mut p = [0.0; 3];
                    p[axis] = s;
                    p[b] = u;
                    p[c] = w;
                    idx(p)
                };
                let (i0, i1) = (corner(quad[k]), corner(quad[(k + 1) % 4]));
                // wind every triangle so its normal points away from the centre
                let n = cross(sub(v[i0], v[a]), sub(v[i1], v[a]));
                let centroid = [
                    (v[a][0] + v[i0][0] + v[i1][0]) / 3.0,
                    (v[a][1] + v[i0][1] + v[i1][1]) / 3.0,
                    (v[a][2] + v[i0][2] + v[i1][2]) / 3.0,
                ];
                if dot(n, centroid) >= 0.0 { t.push([a, i0, i1]) } else { t.push([a, i1, i0]) }
            }
        }
    }
    (v, t)
}

impl Glenz {
    fn new(seed: u32) -> Self {
        let spike = [1.6, 1.3, 2.1][Rng::new(seed).below(3) as usize];
        let (v, t) = glenz_model(spike);
        Glenz { v, t, rot: [0.0; 3], stars: Stars::new(seed) }
    }
}

impl Effect for Glenz {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        fb.clear(scale(c.pal.bg, 0.5));
        self.stars.draw(fb, 0.6 + a.treble * 2.5 + a.pulse * 2.0, c.dt);
        fb.copper_bar(5.0, 5.0, c.pal.at(t * 0.1));
        fb.copper_bar(H as f32 - 6.0, 5.0, c.pal.at(t * 0.1 + 0.5));

        self.rot[0] += c.dt * (0.7 + a.mid * 1.5);
        self.rot[1] += c.dt * (0.9 + a.bass * 0.8);
        self.rot[2] += c.dt * 0.3;
        let s = 1.0 + a.pulse * 0.18 + a.bass * 0.1;
        let pos = [(t * 0.7).sin() * 1.1, (t * 1.3).sin() * 0.3, 5.2];
        let v: Vec<V3> = self
            .v
            .iter()
            .map(|&p| {
                let r = rot3(p, self.rot);
                [r[0] * s + pos[0], r[1] * s + pos[1], r[2] * s + pos[2]]
            })
            .collect();
        let light = norm([-0.3, -0.6, -0.75]);
        // glass colours: palette 0 gives the original blue and white
        let (dark, light_c) = if c.pal.at(0.0) == rainbow(0.0) {
            (rgb(40, 80, 230), rgb(200, 210, 255))
        } else {
            (c.pal.at(0.3), mix(c.pal.hi, rgb(255, 255, 255), 0.4))
        };
        for pass in 0..2 {
            for (k, tri) in self.t.iter().enumerate() {
                let (p0, p1, p2) = (v[tri[0]], v[tri[1]], v[tri[2]]);
                let n = norm(cross(sub(p1, p0), sub(p2, p0)));
                let centre = [(p0[0] + p1[0] + p2[0]) / 3.0, (p0[1] + p1[1] + p2[1]) / 3.0, (p0[2] + p1[2] + p2[2]) / 3.0];
                let front = dot(n, centre) < 0.0;
                if front != (pass == 1) {
                    continue;
                }
                let base = if k % 2 == 0 { dark } else { light_c };
                let (col, alpha) = if front {
                    (scale(base, 0.35 + 0.75 * dot(n, light).max(0.0)), 0.6)
                } else {
                    (scale(base, 0.5), 0.35)
                };
                fb.tri([project(p0, FOV), project(p1, FOV), project(p2, FOV)], col, alpha);
            }
        }
    }
}

// ---------------------------------------------------------------- vector balls over a chessboard

struct Balls {
    shapes: Vec<Vec<V3>>,
    from: usize,
    to: usize,
    morph_t0: f32,
    rot: V3,
    floor_z: f32,
}

fn ball_shapes() -> Vec<Vec<V3>> {
    let mut shapes = Vec::new();
    let mut cube = Vec::new();
    for i in 0..64 {
        let f = |n: usize| (n as f32 - 1.5) * 0.62;
        cube.push([f(i % 4), f((i / 4) % 4), f(i / 16)]);
    }
    shapes.push(cube);
    let golden = PI * (3.0 - 5f32.sqrt());
    shapes.push(
        (0..64)
            .map(|i| {
                let y = 1.0 - (i as f32 + 0.5) / 64.0 * 2.0;
                let r = (1.0 - y * y).sqrt();
                let a = golden * i as f32;
                [a.cos() * r * 1.35, y * 1.35, a.sin() * r * 1.35]
            })
            .collect(),
    );
    shapes.push(
        (0..64)
            .map(|i| {
                let (a, b) = ((i % 16) as f32 / 16.0 * TAU, (i / 16) as f32 / 4.0 * TAU);
                let r = 1.1 + 0.45 * b.cos();
                [a.cos() * r, 0.45 * b.sin(), a.sin() * r]
            })
            .collect(),
    );
    let mut ufo = Vec::new();
    let ring = |n: usize, r: f32, y: f32, out: &mut Vec<V3>| {
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU;
            out.push([a.cos() * r, y, a.sin() * r]);
        }
    };
    ring(24, 1.6, 0.0, &mut ufo);
    ring(16, 1.05, -0.28, &mut ufo);
    ring(12, 0.55, -0.62, &mut ufo);
    ufo.push([0.0, -0.95, 0.0]);
    ring(11, 0.8, 0.3, &mut ufo);
    shapes.push(ufo);
    shapes.push(
        (0..64)
            .map(|i| {
                let k = (i / 2) as f32 / 31.0;
                let a = k * TAU * 1.5 + if i % 2 == 0 { 0.0 } else { PI };
                [a.cos() * 0.85, (k - 0.5) * 2.8, a.sin() * 0.85]
            })
            .collect(),
    );
    shapes
}

impl Balls {
    fn new(seed: u32) -> Self {
        let first = Rng::new(seed).below(5) as usize;
        Balls { shapes: ball_shapes(), from: first, to: first, morph_t0: 0.0, rot: [0.0; 3], floor_z: 0.0 }
    }
}

impl Effect for Balls {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        let horizon = 92;
        let original = c.pal.at(0.0) == rainbow(0.0);
        let (sky_top, sky_low) =
            if original { (rgb(8, 0, 40), rgb(230, 90, 130)) } else { (scale(c.pal.bg, 0.8), c.pal.at(0.9)) };
        for y in 0..horizon {
            let f = (y as f32 / horizon as f32).powf(1.5);
            fb.row(y, mix(sky_top, sky_low, f));
        }
        // striped retro sun sinking into the horizon
        let (scx, r) = (W as f32 / 2.0, 46.0 + a.bass * 4.0);
        let (sun_a, sun_b) = if original { (rgb(255, 240, 90), rgb(255, 60, 140)) } else { (c.pal.hi, c.pal.at(0.7)) };
        for y in (horizon - r as i32).max(0)..horizon {
            let dy = horizon - y;
            if dy < 24 && dy % 6 < 2 + (24 - dy) / 8 {
                continue;
            }
            let half = (r * r - (dy as f32) * (dy as f32)).max(0.0).sqrt();
            fb.hspan((scx - half) as i32, (scx + half) as i32, y, mix(sun_a, sun_b, 1.0 - dy as f32 / r));
        }
        self.floor_z += c.dt * (1.5 + a.bass * 5.0);
        let (ca, cb) = if original { (rgb(230, 230, 255), rgb(20, 20, 90)) } else { (mix(c.pal.hi, rgb(255, 255, 255), 0.5), scale(c.pal.at(0.4), 0.4)) };
        for y in horizon..HI {
            let dy = (y - horizon) as f32 + 1.0;
            let z = 60.0 / dy;
            let v = z + self.floor_z;
            let fog = (dy / (HI - horizon) as f32).sqrt();
            let row = y as usize * W;
            for x in 0..W {
                let u = (x as f32 - W as f32 / 2.0) * z / 40.0 + (t * 0.4).sin() * 2.0;
                let check = (u.floor() as i32 + v.floor() as i32) & 1 == 0;
                fb.px[row + x] = mix(sky_low, if check { ca } else { cb }, fog);
            }
        }

        // morph to the next shape every 8 beats
        if a.beat && c.beats > 0 && c.beats % 8 == 0 {
            self.from = self.to;
            self.to = (self.to + 1) % self.shapes.len();
            self.morph_t0 = c.t;
        }
        let m = smoothstep((c.t - self.morph_t0) / 1.2);
        self.rot[0] = (t * 0.4).sin() * 0.6;
        self.rot[1] += c.dt * (0.8 + a.mid);
        let mut balls: Vec<(f32, f32, f32, f32, usize)> = (0..64)
            .map(|i| {
                let (p, q) = (self.shapes[self.from][i], self.shapes[self.to][i]);
                let p = [p[0] + (q[0] - p[0]) * m, p[1] + (q[1] - p[1]) * m, p[2] + (q[2] - p[2]) * m];
                let r = rot3(p, self.rot);
                let w = [r[0], r[1] - 0.25 + (t * 1.3).sin() * 0.15, r[2] + 5.0];
                let (sx, sy) = project(w, FOV);
                (w[2], sx, sy, 0.21 * FOV / w[2] * (1.0 + a.pulse * 0.25), i)
            })
            .collect();
        balls.sort_by(|x, y| y.0.total_cmp(&x.0));
        for (_, x, y, r, i) in balls {
            fb.ball(x, y, r, unpack(c.pal.at(i as f32 / 64.0 + t * 0.05)));
        }
    }
}

// ---------------------------------------------------------------- twister over plasma

struct Twister {
    sides: usize,
}

impl Twister {
    fn new(seed: u32) -> Self {
        Twister { sides: [4, 4, 3, 5][Rng::new(seed).below(4) as usize] }
    }
}

impl Effect for Twister {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        let original = c.pal.at(0.0) == rainbow(0.0);
        // chunky half-resolution plasma behind
        chunky(fb, 2, |px, py| {
            let (fx, fy) = ((px / 2) as f32, (py / 2) as f32);
            let d = ((fx - 96.0 - (t * 0.7).sin() * 40.0).powi(2) + (fy - 54.0).powi(2)).sqrt();
            let v = (fx * 0.06 + t).sin() + (fy * 0.08 - t * 1.3).sin() + ((fx + fy) * 0.04 + t * 0.7).sin() + (d * 0.09 - t * 2.0).sin();
            let col = if original { pal(v * 0.11 + t * 0.08, (0.0, 0.25, 0.55)) } else { c.pal.at(v * 0.11 + t * 0.08) };
            scale(col, 0.4 + a.mid * 0.2)
        });
        // the twister: a column, each scanline rotated a little more
        let n = self.sides;
        let faces: Vec<u32> = if original && n == 4 {
            vec![rgb(255, 90, 90), rgb(90, 255, 130), rgb(90, 150, 255), rgb(255, 220, 70)]
        } else {
            (0..n).map(|i| mix(c.pal.at(i as f32 / n as f32 + 0.1), rgb(255, 255, 255), 0.2)).collect()
        };
        let step = TAU / n as f32;
        let r = 30.0 + a.bass * 10.0;
        for y in 0..HI {
            let fy = y as f32;
            let cx = W as f32 / 2.0 + (fy * 0.015 + t * 0.8).sin() * 40.0;
            let ang = t * 1.2 + (t * 0.5 + fy * 0.012).sin() * (2.0 + a.mid * 1.5);
            let stripe = ((y + (t * 60.0) as i32) / 6) % 2 == 0;
            for (i, face) in faces.iter().enumerate() {
                let xa = cx + r * (ang + i as f32 * step).sin();
                let xb = cx + r * (ang + (i + 1) as f32 * step).sin();
                if xb <= xa {
                    continue;
                }
                let shade = 0.25 + 0.75 * (xb - xa) / (2.0 * r);
                let base = scale(*face, if stripe { shade * 1.1 } else { shade * 0.85 });
                for x in xa.ceil() as i32..xb.ceil() as i32 {
                    let s = (x as f32 - xa) / (xb - xa);
                    fb.put(x, y, scale(base, 0.7 + 0.3 * (s * PI).sin()));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- texture tunnel

const TW: usize = W * 2;
const TH: usize = H * 2;

struct Tunnel {
    u: Vec<u8>,
    v: Vec<u8>,
    fog: Vec<u8>,
    tex: Vec<u32>,
    tex_for: u32,
    depth: f32,
    spin: f32,
    dir: f32,
}

impl Tunnel {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let twist = [0.0, 0.0, 6.0, -10.0][rng.below(4) as usize];
        let mut u = vec![0; TW * TH];
        let mut v = vec![0; TW * TH];
        let mut fog = vec![0; TW * TH];
        for ty in 0..TH {
            for tx in 0..TW {
                let dx = tx as f32 - W as f32 + 0.5;
                let dy = ty as f32 - H as f32 + 0.5;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let i = ty * TW + tx;
                let depth = 4000.0 / dist;
                u[i] = ((((dy.atan2(dx) / TAU + 0.5) * 512.0) + depth * twist / 64.0) as i32 & 255) as u8;
                v[i] = (depth as u32 & 255) as u8;
                fog[i] = ((dist / (H as f32 * 0.75)).min(1.0) * 255.0) as u8;
            }
        }
        Tunnel { u, v, fog, tex: Vec::new(), tex_for: 0, depth: 0.0, spin: 0.0, dir: if rng.below(2) == 0 { 1.0 } else { -1.0 } }
    }
}

impl Effect for Tunnel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let t = c.time;
        // rebuild the texture when the palette changes
        let key = c.pal.at(0.0) ^ c.pal.at(0.5).rotate_left(7);
        if self.tex.is_empty() || key != self.tex_for {
            self.tex = if c.pal.at(0.0) == rainbow(0.0) {
                let mut tex = vec![0; 256 * 256];
                for y in 0..256 {
                    for x in 0..256 {
                        let v = (x ^ y) as f32 / 255.0;
                        let mut col = pal(v * 0.6 + 0.1, (0.0, 0.15, 0.35));
                        if x % 32 < 2 || y % 32 < 2 {
                            col = mix(col, rgb(255, 255, 255), 0.6);
                        }
                        tex[y * 256 + x] = col;
                    }
                }
                tex
            } else {
                xor_texture(c.pal)
            };
            self.tex_for = key;
        }
        self.depth += c.dt * (60.0 + c.a.bass * 160.0);
        self.spin += c.dt * (20.0 + c.a.mid * 30.0) * self.dir;
        let ox = (W as f32 / 2.0 + (t * 0.5).sin() * W as f32 * 0.4) as usize;
        let oy = (H as f32 / 2.0 + (t * 0.37).cos() * H as f32 * 0.4) as usize;
        let (du, dv) = (self.spin as i32 as u32, self.depth as u32);
        let bright = 0.8 + c.a.pulse * 0.6;
        for y in 0..H {
            let row = (y + oy) * TW + ox;
            for x in 0..W {
                let i = row + x;
                let u = (self.u[i] as u32).wrapping_add(du) & 255;
                let v = (self.v[i] as u32).wrapping_add(dv) & 255;
                fb.px[y * W + x] = scale(self.tex[(v * 256 + u) as usize], self.fog[i] as f32 / 255.0 * bright);
            }
        }
    }
}

// ---------------------------------------------------------------- Kefrens bars

struct Kefrens {
    row: Vec<u32>,
    width: i32,
}

impl Kefrens {
    fn new(seed: u32) -> Self {
        Kefrens { row: vec![0; W], width: [20, 14, 28][Rng::new(seed).below(3) as usize] }
    }
}

impl Effect for Kefrens {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        let top = 44;
        let bw = self.width;
        self.row.fill(rgb(0, 0, 0));
        for y in 0..HI {
            if y < top {
                fb.row(y, mix(mix(rgb(0, 0, 30), c.pal.bg, 0.5), rgb(0, 0, 0), y as f32 / top as f32));
                continue;
            }
            let fy = y as f32;
            let x = W as f32 / 2.0 + (t * 1.7 + fy * 0.035).sin() * (80.0 + a.bass * 30.0) + (t * 1.1 - fy * 0.021).sin() * 50.0
                - bw as f32 / 2.0;
            let col = c.pal.at(fy / HI as f32 * 0.8 + t * 0.2);
            for i in 0..bw {
                let px = x as i32 + i;
                if !(0..WI).contains(&px) {
                    continue;
                }
                let k = ((i as f32 + 0.5) / bw as f32 * PI).sin();
                self.row[px as usize] = add(scale(col, 0.15 + 0.85 * k), scale(rgb(255, 255, 255), k.powi(12) * 0.7));
            }
            let row = y as usize * W;
            fb.px[row..row + W].copy_from_slice(&self.row);
        }
        fb.copper_bar(top as f32 - 2.0, 2.5, c.pal.at(t * 0.2));
    }
}

// ---------------------------------------------------------------- the Spectrum-style room

struct Spectrum {
    zx: Zx,
}

impl Effect for Spectrum {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        self.zx.update(c.time, c.dt, c.a);
        self.zx.draw(fb, c.time, c.a);
    }
}
