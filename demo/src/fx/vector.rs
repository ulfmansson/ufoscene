//! Effects: vector — 3D objects, dots and particles. Starfields, morphing dot
//! clouds, shaded solids, glowing wireframes, a jelly cube, a cube field, a dot
//! tunnel, vector bobs, a tiny raytracer, an FFT city, a particle fountain,
//! Lissajous ribbons and orbiting gems.
//!
//! Everything is drawn with plain software rasterising: rotation matrices,
//! perspective division, painter's-algorithm z-sorting and flat or Gouraud
//! shaded triangles, the way it was done before anyone had a GPU.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::gfx::*;

pub fn register(v: &mut Vec<Def>) {
    let both = LOGO_TOP | SCROLL_BOTTOM;
    v.push(Def { name: "STARFIELD", flags: VECTOR | both, make: |s| Box::new(Starfield::new(s)) });
    v.push(Def { name: "DOT MORPH", flags: VECTOR | both, make: |s| Box::new(DotMorph::new(s)) });
    v.push(Def { name: "TORUS", flags: VECTOR | both, make: |s| Box::new(Torus::new(s)) });
    v.push(Def { name: "WIREFRAME", flags: VECTOR | LOGO_TOP, make: |s| Box::new(Wireframe::new(s)) });
    v.push(Def { name: "RUBBER CUBE", flags: VECTOR | both, make: |s| Box::new(RubberCube::new(s)) });
    v.push(Def { name: "CUBEFIELD", flags: VECTOR, make: |s| Box::new(Cubefield::new(s)) });
    v.push(Def { name: "DOT TUNNEL", flags: VECTOR, make: |s| Box::new(DotTunnel::new(s)) });
    v.push(Def { name: "BOB TRAILS", flags: VECTOR | both, make: |s| Box::new(BobTrails::new(s)) });
    v.push(Def { name: "RAYTRACER", flags: VECTOR | LOGO_TOP, make: |s| Box::new(Raytracer::new(s)) });
    v.push(Def { name: "EQ CITY", flags: VECTOR | LOGO_TOP, make: |s| Box::new(EqCity::new(s)) });
    v.push(Def { name: "FOUNTAIN", flags: VECTOR, make: |s| Box::new(Fountain::new(s)) });
    v.push(Def { name: "LISSAJOUS", flags: VECTOR | both, make: |s| Box::new(Lissajous::new(s)) });
    v.push(Def { name: "GEMS", flags: VECTOR | both, make: |s| Box::new(Gems::new(s)) });
}

// ---------------------------------------------------------------- shared helpers

type M3 = [[f32; 3]; 3];

/// Rotation matrix doing the same as `rot3(_, a)`, so points cost 9 multiplies.
fn rot_m(a: V3) -> M3 {
    let x = rot3([1.0, 0.0, 0.0], a);
    let y = rot3([0.0, 1.0, 0.0], a);
    let z = rot3([0.0, 0.0, 1.0], a);
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

#[inline]
fn mv(m: &M3, p: V3) -> V3 {
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2],
        m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2],
    ]
}

#[inline]
fn addv(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn mulv(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Saturating per-channel add of two pixels.
#[inline]
fn sat_add(a: u32, b: u32) -> u32 {
    let r = ((a & 0xFF) + (b & 0xFF)).min(255);
    let g = (((a >> 8) & 0xFF) + ((b >> 8) & 0xFF)).min(255);
    let bl = (((a >> 16) & 0xFF) + ((b >> 16) & 0xFF)).min(255);
    0xFF00_0000 | (bl << 16) | (g << 8) | r
}

/// Integer blend from `a` to `b` by `k / 256`.
#[inline]
fn mix_fast(a: u32, b: u32, k: u32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = ((a >> s) & 0xFF, (b >> s) & 0xFF);
        ((x * (256 - k) + y * k) >> 8) << s
    };
    0xFF00_0000 | ch(0) | ch(8) | ch(16)
}

/// Multiply a pixel by `k / 256`.
#[inline]
fn dim(c: u32, k: u32) -> u32 {
    let r = ((c & 0xFF) * k) >> 8;
    let g = (((c >> 8) & 0xFF) * k) >> 8;
    let b = (((c >> 16) & 0xFF) * k) >> 8;
    0xFF00_0000 | (b << 16) | (g << 8) | r
}

#[inline]
fn add_px(fb: &mut Fb, x: i32, y: i32, c: u32) {
    if x >= 0 && y >= 0 && x < WI && y < HI {
        let i = y as usize * W + x as usize;
        fb.px[i] = sat_add(fb.px[i], c);
    }
}

/// Additive square dot of size 1..3 pixels.
#[inline]
fn dot_add(fb: &mut Fb, x: f32, y: f32, size: i32, c: u32) {
    let (x0, y0) = (x as i32 - size / 2, y as i32 - size / 2);
    for dy in 0..size {
        for dx in 0..size {
            add_px(fb, x0 + dx, y0 + dy, c);
        }
    }
}

/// Sky over floor: flat `sky`, brightening into `glow` over the last rows above
/// row `hz`, then quickly dropping to a flat `floor` below it.
fn horizon(fb: &mut Fb, sky: u32, glow: u32, floor: u32, hz: f32) {
    for y in 0..H {
        let d = y as f32 - hz;
        let col = if d < 0.0 { mix(sky, glow, smoothstep(1.0 + d / 30.0)) } else { mix(glow, floor, (d / 8.0).min(1.0)) };
        fb.px[y * W..(y + 1) * W].fill(col);
    }
}

/// Flat background with slowly drifting dim specks for a sense of depth.
fn dusty(fb: &mut Fb, c: &Ctx, bg: u32) {
    fb.clear(bg);
    for k in 0..90u32 {
        let h = k.wrapping_mul(2_654_435_761) ^ 0x5bd1_e995;
        let depth = 0.3 + (h >> 24) as f32 / 255.0 * 0.7;
        let x = fract((h & 0xFFFF) as f32 / 65536.0 + c.time * 0.012 * depth) * W as f32;
        let y = ((h >> 12) & 0xFFF) as f32 / 4096.0 * H as f32;
        add_px(fb, x as i32, y as i32, dim(c.pal.hi, (depth * 110.0) as u32));
    }
}

/// A bright, saturated palette colour near `x` (skips the palette's dark stretch).
fn vivid(pal: &Palette, x: f32) -> u32 {
    let peak = |c: u32| {
        let (r, g, b) = unpack(c);
        r.max(g).max(b)
    };
    let mut c = pal.at(x);
    if peak(c) < 0.35 {
        c = pal.at(x + 0.5);
    }
    scale(c, 0.95 / peak(c).max(0.2))
}

/// Additive line whose colour fades from `c0` to `c1` along its length.
fn line_add(fb: &mut Fb, a: (f32, f32), b: (f32, f32), c0: u32, c1: u32) {
    let (w, h) = (W as f32, H as f32);
    if (a.0 < 0.0 && b.0 < 0.0) || (a.1 < 0.0 && b.1 < 0.0) || (a.0 >= w && b.0 >= w) || (a.1 >= h && b.1 >= h) {
        return;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let n = dx.abs().max(dy.abs()).ceil().clamp(1.0, 800.0) as i32;
    let (sx, sy) = (dx / n as f32, dy / n as f32);
    let ch = |c: u32, s: u32| (((c >> s) & 0xFF) as i32) << 16;
    let (mut r, mut g, mut bl) = (ch(c0, 0), ch(c0, 8), ch(c0, 16));
    let (dr, dg, db) = ((ch(c1, 0) - r) / n, (ch(c1, 8) - g) / n, (ch(c1, 16) - bl) / n);
    let (mut x, mut y) = a;
    for _ in 0..=n {
        let c = 0xFF00_0000 | (((bl >> 16) as u32) << 16) | (((g >> 16) as u32) << 8) | (r >> 16) as u32;
        add_px(fb, x.floor() as i32, y.floor() as i32, c);
        x += sx;
        y += sy;
        r += dr;
        g += dg;
        bl += db;
    }
}

/// A line with a soft halo: a bright core plus four dimmer offset copies.
fn glow_line(fb: &mut Fb, a: (f32, f32), b: (f32, f32), c0: u32, c1: u32) {
    let (h0, h1) = (dim(c0, 70), dim(c1, 70));
    for (ox, oy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        line_add(fb, (a.0 + ox, a.1 + oy), (b.0 + ox, b.1 + oy), h0, h1);
    }
    line_add(fb, a, b, c0, c1);
}

/// Gouraud triangle: each corner is (x, y, shade 0..255) and colours come from `lut`.
fn tri_lut(fb: &mut Fb, mut p: [(f32, f32, f32); 3], lut: &[u32; 256]) {
    p.sort_by(|a, b| a.1.total_cmp(&b.1));
    let [a, b, c] = p;
    if c.1 - a.1 < 1e-4 {
        return;
    }
    let ys = (a.1.ceil() as i32).max(0);
    let ye = (c.1.ceil() as i32).min(HI);
    for y in ys..ye {
        let fy = y as f32;
        let t = (fy - a.1) / (c.1 - a.1);
        let (xl, sl) = (a.0 + (c.0 - a.0) * t, a.2 + (c.2 - a.2) * t);
        let (xr, sr) = if fy < b.1 {
            let t = (fy - a.1) / (b.1 - a.1).max(1e-4);
            (a.0 + (b.0 - a.0) * t, a.2 + (b.2 - a.2) * t)
        } else {
            let t = (fy - b.1) / (c.1 - b.1).max(1e-4);
            (b.0 + (c.0 - b.0) * t, b.2 + (c.2 - b.2) * t)
        };
        let (x0, s0, x1, s1) = if xl < xr { (xl, sl, xr, sr) } else { (xr, sr, xl, sl) };
        let (xs, xe) = ((x0.ceil() as i32).max(0), (x1.ceil() as i32).min(WI));
        if xs >= xe {
            continue;
        }
        let ds = (s1 - s0) / (x1 - x0).max(1e-4);
        let mut s = s0 + (xs as f32 - x0) * ds;
        let row = y as usize * W;
        for px in &mut fb.px[row + xs as usize..row + xe as usize] {
            *px = lut[s.clamp(0.0, 255.0) as usize];
            s += ds;
        }
    }
}

/// Flat convex polygon (fan of triangles).
fn poly(fb: &mut Fb, p: &[(f32, f32)], c: u32) {
    for i in 1..p.len().saturating_sub(1) {
        fb.tri([p[0], p[i], p[i + 1]], c, 1.0);
    }
}

/// Perspective projection around an arbitrary screen centre.
#[inline]
fn proj_at(p: V3, fov: f32, cx: f32, cy: f32) -> (f32, f32) {
    let z = p[2].max(0.05);
    (cx + p[0] / z * fov, cy + p[1] / z * fov)
}

/// Screen row of the horizon for a camera looking along `fwd`.
fn horizon_y(fwd: V3, up: V3, fov: f32) -> f32 {
    let h = norm([fwd[0], 0.0, fwd[2]]);
    H as f32 / 2.0 - dot(h, up) / dot(h, fwd).max(0.05) * fov
}

/// Lambert + Blinn-ish highlight for a unit normal in camera space (light from upper left front).
#[inline]
fn light(n: V3, l: V3) -> (f32, f32) {
    let d = dot(n, l).max(0.0);
    let h = norm(addv(l, [0.0, 0.0, -1.0]));
    let s = dot(n, h).max(0.0).powi(24);
    (d, s)
}

/// A convex polyhedron built from its corner points: faces are found as a convex hull.
struct Solid {
    v: Vec<V3>,
    faces: Vec<Vec<usize>>,
    normals: Vec<V3>,
    edges: Vec<(usize, usize)>,
}

const PHI: f32 = 1.618_034;
const CYC: [[usize; 3]; 3] = [[0, 1, 2], [1, 2, 0], [2, 0, 1]];
const ALL: [[usize; 3]; 6] = [[0, 1, 2], [1, 2, 0], [2, 0, 1], [0, 2, 1], [2, 1, 0], [1, 0, 2]];

/// Every permutation (from `perms`) and sign combination of `base`, without duplicates.
fn expand(out: &mut Vec<V3>, base: V3, perms: &[[usize; 3]]) {
    for p in perms {
        for s in 0..8 {
            let sg = |i: usize| if s & (1 << i) != 0 { -1.0 } else { 1.0 };
            let q = [base[p[0]] * sg(0), base[p[1]] * sg(1), base[p[2]] * sg(2)];
            if !out.iter().any(|o| dot(sub(*o, q), sub(*o, q)) < 1e-6) {
                out.push(q);
            }
        }
    }
}

/// Corner points of solid `k` (0..7): tetrahedron, cube, octahedron, icosahedron,
/// dodecahedron, cuboctahedron, truncated octahedron.
fn solid_points(k: usize) -> Vec<V3> {
    let mut v = Vec::new();
    match k % 7 {
        0 => v.extend([[1.0, 1.0, 1.0], [1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, 1.0]]),
        1 => expand(&mut v, [1.0, 1.0, 1.0], &CYC),
        2 => expand(&mut v, [1.0, 0.0, 0.0], &CYC),
        3 => expand(&mut v, [0.0, 1.0, PHI], &CYC),
        4 => {
            expand(&mut v, [1.0, 1.0, 1.0], &CYC);
            expand(&mut v, [0.0, 1.0 / PHI, PHI], &CYC);
        }
        5 => expand(&mut v, [1.0, 1.0, 0.0], &CYC),
        _ => expand(&mut v, [0.0, 1.0, 2.0], &ALL),
    }
    v
}

impl Solid {
    fn new(k: usize) -> Solid {
        let pts = solid_points(k);
        let r = pts.iter().map(|p| dot(*p, *p).sqrt()).fold(0.0, f32::max);
        let v: Vec<V3> = pts.iter().map(|p| mulv(*p, 1.0 / r)).collect();
        let n = v.len();
        let mut faces: Vec<Vec<usize>> = Vec::new();
        let mut normals: Vec<V3> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let c = cross(sub(v[j], v[i]), sub(v[k], v[i]));
                    if dot(c, c) < 1e-8 {
                        continue;
                    }
                    let mut nn = norm(c);
                    let d0 = dot(nn, v[i]);
                    let (mut pos, mut neg) = (false, false);
                    for p in &v {
                        let s = dot(nn, *p) - d0;
                        pos |= s > 1e-3;
                        neg |= s < -1e-3;
                    }
                    if pos && neg {
                        continue;
                    }
                    if pos {
                        nn = mulv(nn, -1.0);
                    }
                    if normals.iter().any(|m| dot(*m, nn) > 0.9999) {
                        continue;
                    }
                    let d = dot(nn, v[i]);
                    let mut idx: Vec<usize> = (0..n).filter(|&q| (dot(nn, v[q]) - d).abs() < 1e-3).collect();
                    let mut cen = [0.0; 3];
                    for &q in &idx {
                        cen = addv(cen, mulv(v[q], 1.0 / idx.len() as f32));
                    }
                    let u = norm(sub(v[idx[0]], cen));
                    let w = cross(nn, u);
                    let ang = |q: usize| {
                        let e = sub(v[q], cen);
                        dot(e, w).atan2(dot(e, u))
                    };
                    idx.sort_by(|&a, &b| ang(a).total_cmp(&ang(b)));
                    faces.push(idx);
                    normals.push(nn);
                }
            }
        }
        let mut edges = Vec::new();
        for f in &faces {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                let e = (a.min(b), a.max(b));
                if !edges.contains(&e) {
                    edges.push(e);
                }
            }
        }
        Solid { v, faces, normals, edges }
    }

    /// Flat-shade the solid: rotated by `m`, scaled by `s`, placed at `pos` (camera space).
    /// `shade(face index, camera-space normal)` gives each visible face's colour.
    fn draw(&self, fb: &mut Fb, m: &M3, s: f32, pos: V3, fov: f32, mut shade: impl FnMut(usize, V3) -> u32) {
        let mut sp = [(0.0f32, 0.0f32); 32];
        let mut cam = [[0.0f32; 3]; 32];
        for (i, p) in self.v.iter().enumerate().take(32) {
            cam[i] = addv(mulv(mv(m, *p), s), pos);
            sp[i] = project(cam[i], fov);
        }
        let mut pts = [(0.0f32, 0.0f32); 8];
        for (fi, f) in self.faces.iter().enumerate() {
            let n = mv(m, self.normals[fi]);
            if dot(n, cam[f[0]]) >= 0.0 {
                continue;
            }
            let c = shade(fi, n);
            let k = f.len().min(8);
            for (j, &q) in f.iter().take(k).enumerate() {
                pts[j] = sp[q];
            }
            poly(fb, &pts[..k], c);
        }
    }
}

