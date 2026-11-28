//! UFO — 100 Amiga-style demos that dance to whatever is playing.
//!
//! Compiled to WebAssembly, this crate exposes a tiny C ABI to the page:
//! the page copies FFT bins in, calls `ufo_frame`, and blits the framebuffer.
//! It can also hand over the song now playing and a 256×256 artist picture.

pub mod audio;
pub mod demo;
pub mod eggs;
pub mod font;
pub mod fx;
pub mod gfx;
pub mod png;
pub mod qr;
pub mod recipes;
pub mod zx;

pub use demo::Demo;

// SAFETY (all exports below): `d` is the pointer returned by `ufo_new`, which is
// never freed, and the page is single-threaded, so it is valid and unaliased.

#[unsafe(no_mangle)]
pub extern "C" fn ufo_new() -> *mut Demo {
    Box::into_raw(Box::new(Demo::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_width() -> u32 {
    gfx::W as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_height() -> u32 {
    gfx::H as u32
}

/// Where the page writes 512 bytes of `getByteFrequencyData` each frame.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_bins(d: *mut Demo) -> *mut u8 {
    unsafe { (*d).audio.bins.as_mut_ptr() }
}

/// RGBA framebuffer, `ufo_width() * ufo_height() * 4` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_pixels(d: *mut Demo) -> *const u32 {
    unsafe { (*d).fb.px.as_ptr() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_frame(d: *mut Demo, t_ms: f64, has_audio: u32) {
    unsafe { (*d).frame((t_ms / 1000.0) as f32, has_audio != 0) }
}

/// Next part of the current demo.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_next_scene(d: *mut Demo) {
    unsafe { (*d).next_part() }
}

/// Show (or hide) the QR code to the website for 10 seconds.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_show_qr(d: *mut Demo) {
    unsafe { (*d).toggle_qr() }
}

/// Start an easter egg (see `eggs.rs` for the kinds); `arg` is the year for fireworks.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_egg(d: *mut Demo, kind: u32, arg: u32) {
    unsafe { (*d).egg(kind, arg) }
}

/// Open the show with the Christmas scene, then carry on with the current demo.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_christmas(d: *mut Demo) {
    unsafe { (*d).christmas_first() }
}

/// Start a concert (0: the half-hour rock concert, 1: the ten-minute Ukraine show, 2: the
/// eight-minute secret show, 3: Kite at Dalhalla, 87 minutes); the demos carry on when it is
/// over.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_concert(d: *mut Demo, show: u32) {
    unsafe { (*d).concert(show as usize) }
}

/// Seconds left of the concert, 0 when none is on.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_concert_left(d: *mut Demo) -> f32 {
    unsafe { (*d).concert_left() }
}

/// Snow over the demos on (1) or off (0).
#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_snow(d: *mut Demo, on: u32) {
    unsafe { (*d).eggs.snow = on != 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_toggle_hud(d: *mut Demo) {
    unsafe { (*d).toggle_hud() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_demo_count(d: *mut Demo) -> u32 {
    unsafe { (*d).count() as u32 }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_current_demo(d: *mut Demo) -> u32 {
    unsafe { (*d).current() as u32 }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_demo(d: *mut Demo, i: u32) {
    unsafe { (*d).set_demo(i as usize) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_next_demo(d: *mut Demo) {
    unsafe { (*d).next_demo() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_prev_demo(d: *mut Demo) {
    unsafe { (*d).prev_demo() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_random_demo(d: *mut Demo) {
    unsafe { (*d).random_demo() }
}

/// Seed the random choices; the page passes a random number so shuffle differs each time.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_seed(d: *mut Demo, seed: u32) {
    unsafe { (*d).seed(seed) }
}

/// 0 = next demo when one ends, 1 = repeat it, 2 = shuffle.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_jukebox(d: *mut Demo, mode: u32) {
    unsafe { (*d).jukebox = (mode % 3) as u8 }
}

/// UTF-8 list, one line per demo: "title\tgroup\tpart, part, ...".
#[unsafe(no_mangle)]
pub extern "C" fn ufo_list(d: *mut Demo) -> *const u8 {
    unsafe { (*d).list().as_ptr() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_list_len(d: *mut Demo) -> u32 {
    unsafe { (*d).list().len() as u32 }
}

/// 1024 bytes where the page writes UTF-8 "artist\ntrack" before `ufo_set_track`.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_text_buf(d: *mut Demo) -> *mut u8 {
    unsafe { (*d).text_buf.as_mut_ptr() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_track(d: *mut Demo, len: u32) {
    unsafe { (*d).set_track(len as usize) }
}

/// One UTF-8 line in `ufo_text_buf` for the concert stage's LED wall (the secret show's
/// dedication); length 0 removes it. Set it before `ufo_concert`.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_concert_note(d: *mut Demo, len: u32) {
    unsafe { (*d).set_concert_note(len as usize) }
}

/// One UTF-8 line in `ufo_text_buf` to add to the end of every scroll text; length 0 removes it.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_set_scroll_extra(d: *mut Demo, len: u32) {
    unsafe { (*d).set_scroll_extra(len as usize) }
}

/// 256×256 RGBA pixels where the page writes the artist picture before `ufo_image_ready`;
/// the page writes the picture's credit line (UTF-8) into `ufo_text_buf` and passes its length.
#[unsafe(no_mangle)]
pub extern "C" fn ufo_image_buf(d: *mut Demo) -> *mut u32 {
    unsafe { (*d).image_buf.as_mut_ptr() }
}

#[unsafe(no_mangle)]
pub extern "C" fn ufo_image_ready(d: *mut Demo, credit_len: u32) {
    unsafe { (*d).image_ready(credit_len as usize) }
}