/// A colour scaled by a light level plus a white highlight.
#[inline]
fn lit(c: u32, d: f32, h: f32) -> u32 {
    let (r, g, b) = unpack(c);
    rgbf(r * d + h, g * d + h, b * d + h)
}

// ---------------------------------------------------------------- 1. STARFIELD

/// Warp-speed starfield: stars fly at the viewer leaving streaks; bass sets the
/// speed and beats kick in a warp jump.
struct Starfield {
    stars: Vec<(V3, f32)>,
    rng: Rng,
    warp: f32,
    roll: f32,
    roll_dir: f32,
    colourful: bool,
    wobble: f32,
}

const STAR_DEPTH: f32 = 24.0;

impl Starfield {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let v = rng.below(4);
        let stars = (0..1400).map(|_| ([rng.range(-12.0, 12.0), rng.range(-12.0, 12.0), rng.range(0.3, STAR_DEPTH)], rng.f())).collect();
        Starfield {
            stars,
            rng,
            warp: 0.0,
            roll: 0.0,
            roll_dir: if v & 1 == 0 { 1.0 } else { -1.0 },
            colourful: v >= 2,
            wobble: if v == 1 || v == 2 { 1.0 } else { 0.0 },
        }
    }
}

impl Effect for Starfield {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.warp = if c.beats % 4 == 0 { 1.6 } else { 1.0 };
        }
        self.warp *= (-c.dt * 3.0).exp();
        let speed = 5.0 + a.bass * 10.0 + self.warp * self.warp * 30.0;
        self.roll += c.dt * self.roll_dir * (0.15 + a.mid * 0.6 + self.warp * 0.8);
        let (sr, cr) = self.roll.sin_cos();
        let (cx, cy) = (
            W as f32 / 2.0 + (c.t * 0.7).sin() * 40.0 * self.wobble,
            H as f32 / 2.0 + (c.t * 0.53).cos() * 25.0 * self.wobble,
        );
        fb.clear(mix(pal_bg(c), c.pal.at(0.6), self.warp * 0.12 + a.pulse * 0.05));
        let fov = 150.0;
        let streak = 0.012 + self.warp * 0.03;
        for (p, hue) in self.stars.iter_mut() {
            p[2] -= speed * c.dt;
            if p[2] < 0.3 {
                *p = [self.rng.range(-12.0, 12.0), self.rng.range(-12.0, 12.0), p[2] + STAR_DEPTH];
                *hue = self.rng.f();
            }
            let (x, y) = (p[0] * cr - p[1] * sr, p[0] * sr + p[1] * cr);
            let z_tail = p[2] + speed * streak;
            let head = (cx + x / p[2] * fov, cy + y / p[2] * fov);
            let tail = (cx + x / z_tail * fov, cy + y / z_tail * fov);
            let b = (1.0 - p[2] / STAR_DEPTH).powi(2);
            let col = if self.colourful {
                scale(c.pal.at(*hue), 0.3 + 1.2 * b)
            } else {
                c.pal.ramp(0.15 + 0.85 * b)
            };
            line_add(fb, tail, head, dim(col, 60), col);
            if b > 0.5 {
                dot_add(fb, head.0, head.1, 2, dim(col, 160));
            }
        }
    }
}

/// The palette's background colour, lifted just enough to stay visible after 12-bit reduction.
fn pal_bg(c: &Ctx) -> u32 {
    mix(c.pal.bg, c.pal.at(0.2), 0.08)
}

// ---------------------------------------------------------------- 2. DOT MORPH

/// Thousands of dots flowing between a sphere, torus, cube, knot, spring and wave,
/// a new shape every few beats; shaded by depth, rotation follows the mids.
struct DotMorph {
    shapes: Vec<Vec<V3>>,
    from: usize,
    to: usize,
    m: f32,
    count: u32,
    ang: V3,
    hue_mode: bool,
    every: u32,
}

const DOTS: usize = 2400;

fn dot_shapes() -> Vec<Vec<V3>> {
    let n = DOTS;
    let golden = PI * (3.0 - 5f32.sqrt());
    let sphere = (0..n)
        .map(|i| {
            let y = 1.0 - (i as f32 + 0.5) / n as f32 * 2.0;
            let r = (1.0 - y * y).sqrt();
            let a = golden * i as f32;
            [a.cos() * r, y, a.sin() * r]
        })
        .collect();
    let torus = (0..n)
        .map(|i| {
            let (u, v) = ((i % 80) as f32 / 80.0 * TAU, (i / 80) as f32 / 30.0 * TAU);
            let r = 0.78 + 0.3 * v.cos();
            [u.cos() * r, 0.3 * v.sin(), u.sin() * r]
        })
        .collect();
    let cube = (0..n)
        .map(|i| {
            let face = i / 400;
            let (a, b) = (((i % 20) as f32 / 19.0) * 2.0 - 1.0, ((i / 20 % 20) as f32 / 19.0) * 2.0 - 1.0);
            let s = if face & 1 == 0 { 0.72 } else { -0.72 };
            let (a, b) = (a * 0.72, b * 0.72);
            match face / 2 {
                0 => [s, a, b],
                1 => [a, s, b],
                _ => [a, b, s],
            }
        })
        .collect();
    let knot = (0..n)
        .map(|i| {
            let u = (i / 8) as f32 / (n / 8) as f32 * TAU;
            let v = (i % 8) as f32 / 8.0 * TAU;
            let c = |u: f32| [(2.0 + (3.0 * u).cos()) * (2.0 * u).cos(), (2.0 + (3.0 * u).cos()) * (2.0 * u).sin(), (3.0 * u).sin()];
            let p = c(u);
            let t = norm(sub(c(u + 0.01), p));
            let nn = norm(cross(t, [0.0, 0.0, 1.0]));
            let bn = cross(t, nn);
            let q = addv(p, addv(mulv(nn, 0.35 * v.cos()), mulv(bn, 0.35 * v.sin())));
            mulv(q, 0.32)
        })
        .collect();
    let spring = (0..n)
        .map(|i| {
            let u = i as f32 / n as f32;
            let a = u * TAU * 7.0;
            let v = (i % 6) as f32 / 6.0 * TAU;
            let r = 0.8 + 0.12 * v.cos();
            [a.cos() * r, (u - 0.5) * 1.8 + 0.12 * v.sin(), a.sin() * r]
        })
        .collect();
    let wave = (0..n)
        .map(|i| {
            let (x, z) = ((i % 48) as f32 / 47.0 * 2.0 - 1.0, (i / 48) as f32 / 49.0 * 2.0 - 1.0);
            let d = (x * x + z * z).sqrt();
            [x * 1.1, (d * 9.0).cos() * 0.25 * (1.0 - d * 0.5), z * 1.1]
        })
        .collect();
    vec![sphere, torus, cube, knot, spring, wave]
}

impl DotMorph {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut shapes = dot_shapes();
        // shuffle the order so every demo morphs through a different sequence
        for i in (1..shapes.len()).rev() {
            shapes.swap(i, rng.below(i as u32 + 1) as usize);
        }
        DotMorph {
            shapes,
            from: 0,
            to: 0,
            m: 1.0,
            count: 0,
            ang: [rng.f(), rng.f(), 0.0],
            hue_mode: rng.below(2) == 0,
            every: 2 + rng.below(2) * 2,
        }
    }
}

impl Effect for DotMorph {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.count += 1;
            if self.count % self.every == 0 && self.m >= 1.0 {
                self.from = self.to;
                self.to = (self.to + 1) % self.shapes.len();
                self.m = 0.0;
            }
        }
        self.m = (self.m + c.dt * (0.8 + a.bass * 0.6)).min(1.0);
        self.ang[0] += c.dt * (0.25 + a.mid * 0.5);
        self.ang[1] += c.dt * (0.4 + a.mid * 0.9);
        self.ang[2] = (c.t * 0.3).sin() * 0.4;
        let m = rot_m(self.ang);
        dusty(fb, c, mix(pal_bg(c), c.pal.at(0.65), a.pulse * 0.1));
        let (from, to) = (&self.shapes[self.from], &self.shapes[self.to]);
        let s = 1.0 + a.pulse * 0.18;
        let fov = 200.0;
        let sparkle = (a.treble * 40.0) as u32;
        let frame = (c.time * 60.0) as u32;
        for i in 0..DOTS {
            let lag = i as f32 / DOTS as f32 * 0.4;
            let k = smoothstep((self.m * 1.4 - lag) / 1.0);
            let puff = 1.0 + (k * PI).sin() * 0.35;
            let p = [
                lerp(from[i][0], to[i][0], k) * puff,
                lerp(from[i][1], to[i][1], k) * puff,
                lerp(from[i][2], to[i][2], k) * puff,
            ];
            let mut q = mv(&m, mulv(p, s));
            q[2] += 3.3;
            let (x, y) = project(q, fov);
            let depth = ((q[2] - 2.0) / 2.6).clamp(0.0, 1.0);
            let b = 1.0 - depth;
            let mut col = if self.hue_mode {
                scale(c.pal.at(i as f32 / DOTS as f32 + c.t * 0.05), 0.25 + b * 0.9)
            } else {
                c.pal.ramp(0.2 + 0.75 * b)
            };
            if (i as u32 ^ frame.wrapping_mul(2_654_435_761)) % 97 < sparkle / 8 {
                col = rgb(255, 255, 255);
            }
            dot_add(fb, x, y, if b > 0.55 { 2 } else { 1 }, col);
        }
    }
}

// ---------------------------------------------------------------- 3. TORUS

/// A solid z-sorted torus (or torus knot, or flower ring) with Gouraud or banded
/// flat shading; it pumps out along its normals on every beat.
struct Torus {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tris: Vec<[u16; 3]>,
    band: Vec<u8>,
    cam: Vec<V3>,
    scr: Vec<(f32, f32, f32)>,
    order: Vec<(f32, u16)>,
    gouraud: bool,
    ang: V3,
    spin: V3,
}

impl Torus {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let shape = rng.below(3);
        let (nu, nv) = match shape {
            0 => (30, 14),
            1 => (120, 8),
            _ => (40, 12),
        };
        // surface point and the spine point it bulges away from
        let surf = |u: f32, v: f32| -> (V3, V3) {
            match shape {
                0 => {
                    let c = [u.cos(), 0.0, u.sin()];
                    (addv(c, addv(mulv(c, 0.45 * v.cos()), [0.0, 0.45 * v.sin(), 0.0])), c)
                }
                1 => {
                    let k = |u: f32| mulv([(2.0 + (3.0 * u).cos()) * (2.0 * u).cos(), (2.0 + (3.0 * u).cos()) * (2.0 * u).sin(), (3.0 * u).sin()], 0.42);
                    let c = k(u);
                    let t = norm(sub(k(u + 0.01), c));
                    let n = norm(cross(t, [0.0, 0.0, 1.0]));
                    let b = cross(t, n);
                    (addv(c, addv(mulv(n, 0.2 * v.cos()), mulv(b, 0.2 * v.sin()))), c)
                }
                _ => {
                    let c = [u.cos(), 0.0, u.sin()];
                    let r = 0.3 + 0.14 * (5.0 * u).sin();
                    (addv(c, addv(mulv(c, r * v.cos()), [0.0, r * v.sin(), 0.0])), c)
                }
            }
        };
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        for j in 0..nv {
            for i in 0..nu {
                let (p, ctr) = surf(i as f32 / nu as f32 * TAU, j as f32 / nv as f32 * TAU);
                pos.push(p);
                nrm.push(norm(sub(p, ctr)));
            }
        }
        let id = |i: usize, j: usize| ((j % nv) * nu + i % nu) as u16;
        let mut tris = Vec::new();
        let mut band = Vec::new();
        for j in 0..nv {
            for i in 0..nu {
                let q = [id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)];
                for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                    let [a, b, c] = t.map(|k| pos[k as usize]);
                    let n = cross(sub(b, a), sub(c, a));
                    let avg = addv(addv(nrm[t[0] as usize], nrm[t[1] as usize]), nrm[t[2] as usize]);
                    tris.push(if dot(n, avg) >= 0.0 { t } else { [t[0], t[2], t[1]] });
                    band.push((((i * 4 / nu) + j / (nv / 2).max(1)) % 4) as u8);
                }
            }
        }
        let n = pos.len();
        Torus {
            cam: vec![[0.0; 3]; n],
            scr: vec![(0.0, 0.0, 0.0); n],
            order: Vec::with_capacity(tris.len()),
            pos,
            nrm,
            tris,
            band,
            gouraud: rng.below(3) != 0,
            ang: [1.0, 0.0, 0.0],
            spin: [rng.range(0.2, 0.4), rng.range(0.5, 0.9), rng.range(0.1, 0.3)],
        }
    }
}

impl Effect for Torus {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        for k in 0..3 {
            self.ang[k] += c.dt * self.spin[k] * (0.6 + a.mid * 1.2);
        }
        let m = rot_m(self.ang);
        dusty(fb, c, pal_bg(c));
        // a slow pair of copper lines behind the object
        for k in 0..2 {
            let y = H as f32 * (0.5 + 0.38 * (c.t * 0.7 + k as f32 * PI).sin());
            fb.copper_bar(y, 5.0 + a.bass * 3.0, scale(c.pal.at(0.4 + k as f32 * 0.3), 0.6));
        }
        let push = a.pulse * 0.12;
        let s = 0.95 + a.bass * 0.08;
        let l = norm([-0.4, -0.6, -0.7]);
        let fov = 190.0;
        for i in 0..self.pos.len() {
            let n = mv(&m, self.nrm[i]);
            let mut q = mv(&m, mulv(addv(self.pos[i], mulv(self.nrm[i], push)), s));
            q[2] += 4.3;
            self.cam[i] = q;
            let (x, y) = project(q, fov);
            let (d, h) = light(n, l);
            self.scr[i] = (x, y, (55.0 + d * 170.0 + h * 110.0).min(255.0));
        }
        self.order.clear();
        for (k, t) in self.tris.iter().enumerate() {
            let [p0, p1, p2] = t.map(|i| self.cam[i as usize]);
            if dot(cross(sub(p1, p0), sub(p2, p0)), p0) >= 0.0 {
                continue;
            }
            self.order.push((p0[2] + p1[2] + p2[2], k as u16));
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let tint = vivid(c.pal, 0.3 + c.t * 0.02);
        let lut: [u32; 256] = std::array::from_fn(|i| {
            let l = i as f32 / 255.0;
            mix(scale(tint, 0.1 + l * 1.1), rgb(255, 255, 255), (l - 0.78) * 3.5)
        });
        let bands: [u32; 4] = std::array::from_fn(|i| vivid(c.pal, 0.1 + i as f32 * 0.22 + c.t * 0.02));
        for &(_, k) in &self.order {
            let t = self.tris[k as usize];
            let p = t.map(|i| self.scr[i as usize]);
            if self.gouraud {
                tri_lut(fb, p, &lut);
            } else {
                let sh = (p[0].2 + p[1].2 + p[2].2) / (3.0 * 255.0);
                let col = lit(bands[self.band[k as usize] as usize], 0.15 + sh, (sh - 0.85).max(0.0) * 2.0);
                fb.tri([(p[0].0, p[0].1), (p[1].0, p[1].1), (p[2].0, p[2].1)], col, 1.0);
            }
        }
    }
}

// ---------------------------------------------------------------- 4. WIREFRAME

/// Glowing wireframe polyhedra (and a 4D hypercube) over a scrolling grid floor;
/// lines dim with depth and the shape changes on the beat.
struct Wireframe {
    shapes: Vec<(Vec<[f32; 4]>, Vec<(usize, usize)>, bool)>,
    cur: usize,
    count: u32,
    ang: V3,
    ang4: f32,
    floor: f32,
    inner: bool,
}

fn tesseract() -> (Vec<[f32; 4]>, Vec<(usize, usize)>, bool) {
    let v: Vec<[f32; 4]> = (0..16)
        .map(|i| {
            let s = |b: usize| if i & b != 0 { 0.75 } else { -0.75 };
            [s(1), s(2), s(4), s(8)]
        })
        .collect();
    let mut e = Vec::new();
    for i in 0..16usize {
        for b in 0..4 {
            let j = i ^ (1 << b);
            if i < j {
                e.push((i, j));
            }
        }
    }
    (v, e, true)
}

impl Wireframe {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut shapes: Vec<_> = (0..7)
            .map(|k| {
                let s = Solid::new(k);
                (s.v.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect(), s.edges, false)
            })
            .collect();
        shapes.push(tesseract());
        for i in (1..shapes.len()).rev() {
            shapes.swap(i, rng.below(i as u32 + 1) as usize);
        }
        Wireframe { shapes, cur: 0, count: 0, ang: [0.3, 0.0, 0.0], ang4: 0.0, floor: 0.0, inner: rng.below(2) == 0 }
    }

    fn shape(&self, fb: &mut Fb, c: &Ctx, m: &M3, s: f32, bright: f32, hue: f32) {
        let (v, e, four) = &self.shapes[self.cur];
        let mut sp = [(0.0f32, 0.0f32, 0.0f32); 32];
        let (s4, c4) = self.ang4.sin_cos();
        for (i, p) in v.iter().enumerate().take(32) {
            let mut q = [p[0], p[1], p[2]];
            if *four {
                // rotate in the x-w plane, then project 4D -> 3D
                let (x, w) = (p[0] * c4 - p[3] * s4, p[0] * s4 + p[3] * c4);
                let k = 1.6 / (2.2 - w);
                q = [x * k, p[1] * k, p[2] * k];
            }
            let mut r = mulv(mv(m, q), s);
            r[2] += 4.2;
            let (x, y) = proj_at(r, 200.0, W as f32 / 2.0, H as f32 * 0.52);
            sp[i] = (x, y, ((5.2 - r[2]) / 2.0).clamp(0.15, 1.0) * bright);
        }
        let col = |b: f32| {
            let base = vivid(c.pal, hue);
            mix(scale(base, b), c.pal.hi, (b - 0.6).max(0.0))
        };
        for &(i, j) in e {
            let (a, b) = (sp[i], sp[j]);
            glow_line(fb, (a.0, a.1), (b.0, b.1), col(a.2), col(b.2));
        }
        for p in sp.iter().take(v.len()) {
            dot_add(fb, p.0, p.1, 2, col(p.2));
        }
    }
}

impl Effect for Wireframe {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.count += 1;
            if self.count % 2 == 0 {
                self.cur = (self.cur + 1) % self.shapes.len();
            }
        }
        self.ang[0] += c.dt * (0.3 + a.mid * 0.7);
        self.ang[1] += c.dt * (0.5 + a.mid * 1.1);
        self.ang4 += c.dt * (0.6 + a.bass);
        self.floor = fract(self.floor + c.dt * (0.8 + a.bass * 2.0));

        // sky fading into a glowing horizon, dark floor below
        let hz = H as f32 / 2.0 + 8.0;
        let glow = mix(pal_bg(c), vivid(c.pal, 0.75), 0.4 + a.pulse * 0.25);
        horizon(fb, pal_bg(c), glow, scale(c.pal.bg, 0.6), hz);
        // grid floor scrolling towards the viewer
        let gcol = scale(vivid(c.pal, 0.45), 0.6);
        let (cx, fov, fy, far) = (W as f32 / 2.0, 200.0, 1.7, 30.0);
        let cy = hz - fy / far * fov;
        for k in -12..=12 {
            let x = k as f32 * 1.2;
            let p0 = proj_at([x, fy, 1.5], fov, cx, cy);
            let p1 = proj_at([x, fy, far], fov, cx, cy);
            line_add(fb, p0, p1, gcol, 0);
        }
        for k in 0..24 {
            let z = 1.5 + (k as f32 + 1.0 - self.floor) * 1.2;
            let f = (1.0 - z / far).max(0.0);
            let cc = dim(gcol, (f * 256.0) as u32);
            let y = (cy + fy / z * fov) as i32;
            if (0..HI).contains(&y) {
                let row = y as usize * W;
                for px in &mut fb.px[row..row + W] {
                    *px = sat_add(*px, cc);
                }
            }
        }

        let m = rot_m(self.ang);
        let s = 1.0 + a.pulse * 0.25;
        self.shape(fb, c, &m, s, 1.0, 0.1 + c.t * 0.03);
        if self.inner {
            let m2 = rot_m([-self.ang[0] * 1.3, -self.ang[1], self.ang[2]]);
            self.shape(fb, c, &m2, s * 0.5, 0.6, 0.55 + c.t * 0.03);
        }
    }
}

// ---------------------------------------------------------------- 5. RUBBER CUBE

/// A jelly cube: every scanline is drawn with the rotation from a few frames
/// earlier, so beat kicks ripple through it like rubber. Checker or striped faces.
struct RubberCube {
    hist: [[f32; 4]; 128],
    head: usize,
    ang: V3,
    spin: V3,
    kick: f32,
    lag: f32,
    pattern: u32,
}

const CUBE_FACES: [([usize; 4], V3); 6] = [
    ([0, 1, 3, 2], [0.0, 0.0, -1.0]),
    ([4, 6, 7, 5], [0.0, 0.0, 1.0]),
    ([0, 2, 6, 4], [-1.0, 0.0, 0.0]),
    ([1, 5, 7, 3], [1.0, 0.0, 0.0]),
    ([0, 4, 5, 1], [0.0, -1.0, 0.0]),
    ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
];

fn cube_corner(i: usize) -> V3 {
    let s = |b: usize| if i & b != 0 { 1.0 } else { -1.0 };
    [s(1), s(2), s(4)]
}

impl RubberCube {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        RubberCube {
            hist: [[0.4, 0.3, 0.0, 1.0]; 128],
            head: 0,
            ang: [0.4, 0.3, 0.0],
            spin: [rng.range(0.6, 1.0), rng.range(0.8, 1.3), rng.range(0.1, 0.4)],
            kick: 0.0,
            lag: rng.range(5.0, 10.0),
            pattern: rng.below(3),
        }
    }

    fn sample(&self, back: f32) -> [f32; 4] {
        let back = back.clamp(0.0, 126.0);
        let i = back.floor() as usize;
        let f = back - i as f32;
        let a = self.hist[(self.head + 128 - i) % 128];
        let b = self.hist[(self.head + 128 - i - 1) % 128];
        std::array::from_fn(|k| lerp(a[k], b[k], f))
    }
}

impl Effect for RubberCube {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.kick = 1.0;
        }
        self.kick *= (-c.dt * 4.0).exp();
        let k = (0.5 + a.mid) * (1.0 + self.kick * 4.0);
        for i in 0..3 {
            self.ang[i] += c.dt * self.spin[i] * k;
        }
        self.head = (self.head + 1) % 128;
        self.hist[self.head] = [self.ang[0], self.ang[1], self.ang[2], 1.0 + a.pulse * 0.22 + a.bass * 0.05];

        let fov = 190.0;
        let l = norm([-0.4, -0.6, -0.7]);
        let face_col: [u32; 6] = std::array::from_fn(|i| vivid(c.pal, i as f32 / 6.0 + 0.05));
        let cy = H as f32 / 2.0;
        // background: copper bars sweeping up and down behind the cube
        fb.clear(pal_bg(c));
        for k in 0..4 {
            let y = cy + (c.t * 1.1 + k as f32 * 0.8).sin() * 80.0;
            fb.copper_bar(y, 7.0, scale(vivid(c.pal, k as f32 * 0.25), 0.55 + a.bass * 0.2));
        }
        for y in 0..H {
            let fy = y as f32 + 0.5;
            let st = self.sample((fy - cy).abs() / cy * self.lag);
            let m = rot_m([st[0], st[1], st[2]]);
            let mut cam = [[0.0; 3]; 8];
            let mut sp = [(0.0f32, 0.0f32); 8];
            for i in 0..8 {
                let mut q = mulv(mv(&m, cube_corner(i)), 0.95 * st[3]);
                q[2] += 5.2;
                cam[i] = q;
                sp[i] = project(q, fov);
            }
            for (fi, (f, n)) in CUBE_FACES.iter().enumerate() {
                let n = mv(&m, *n);
                if dot(n, cam[f[0]]) >= 0.0 {
                    continue;
                }
                // intersect the face outline with this scanline, carrying face (u, v)
                let uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
                let mut lo = (f32::MAX, 0.0, 0.0);
                let mut hi = (f32::MIN, 0.0, 0.0);
                for e in 0..4 {
                    let (p0, p1) = (sp[f[e]], sp[f[(e + 1) % 4]]);
                    if (p0.1 <= fy) == (p1.1 <= fy) {
                        continue;
                    }
                    let t = (fy - p0.1) / (p1.1 - p0.1);
                    let x = p0.0 + (p1.0 - p0.0) * t;
                    let (u0, v0) = uv[e];
                    let (u1, v1) = uv[(e + 1) % 4];
                    let (u, v) = (u0 + (u1 - u0) * t, v0 + (v1 - v0) * t);
                    if x < lo.0 {
                        lo = (x, u, v);
                    }
                    if x > hi.0 {
                        hi = (x, u, v);
                    }
                }
                if hi.0 <= lo.0 {
                    continue;
                }
                let (d, h) = light(n, l);
                let base = lit(face_col[fi], 0.2 + 0.85 * d, h * 0.8);
                let alt = dim(base, 150);
                let edge = mix(base, rgb(255, 255, 255), 0.5);
                let xs = (lo.0.ceil() as i32).max(0);
                let xe = (hi.0.ceil() as i32).min(WI);
                let span = (hi.0 - lo.0).max(1e-3);
                let row = y * W;
                for x in xs..xe {
                    let t = (x as f32 - lo.0) / span;
                    let (u, v) = (lerp(lo.1, hi.1, t), lerp(lo.2, hi.2, t));
                    let on = match self.pattern {
                        0 => ((u * 4.0) as i32 + (v * 4.0) as i32) & 1 == 0,
                        1 => ((u + v) * 5.0) as i32 & 1 == 0,
                        _ => true,
                    };
                    let rim = u < 0.04 || u > 0.96 || v < 0.04 || v > 0.96;
                    fb.px[row + x as usize] = if rim { edge } else if on { base } else { alt };
                }
            }
        }
    }
}

// ---------------------------------------------------------------- 6. CUBEFIELD

/// Flying through a field of shaded, fogged cubes; the camera banks with the
/// mids and the horizon tilts with it, bass and beats push the speed.
struct Cubefield {
    cubes: Vec<(V3, V3, f32)>,
    order: Vec<(f32, u16)>,
    rng: Rng,
    roll: f32,
    travel: f32,
    spin: bool,
    lanes: bool,
    ang: f32,
    solid: Solid,
}

const FIELD_Z: f32 = 40.0;

impl Cubefield {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let v = rng.below(4);
        let mut f = Cubefield {
            cubes: Vec::new(),
            order: Vec::with_capacity(90),
            rng: Rng::new(seed ^ 0x55),
            roll: 0.0,
            travel: 0.0,
            spin: v != 1,
            lanes: v >= 2,
            ang: 0.0,
            solid: Solid::new(1),
        };
        for _ in 0..80 {
            let mut c = f.spawn();
            c.0[2] = f.rng.range(1.0, FIELD_Z);
            f.cubes.push(c);
        }
        f
    }

    fn spawn(&mut self) -> (V3, V3, f32) {
        let r = &mut self.rng;
        let (mut x, mut y) = (r.range(-9.0, 9.0), r.range(-6.0, 6.0));
        if self.lanes {
            x = (x / 2.5).round() * 2.5;
            y = (y / 2.5).round() * 2.5;
        }
        // keep a clear corridor for the camera to fly through
        if x.abs() < 3.0 && y.abs() < 2.5 {
            x += 3.0f32.copysign(x);
        }
        let rot = [r.f() * TAU, r.f() * TAU, 0.0];
        ([x, y, FIELD_Z], rot, r.f())
    }
}

impl Effect for Cubefield {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let target = (c.t * 0.45).sin() * 0.35 + (a.mid - 0.5) * 0.7;
        self.roll += (target - self.roll) * (1.0 - (-c.dt * 2.0).exp());
        self.ang += c.dt * (0.5 + a.mid);
        let speed = 6.0 + a.bass * 10.0 + a.pulse * 14.0;
        let (sr, cr) = self.roll.sin_cos();
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);

        self.travel += speed * c.dt;
        // tilted horizon: sky above, a perspective checker ground below, both
        // melting into the fog colour at the horizon
        let fog = mix(scale(vivid(c.pal, 0.55), 0.75), pal_bg(c), 0.15);
        let sky: [u32; 64] = std::array::from_fn(|i| mix(fog, pal_bg(c), smoothstep(i as f32 / 40.0)));
        let g0 = scale(vivid(c.pal, 0.1), 0.45);
        let g1 = scale(vivid(c.pal, 0.1), 0.2);
        let fogk: [u32; 64] = std::array::from_fn(|i| (smoothstep(i as f32 / 63.0) * 256.0) as u32);
        let cam_h = 7.0;
        let ground_fov = 170.0;
        for y in 0..H {
            let dy = y as f32 - cy;
            // v: distance below the horizon, u: position along it
            let mut v = dy * cr + cx * sr;
            let mut u = -cx * cr + dy * sr;
            let row = y * W;
            for px in &mut fb.px[row..row + W] {
                *px = if v <= 1.0 {
                    sky[(-v * 0.5).clamp(0.0, 63.0) as usize]
                } else {
                    let z = cam_h * ground_fov / v;
                    let gx = u * z / ground_fov;
                    let chk = ((gx * 0.25).floor() as i32 + ((z + self.travel) * 0.25).floor() as i32) & 1;
                    let base = if chk == 0 { g0 } else { g1 };
                    let k = fogk[((z / FIELD_Z) * 63.0).min(63.0) as usize];
                    mix_fast(base, fog, k)
                };
                v -= sr;
                u += cr;
            }
        }

        for i in 0..self.cubes.len() {
            self.cubes[i].0[2] -= speed * c.dt;
            if self.cubes[i].0[2] < 0.8 {
                let n = self.spawn();
                self.cubes[i] = n;
            }
        }
        self.order.clear();
        for (i, cu) in self.cubes.iter().enumerate() {
            self.order.push((cu.0[2], i as u16));
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let fov = 170.0 + a.pulse * 25.0;
        let sway = [(c.t * 0.31).sin() * 1.5, (c.t * 0.23).cos() * 1.0];
        for &(_, i) in &self.order {
            let (p, rot, hue) = self.cubes[i as usize];
            let (x, y) = (p[0] - sway[0], p[1] - sway[1]);
            let pos = [x * cr - y * sr, x * sr + y * cr, p[2]];
            let m = if self.spin { rot_m([rot[0] + self.ang, rot[1] + self.ang * 0.7, 0.0]) } else { rot_m([0.0, 0.0, -self.roll]) };
            let f = (p[2] / FIELD_Z).clamp(0.0, 1.0);
            let base = vivid(c.pal, hue * 0.7 + 0.35);
            let l = norm([-0.5, -0.7, -0.5]);
            self.solid.draw(fb, &m, 0.75 * 3f32.sqrt(), pos, fov, |_, n| {
                let (d, h) = light(n, l);
                mix(lit(base, 0.3 + 0.75 * d, h * 0.7), fog, smoothstep(f * 1.2))
            });
        }
    }
}

// ---------------------------------------------------------------- 7. DOT TUNNEL

/// Rings of dots forming a curving tunnel the camera flies through; bass sets
/// the speed, beats pump the rings, the mids twist them.
struct DotTunnel {
    radial: Vec<u8>,
    s: f32,
    twist: f32,
    per_ring: usize,
    shape: u32,
}

const RINGS: usize = 44;
const RING_DZ: f32 = 1.0;

impl DotTunnel {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let (tw, th) = (W * 2, H * 2);
        let mut radial = vec![0; tw * th];
        for y in 0..th {
            for x in 0..tw {
                let (dx, dy) = (x as f32 - W as f32, y as f32 - H as f32);
                radial[y * tw + x] = ((dx * dx + dy * dy).sqrt() / 1.6).min(255.0) as u8;
            }
        }
        DotTunnel { radial, s: 0.0, twist: 0.0, per_ring: [36, 48, 28][rng.below(3) as usize], shape: rng.below(3) }
    }

    fn path(z: f32) -> (f32, f32) {
        ((z * 0.06).sin() * 3.0 + (z * 0.021).sin() * 4.0, (z * 0.047).cos() * 2.5)
    }
}

impl Effect for DotTunnel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.s += c.dt * (4.0 + a.bass * 7.0 + a.pulse * 6.0);
        self.twist += c.dt * (0.3 + a.mid * 1.2);
        let (px0, py0) = Self::path(self.s);
        let (px1, py1) = Self::path(self.s + 0.5);
        let (dxs, dys) = ((px1 - px0) * 2.0, (py1 - py0) * 2.0);
        let fov = 170.0;
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        let rel = |z: f32| {
            let (x, y) = Self::path(self.s + z);
            (x - px0 - dxs * z, y - py0 - dys * z)
        };

        // background: radial glow around the far vanishing point
        let far = 40.0;
        let (vx, vy) = rel(far);
        let (ox, oy) = ((vx / far * fov) as i32, (vy / far * fov) as i32);
        let (ox, oy) = (ox.clamp(-(W as i32) / 2, W as i32 / 2), oy.clamp(-(H as i32) / 2, H as i32 / 2));
        let glow = mix(vivid(c.pal, 0.5), rgb(255, 255, 255), 0.3 + a.pulse * 0.3);
        let lut: [u32; 256] = std::array::from_fn(|i| mix(glow, pal_bg(c), (i as f32 / 22.0).min(1.0).sqrt()));
        let tw = W * 2;
        for y in 0..H {
            let sy = (y as i32 - oy + H as i32 / 2) as usize;
            let sx = (W as i32 / 2 - ox) as usize;
            let src = &self.radial[sy * tw + sx..sy * tw + sx + W];
            for (d, &r) in fb.px[y * W..(y + 1) * W].iter_mut().zip(src) {
                *d = lut[r as usize];
            }
        }

        let first = (self.s / RING_DZ).floor() as i32 + 1;
        let n = self.per_ring;
        for k in (0..RINGS as i32).rev() {
            let ring = first + k;
            let zw = ring as f32 * RING_DZ;
            let z = zw - self.s;
            if z < 0.2 {
                continue;
            }
            let (ox, oy) = rel(z);
            let fade = (1.0 - z / (RINGS as f32 * RING_DZ)).max(0.0);
            let pump = 1.0 + a.pulse * 0.25 * fade;
            let r = 2.0 * pump;
            let bright = ring % 4 == 0;
            let base = if bright { c.pal.hi } else { c.pal.at(ring as f32 * 0.03 + c.t * 0.05) };
            let col = scale(base, fade * (0.5 + 0.7 * fade));
            let size = if z < 4.0 { 3 } else if z < 12.0 { 2 } else { 1 };
            let tw = zw * 0.08 + self.twist;
            for j in 0..n {
                let th = j as f32 / n as f32 * TAU + tw;
                let (s, co) = th.sin_cos();
                let (x, y) = match self.shape {
                    0 => (co * r, s * r),
                    1 => {
                        // rounded square
                        let k = r / (co.abs().powi(4) + s.abs().powi(4)).powf(0.25);
                        (co * k, s * k)
                    }
                    _ => {
                        // spiral arms: radius wobbles around the ring
                        let k = r * (1.0 + 0.18 * (th * 3.0 - zw * 0.4).sin());
                        (co * k, s * k)
                    }
                };
                let (sx, sy) = (cx + (x + ox) / z * fov, cy + (y + oy) / z * fov);
                dot_add(fb, sx, sy, size, col);
            }
        }
    }
}

// ---------------------------------------------------------------- 8. BOB TRAILS

/// Shaded vector bobs chasing each other along a 3D knot, smearing trails into
/// their own fading buffer; beats pop the bobs, bass lengthens the trails.
struct BobTrails {
    buf: Vec<u32>,
    masks: Vec<Vec<u8>>,
    order: Vec<(f32, f32, f32, f32, u16)>,
    n: usize,
    drift: u32,
    path: u32,
    ang: V3,
}

const MAX_R: usize = 16;

impl BobTrails {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let masks = (0..=MAX_R)
            .map(|r| {
                let d = r * 2 + 1;
                let mut m = vec![0u8; d * d];
                let rf = r as f32 + 0.5;
                for y in 0..d {
                    for x in 0..d {
                        let (dx, dy) = ((x as f32 - r as f32) / rf, (y as f32 - r as f32) / rf);
                        let nz2 = 1.0 - dx * dx - dy * dy;
                        if nz2 > 0.0 {
                            let l = (-0.45 * dx - 0.55 * dy + 0.7 * nz2.sqrt()).max(0.0);
                            m[y * d + x] = (30.0 + l * 225.0) as u8;
                        }
                    }
                }
                m
            })
            .collect();
        BobTrails {
            buf: vec![0xFF00_0000; W * H],
            masks,
            order: Vec::with_capacity(40),
            n: [18, 24, 32][rng.below(3) as usize],
            drift: rng.below(3),
            path: rng.below(3),
            ang: [0.0; 3],
        }
    }

    fn knot(&self, u: f32) -> V3 {
        match self.path {
            0 => {
                let r = 1.0 + 0.4 * (3.0 * u).cos();
                [r * (2.0 * u).cos(), r * (2.0 * u).sin(), 0.4 * (3.0 * u).sin()]
            }
            1 => [(3.0 * u).sin() * 1.2, (2.0 * u).sin() * 0.9, (5.0 * u).cos() * 0.8],
            _ => {
                let r = 1.0 + 0.3 * (5.0 * u).sin();
                [r * u.cos(), 0.5 * (4.0 * u).sin(), r * u.sin()]
            }
        }
    }
}

impl Effect for BobTrails {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        // fade (and optionally drift) the trail buffer
        let keep = (222.0 + a.bass * 26.0) as u32;
        match self.drift {
            0 | 1 => {
                self.buf.copy_within(W.., 0);
                self.buf[(H - 1) * W..].fill(0xFF00_0000);
            }
            2 => {
                self.buf.copy_within(1.., 0);
            }
            _ => {}
        }
        for p in self.buf.iter_mut() {
            *p = dim(*p, keep);
        }
        self.ang[0] += c.dt * (0.5 + a.mid * 0.8);
        self.ang[1] += c.dt * (0.8 + a.mid * 1.2);
        self.ang[2] = (c.t * 0.4).sin() * 0.6;
        let m = rot_m(self.ang);
        let fov = 180.0;
        self.order.clear();
        for i in 0..self.n {
            let u = i as f32 / self.n as f32 * TAU - c.t * 0.9;
            let mut q = mulv(mv(&m, self.knot(u)), 1.1);
            q[0] *= 1.5;
            q[2] += 4.0;
            let (x, y) = project(q, fov);
            let pop = 1.0 + a.pulse * if i % 2 == 0 { 0.45 } else { 0.2 };
            let r = (0.12 * fov / q[2] * pop).min(MAX_R as f32);
            self.order.push((q[2], x, y, r, i as u16));
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let luts: [[u32; 256]; 4] = std::array::from_fn(|g| {
            let base = vivid(c.pal, g as f32 * 0.25 + 0.1);
            std::array::from_fn(|i| {
                let l = i as f32 / 255.0;
                mix(scale(base, l * 1.2), rgb(255, 255, 255), ((l - 0.8) * 4.0).max(0.0))
            })
        });
        for &(_, x, y, r, i) in &self.order {
            let ri = r.round().max(1.0) as usize;
            let d = ri * 2 + 1;
            let mask = &self.masks[ri];
            let lut = &luts[i as usize % 4];
            let (x0, y0) = (x as i32 - ri as i32, y as i32 - ri as i32);
            for my in 0..d {
                let py = y0 + my as i32;
                if py < 0 || py >= HI {
                    continue;
                }
                for mx in 0..d {
                    let px = x0 + mx as i32;
                    let v = mask[my * d + mx];
                    if v > 0 && px >= 0 && px < WI {
                        self.buf[py as usize * W + px as usize] = lut[v as usize];
                    }
                }
            }
        }
        let bg = pal_bg(c);
        for (d, &s) in fb.px.iter_mut().zip(&self.buf) {
            *d = sat_add(s, bg);
        }
    }
}

// ---------------------------------------------------------------- 9. RAYTRACER

/// Realtime raytraced mirror spheres hopping on the beat over a checker floor,
/// with shadows, one reflection bounce and fog; rendered chunky (2×2 pixels).
struct Raytracer {
    n: usize,
    since: f32,
    hops: u32,
    cam_a: f32,
    block: usize,
}

struct Scene {
    sph: [(V3, f32, (f32, f32, f32)); 4],
    n: usize,
    light: V3,
    sky0: (f32, f32, f32),
    sky1: (f32, f32, f32),
    chk: [(f32, f32, f32); 2],
}

impl Scene {
    /// Nearest sphere hit: (distance, index).
    fn hit_sphere(&self, o: V3, d: V3) -> (f32, usize) {
        let mut best = (f32::MAX, usize::MAX);
        for (i, s) in self.sph[..self.n].iter().enumerate() {
            let oc = sub(o, s.0);
            let b = dot(oc, d);
            let cc = dot(oc, oc) - s.1 * s.1;
            let h = b * b - cc;
            if h > 0.0 {
                let t = -b - h.sqrt();
                if t > 1e-3 && t < best.0 {
                    best = (t, i);
                }
            }
        }
        best
    }

    fn sky(&self, d: V3) -> (f32, f32, f32) {
        let k = (d[1] * 1.6).clamp(0.0, 1.0);
        let (a, b) = (self.sky0, self.sky1);
        (lerp(a.0, b.0, k), lerp(a.1, b.1, k), lerp(a.2, b.2, k))
    }

    /// Colour seen along a ray; `depth` > 0 allows a mirror bounce.
    fn trace(&self, o: V3, d: V3, depth: u32) -> (f32, f32, f32) {
        let (ts, si) = self.hit_sphere(o, d);
        let tp = if d[1] < -1e-4 { -o[1] / d[1] } else { f32::MAX };
        if ts == f32::MAX && tp == f32::MAX {
            return self.sky(d);
        }
        if ts < tp {
            let s = &self.sph[si];
            let p = addv(o, mulv(d, ts));
            let n = mulv(sub(p, s.0), 1.0 / s.1);
            let diff = dot(n, self.light).max(0.0);
            let h = norm(sub(self.light, d));
            let spec = dot(n, h).max(0.0).powi(32);
            let base = (s.2 .0 * (0.15 + diff * 0.8), s.2 .1 * (0.15 + diff * 0.8), s.2 .2 * (0.15 + diff * 0.8));
            let refl = if depth > 0 {
                let r = sub(d, mulv(n, 2.0 * dot(d, n)));
                self.trace(p, r, depth - 1)
            } else {
                self.sky(n)
            };
            let k = 0.55;
            (base.0 * (1.0 - k) + refl.0 * k + spec, base.1 * (1.0 - k) + refl.1 * k + spec, base.2 * (1.0 - k) + refl.2 * k + spec)
        } else {
            let p = addv(o, mulv(d, tp));
            let cell = ((p[0].floor() as i32 + p[2].floor() as i32) & 1) as usize;
            let mut col = self.chk[cell];
            let shadow = if self.hit_sphere(p, self.light).0 < f32::MAX { 0.4 } else { 1.0 };
            col = (col.0 * shadow, col.1 * shadow, col.2 * shadow);
            if depth > 0 && shadow > 0.5 {
                let r = [d[0], -d[1], d[2]];
                let (t2, _) = self.hit_sphere(p, r);
                if t2 < f32::MAX {
                    let rc = self.trace(p, r, 0);
                    col = (col.0 * 0.7 + rc.0 * 0.3, col.1 * 0.7 + rc.1 * 0.3, col.2 * 0.7 + rc.2 * 0.3);
                }
            }
            let fog = (tp / 28.0).min(1.0);
            let sk = self.sky0;
            (lerp(col.0, sk.0, fog), lerp(col.1, sk.1, fog), lerp(col.2, sk.2, fog))
        }
    }
}

impl Raytracer {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        Raytracer { n: 3 + rng.below(2) as usize, since: 10.0, hops: 0, cam_a: rng.f() * TAU, block: 2 }
    }
}

impl Effect for Raytracer {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.since = 0.0;
            self.hops += 1;
        }
        self.since += c.dt;
        self.cam_a += c.dt * (0.15 + a.mid * 0.3);
        let hop = {
            let x = (self.since / a.period.max(0.3)).min(1.0);
            4.0 * x * (1.0 - x)
        };
        let u = |x: u32| unpack(x);
        let mut sc = Scene {
            sph: [([0.0; 3], 1.0, (1.0, 1.0, 1.0)); 4],
            n: self.n,
            light: norm([0.5, 0.9, -0.4]),
            sky0: u(mix(c.pal.at(0.55), rgb(255, 255, 255), 0.25)),
            sky1: u(scale(c.pal.at(0.15), 0.35)),
            chk: [u(c.pal.at(0.05)), u(scale(c.pal.at(0.5), 0.35))],
        };
        for i in 0..self.n {
            let ang = i as f32 / self.n as f32 * TAU + c.t * 0.4;
            let r = 0.8 + (i % 2) as f32 * 0.25;
            let jump = if (self.hops as usize + i) % 2 == 0 { hop * (1.0 + a.bass) } else { 0.0 };
            sc.sph[i] = ([ang.cos() * 2.1, r + jump, ang.sin() * 2.1], r, u(mix(c.pal.at(i as f32 * 0.27 + 0.3), rgb(255, 255, 255), 0.15)));
        }
        let cam = [self.cam_a.sin() * 7.0, 2.6 + (c.t * 0.3).sin() * 0.6, self.cam_a.cos() * 7.0];
        let fwd = norm(sub([0.0, 1.0, 0.0], cam));
        let right = norm(cross([0.0, 1.0, 0.0], fwd));
        let up = cross(fwd, right);
        let flash = a.pulse * 0.15;
        let (hw, hh) = (W as f32 / 2.0, H as f32 / 2.0);
        let zoom = 1.25 * hw;
        let b = self.block as f32 * 0.5;
        chunky(fb, self.block, |x, y| {
            let (sx, sy) = ((x as f32 + b - hw) / zoom, (hh - y as f32 - b) / zoom);
            let d = norm(addv(addv(fwd, mulv(right, sx)), mulv(up, sy)));
            let col = sc.trace(cam, d, 1);
            rgbf(col.0 + flash, col.1 + flash, col.2 + flash)
        });
    }
}

// ---------------------------------------------------------------- 10. EQ CITY

/// A city of bars: the spectrum runs left to right and scrolls back in time,
/// seen from an orbiting camera. Without live audio it plays a fake spectrum.
struct EqCity {
    hist: [[f32; EQ_X]; EQ_Z],
    edges: [usize; EQ_X + 1],
    acc: f32,
    cam_a: f32,
    order: Vec<(f32, u16)>,
    hue_mode: bool,
}

const EQ_X: usize = 16;
const EQ_Z: usize = 14;

impl EqCity {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let edges = std::array::from_fn(|k| (2.0 * (150.0f32).powf(k as f32 / EQ_X as f32)).round() as usize);
        EqCity { hist: [[0.0; EQ_X]; EQ_Z], edges, acc: 0.0, cam_a: rng.f() * TAU, order: Vec::with_capacity(EQ_X * EQ_Z), hue_mode: rng.below(2) == 0 }
    }

    fn spectrum(&self, c: &Ctx) -> [f32; EQ_X] {
        let a = c.a;
        let total: u32 = a.bins[1..300].iter().map(|&b| b as u32).sum();
        std::array::from_fn(|k| {
            if total > 300 && !a.autopilot {
                let (lo, hi) = (self.edges[k], self.edges[k + 1].max(self.edges[k] + 1));
                let s: u32 = a.bins[lo..hi].iter().map(|&b| b as u32).sum();
                (s as f32 / ((hi - lo) as f32 * 255.0)).sqrt()
            } else {
                let x = k as f32;
                let g = |c: f32, w: f32| (-((x - c) / w).powi(2)).exp();
                let wob = 0.5 + 0.5 * (c.time * (2.0 + x * 0.37) + x * 1.7).sin();
                (a.bass * g(1.0, 2.5) + a.mid * 0.7 * g(7.0, 3.0) + a.treble * 0.6 * g(13.0, 2.5)) * (0.55 + 0.45 * wob)
            }
        })
    }
}

impl Effect for EqCity {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let spec = self.spectrum(c);
        for (h, s) in self.hist[0].iter_mut().zip(spec) {
            *h = if s > *h { s } else { *h + (s - *h) * (1.0 - (-c.dt * 6.0).exp()) };
        }
        self.acc += c.dt;
        if self.acc > 0.07 {
            self.acc = 0.0;
            for z in (1..EQ_Z).rev() {
                self.hist[z] = self.hist[z - 1];
            }
        }
        self.cam_a += c.dt * (0.12 + a.mid * 0.35);
        let target = [0.0, 2.0, (EQ_Z as f32) * 0.5 - 0.5];
        let cam = [target[0] + self.cam_a.sin() * 18.0, 7.5 + (c.t * 0.4).sin() * 1.5, target[2] + self.cam_a.cos() * 18.0];
        let fwd = norm(sub(target, cam));
        let right = norm(cross([0.0, 1.0, 0.0], fwd));
        let up = cross(fwd, right);
        let to_cam = |p: V3| {
            let d = sub(p, cam);
            [dot(d, right), -dot(d, up), dot(d, fwd)]
        };

        let glow = mix(vivid(c.pal, 0.7), pal_bg(c), 0.45 - a.pulse * 0.25);
        horizon(fb, pal_bg(c), glow, scale(pal_bg(c), 0.7), horizon_y(fwd, up, 200.0));
        self.order.clear();
        for z in 0..EQ_Z {
            for x in 0..EQ_X {
                let p = [x as f32 - EQ_X as f32 / 2.0 + 0.5, 0.0, z as f32];
                let d = sub(p, cam);
                self.order.push((dot(d, d), (z * EQ_X + x) as u16));
            }
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let fov = 200.0;
        let hw = 0.4;
        for &(_, id) in &self.order {
            let (z, x) = (id as usize / EQ_X, id as usize % EQ_X);
            let h = self.hist[z][x] * 6.0 + 0.05;
            let (bx, bz) = (x as f32 - EQ_X as f32 / 2.0 + 0.5, z as f32);
            let age = 1.0 - z as f32 / EQ_Z as f32;
            let base = if self.hue_mode { vivid(c.pal, x as f32 / EQ_X as f32 * 0.7 + 0.05) } else { c.pal.ramp(0.45 + self.hist[z][x] * 0.55) };
            let base = scale(base, 0.5 + 0.5 * age);
            let corner = |dx: f32, y: f32, dz: f32| project(to_cam([bx + dx, y, bz + dz]), fov);
            // sides facing the camera, then the lid
            if cam[0] > bx + hw {
                poly(fb, &[corner(hw, 0.0, -hw), corner(hw, 0.0, hw), corner(hw, h, hw), corner(hw, h, -hw)], scale(base, 0.55));
            } else if cam[0] < bx - hw {
                poly(fb, &[corner(-hw, 0.0, -hw), corner(-hw, 0.0, hw), corner(-hw, h, hw), corner(-hw, h, -hw)], scale(base, 0.55));
            }
            if cam[2] > bz + hw {
                poly(fb, &[corner(-hw, 0.0, hw), corner(hw, 0.0, hw), corner(hw, h, hw), corner(-hw, h, hw)], scale(base, 0.8));
            } else if cam[2] < bz - hw {
                poly(fb, &[corner(-hw, 0.0, -hw), corner(hw, 0.0, -hw), corner(hw, h, -hw), corner(-hw, h, -hw)], scale(base, 0.8));
            }
            let lid = mix(base, rgb(255, 255, 255), (self.hist[z][x] - 0.5).max(0.0) * 0.8 + if z == 0 { 0.25 } else { 0.0 });
            poly(fb, &[corner(-hw, h, -hw), corner(hw, h, -hw), corner(hw, h, hw), corner(-hw, h, hw)], lid);
        }
    }
}

// ---------------------------------------------------------------- 11. FOUNTAIN

/// A 3D particle fountain with gravity and floor bounces; every beat fires a
/// firework burst in the air. Particles cool from white through the palette.
struct Fountain {
    p: Vec<Particle>,
    next: usize,
    rng: Rng,
    emit: f32,
    cam_a: f32,
    jets: u32,
    ring: bool,
}

#[derive(Clone, Copy)]
struct Particle {
    pos: V3,
    vel: V3,
    age: f32,
    life: f32,
    hue: f32,
}

const PARTICLES: usize = 2600;

impl Fountain {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let v = rng.below(4);
        Fountain {
            p: vec![Particle { pos: [0.0; 3], vel: [0.0; 3], age: 1.0, life: 0.0, hue: 0.0 }; PARTICLES],
            next: 0,
            rng,
            emit: 0.0,
            cam_a: 0.0,
            jets: if v & 1 == 0 { 1 } else { 3 },
            ring: v >= 2,
        }
    }

    fn spawn(&mut self, pos: V3, vel: V3, life: f32, hue: f32) {
        self.p[self.next] = Particle { pos, vel, age: 0.0, life, hue };
        self.next = (self.next + 1) % PARTICLES;
    }
}

impl Effect for Fountain {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let dt = c.dt;
        // fountain jets
        self.emit += dt * (500.0 + a.bass * 700.0);
        while self.emit >= 1.0 {
            self.emit -= 1.0;
            let j = self.rng.below(self.jets) as f32;
            let base = if self.jets == 1 { [0.0; 3] } else { let t = j / 3.0 * TAU; [t.cos() * 3.0, 0.0, t.sin() * 3.0] };
            let (ang, spread) = (self.rng.f() * TAU, self.rng.f() * 1.3);
            let vel = [ang.cos() * spread, self.rng.range(8.5, 10.5) + a.bass * 2.5, ang.sin() * spread];
            let hue = j * 0.3 + self.rng.f() * 0.08;
            self.spawn(base, vel, 2.2, hue);
        }
        // fireworks on the beat
        if a.beat {
            let at = [self.rng.range(-3.5, 3.5), self.rng.range(6.0, 8.5), self.rng.range(-3.5, 3.5)];
            let hue = self.rng.f();
            for k in 0..260 {
                let vel = if self.ring {
                    let t = k as f32 / 260.0 * TAU;
                    let s = self.rng.range(5.5, 6.5);
                    [t.cos() * s, self.rng.range(-0.4, 0.4), t.sin() * s]
                } else {
                    let d = norm([self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0)]);
                    mulv(d, self.rng.range(4.0, 7.0))
                };
                let (life, h) = (self.rng.range(1.2, 1.8), hue + self.rng.f() * 0.1);
                self.spawn(at, vel, life, h);
            }
        }
        self.cam_a += dt * (0.2 + a.mid * 0.4);

        let cam = [self.cam_a.sin() * 17.0, 3.0, self.cam_a.cos() * 17.0];
        let target = [0.0, 4.5, 0.0];
        let fwd = norm(sub(target, cam));
        let right = norm(cross([0.0, 1.0, 0.0], fwd));
        let up = cross(fwd, right);
        let to_cam = |p: V3| {
            let d = sub(p, cam);
            [dot(d, right), -dot(d, up), dot(d, fwd)]
        };
        let fov = 180.0;

        let glow = mix(vivid(c.pal, 0.6), pal_bg(c), 0.5 - a.pulse * 0.2);
        horizon(fb, pal_bg(c), glow, scale(pal_bg(c), 0.6), horizon_y(fwd, up, fov));
        // floor grid
        let gc = scale(c.pal.at(0.6), 0.35);
        for k in -6..=6 {
            let x = k as f32 * 1.5;
            for (p0, p1) in [([x, 0.0, -9.0], [x, 0.0, 9.0]), ([-9.0, 0.0, x], [9.0, 0.0, x])] {
                let (q0, q1) = (to_cam(p0), to_cam(p1));
                if q0[2] > 0.5 && q1[2] > 0.5 {
                    let f = |q: V3| dim(gc, ((1.6 - q[2] / 16.0).clamp(0.0, 1.0) * 200.0) as u32);
                    line_add(fb, project(q0, fov), project(q1, fov), f(q0), f(q1));
                }
            }
        }
        for p in self.p.iter_mut() {
            if p.age >= p.life {
                continue;
            }
            p.age += dt;
            p.vel[1] -= 9.8 * dt;
            p.vel = mulv(p.vel, 1.0 - dt * 0.3);
            p.pos = addv(p.pos, mulv(p.vel, dt));
            if p.pos[1] < 0.0 {
                p.pos[1] = 0.0;
                p.vel[1] *= -0.4;
                p.vel[0] *= 0.7;
                p.vel[2] *= 0.7;
            }
            let q = to_cam(p.pos);
            if q[2] < 0.5 {
                continue;
            }
            let heat = 1.0 - p.age / p.life;
            let col = mix(scale(c.pal.at(p.hue), heat * 1.3), rgb(255, 255, 240), (heat - 0.85).max(0.0) * 4.0);
            let tail = to_cam(sub(p.pos, mulv(p.vel, 0.025)));
            let (h, t) = (project(q, fov), project(tail, fov));
            line_add(fb, t, h, dim(col, 90), col);
            if q[2] < 12.0 && heat > 0.4 {
                dot_add(fb, h.0, h.1, 2, dim(col, 120));
            }
        }
    }
}

// ---------------------------------------------------------------- 12. LISSAJOUS

/// A 3D Lissajous figure drawn as a shaded, z-sorted ribbon (or a chain of
/// glowing dots); its frequencies slide with the music, beats widen it.
struct Lissajous {
    freq: V3,
    shift: f32,
    ang: V3,
    ribbon: bool,
    pts: Vec<(V3, V3)>,
    order: Vec<(f32, u16)>,
}

const LISS_N: usize = 640;

impl Lissajous {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let freq = [[3.0, 2.0, 5.0], [5.0, 4.0, 3.0], [2.0, 3.0, 7.0], [4.0, 5.0, 2.0]][rng.below(4) as usize];
        Lissajous {
            freq,
            shift: 0.0,
            ang: [0.4, 0.0, 0.0],
            ribbon: rng.below(3) != 0,
            pts: vec![([0.0; 3], [0.0; 3]); LISS_N + 1],
            order: Vec::with_capacity(LISS_N),
        }
    }
}

impl Effect for Lissajous {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.shift += c.dt * (0.03 + a.mid * 0.08);
        self.ang[0] += c.dt * (0.2 + a.mid * 0.3);
        self.ang[1] += c.dt * (0.35 + a.mid * 0.5);
        let m = rot_m(self.ang);
        let f = [self.freq[0] + self.shift.sin() * 0.5, self.freq[1] + (self.shift * 0.7).cos() * 0.5, self.freq[2] + a.pulse * 0.04];
        let ph = [c.t * 0.3, c.t * 0.2 + 1.0, c.t * 0.25 + 2.0];
        let curve = |u: f32| [(f[0] * u + ph[0]).sin(), (f[1] * u + ph[1]).sin(), (f[2] * u + ph[2]).sin()];
        let width = 0.06 + a.pulse * 0.07 + a.bass * 0.02;
        let s = 1.05 + a.bass * 0.08;
        for i in 0..=LISS_N {
            let u = i as f32 / LISS_N as f32 * TAU;
            let p = curve(u);
            let t = norm(sub(curve(u + 0.003), p));
            let w = norm(cross(t, norm(addv(p, [0.0, 0.0, 0.01]))));
            let mut q = mv(&m, mulv(p, s));
            q[2] += 4.2;
            self.pts[i] = (q, mv(&m, mulv(w, width)));
        }
        dusty(fb, c, pal_bg(c));
        let fov = 190.0;
        if !self.ribbon {
            for i in 0..LISS_N {
                let (q, _) = self.pts[i];
                let (x, y) = project(q, fov);
                let b = ((5.4 - q[2]) / 2.4).clamp(0.1, 1.0);
                let col = scale(c.pal.at(i as f32 / LISS_N as f32 * 3.0 + c.t * 0.1), 0.25 + b);
                dot_add(fb, x, y, 3, dim(col, 80));
                dot_add(fb, x, y, 2, col);
            }
            return;
        }
        self.order.clear();
        for i in 0..LISS_N {
            self.order.push((self.pts[i].0[2] + self.pts[i + 1].0[2], i as u16));
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let l = norm([-0.3, -0.6, -0.75]);
        for &(_, i) in &self.order {
            let i = i as usize;
            let ((p0, w0), (p1, w1)) = (self.pts[i], self.pts[i + 1]);
            let n = norm(cross(sub(p1, p0), w0));
            let d = dot(n, l).abs();
            let spec = dot(n, norm(addv(l, [0.0, 0.0, -1.0]))).abs().powi(20);
            let fog = ((5.6 - p0[2]) / 2.8).clamp(0.2, 1.0);
            let base = c.pal.at(i as f32 / LISS_N as f32 * 2.0 + c.t * 0.08);
            let col = lit(base, (0.2 + 0.85 * d) * fog, spec * 0.8);
            let corners = [addv(p0, w0), addv(p1, w1), sub(p1, w1), sub(p0, w0)].map(|q| project(q, fov));
            poly(fb, &corners, col);
        }
    }
}

// ---------------------------------------------------------------- 13. GEMS

/// Flat-shaded polyhedra (tetra-, octa-, icosa-, dodecahedra and friends)
/// orbiting a large central gem under a moving light; beats pop them in turn.
struct Gems {
    solids: Vec<Solid>,
    centre: usize,
    count: u32,
    orbit: f32,
    spin: f32,
    tilt: f32,
    order: Vec<(f32, u16)>,
}

impl Gems {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut solids: Vec<Solid> = (0..7).map(Solid::new).collect();
        for i in (1..solids.len()).rev() {
            solids.swap(i, rng.below(i as u32 + 1) as usize);
        }
        Gems { solids, centre: 0, count: 0, orbit: rng.f() * TAU, spin: 0.0, tilt: rng.range(0.25, 0.5), order: Vec::with_capacity(8) }
    }
}

impl Effect for Gems {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            self.count += 1;
            if self.count % 8 == 0 {
                self.centre = (self.centre + 1) % self.solids.len();
            }
        }
        self.orbit += c.dt * (0.35 + a.mid * 0.5);
        self.spin += c.dt * (0.6 + a.mid);
        dusty(fb, c, pal_bg(c));
        // faint sparkles twinkle with the treble
        let frame = (c.time * 20.0) as u32;
        for k in 0..60u32 {
            let h = (k.wrapping_mul(2_654_435_761) ^ frame.wrapping_mul(40_503)).wrapping_mul(2_246_822_519);
            if (h >> 28) as f32 / 16.0 < a.treble * 0.6 {
                let (x, y) = ((k * 97 % W as u32) as f32, (k * 61 % H as u32) as f32);
                dot_add(fb, x, y, 1 + (h & 1) as i32, c.pal.hi);
            }
        }

        let ring = self.solids.len() - 1;
        let (st, ct) = self.tilt.sin_cos();
        self.order.clear();
        let mut pos = [[0.0f32; 3]; 8];
        for k in 0..self.solids.len() {
            let p = if k == self.centre {
                [0.0, 0.0, 0.0]
            } else {
                let slot = (k + self.solids.len() - self.centre - 1) % self.solids.len();
                let t = self.orbit + slot as f32 / ring as f32 * TAU;
                let (x, z) = (t.cos() * 2.4, t.sin() * 2.4);
                [x, z * st, z * ct]
            };
            pos[k] = [p[0], p[1], p[2] + 6.0];
            self.order.push((pos[k][2], k as u16));
        }
        self.order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
        let fov = 190.0;
        let la = c.t * 0.7;
        let key = norm([la.cos() * 0.8, -0.6, -0.5 + la.sin() * 0.3]);
        let fill = norm([-key[0], 0.6, 0.2]);
        for &(_, k) in &self.order {
            let k = k as usize;
            let centre = k == self.centre;
            let pop = if (self.count as usize % self.solids.len()) == k { a.pulse * 0.35 } else { a.pulse * 0.08 };
            let size = if centre { 1.25 } else { 0.55 } * (1.0 + pop);
            let m = rot_m([self.spin * (0.7 + k as f32 * 0.13), self.spin * (1.0 - k as f32 * 0.07), k as f32]);
            let hue = k as f32 / self.solids.len() as f32;
            let base = vivid(c.pal, hue);
            let rim = scale(vivid(c.pal, hue + 0.5), 0.6);
            self.solids[k].draw(fb, &m, size, pos[k], fov, |fi, n| {
                let (d, h) = light(n, key);
                let back = dot(n, fill).max(0.0);
                let face = scale(base, 0.92 + (fi % 3) as f32 * 0.06);
                add(lit(face, 0.22 + 0.85 * d, h * 0.9), scale(rim, back))
            });
        }
    }
}
