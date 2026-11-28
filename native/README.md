# ufo-native: the demos on a Raspberry Pi, without a browser

On a ufoscene Pi the demos normally run in Chromium (cage + Chromium in kiosk mode, the TV page
from the Pi's server, `ufo.wasm` inside it). `ufo-native` runs the **same Rust demo code**
compiled for the Pi's CPU and draws it straight to the TV. No browser, no Wayland, no GPU
library, and no crates: std and a few Linux system calls.

| | Browser kiosk | ufo-native |
|---|---|---|
| Demo code | WebAssembly in Chromium | native ARM code |
| Picture | canvas → Chromium's compositor → cage → the TV | DRM/KMS dumb buffers, page-flipped on the vertical blank |
| Memory | Chromium: several hundred MB | a few MB |
| Sound | `getUserMedia` / the radio's `<audio>` → `AnalyserNode` | `parec` (or `pw-record`) from PipeWire → the same analyser, rebuilt |
| Radio | radio-browser.info stations, in the page | `mpv` plays a station list, the player listens to the output |
| Remote | `tv.js` ← `/api/events` | the same events, the same commands |

The analyser gives **the same bytes as Chromium's** `getByteFrequencyData` (Blackman window,
smoothing 0.55, -100..-30 dB), so the demos react to the music the same way. A test compares
the two (`analyser::compare`).

## On a ufoscene Pi

Every ufoscene-pi image and release since v0.2.0 carries the player for both Pi families (its CI
runs `native/build-pi.sh all`); the browser kiosk stays the default. Over SSH:

```bash
sudo /opt/ufoscene/player.sh native
```

```bash
sudo /opt/ufoscene/player.sh browser
```

The choice is kept in `/etc/ufoscene/player`, so the nightly update keeps it.

## Build (on the Mac)

```bash
/Users/ulf.mansson/IdeaProjects/ufoscene/native/build-pi.sh
```

This makes static binaries for both Pi families in `target/pi-native/`, linked with Rust's own
`rust-lld`: no cross C toolchain, Docker or Pi needed. The first run adds the Rust targets
`aarch64-unknown-linux-musl` and `armv7-unknown-linux-musleabihf` with rustup.

| File | Pi |
|---|---|
| `ufo-native-arm64` | 3, 4, 400, 5, 500, Zero 2 W with 64-bit Raspberry Pi OS |
| `ufo-native-armhf` | 2, and any Pi with 32-bit Raspberry Pi OS |

## Try a build of your own on a ufoscene Pi

```bash
scp -r /Users/ulf.mansson/IdeaProjects/ufoscene/target/pi-native <user>@ufoscene.local:
```

Then on the Pi (`ssh <user>@ufoscene.local`):

```bash
sudo ./pi-native/try-native.sh bench
```

```bash
sudo ./pi-native/try-native.sh on
```

```bash
sudo ./pi-native/try-native.sh off
```

* `bench` times 60 s of demos on this Pi's CPU (1920×1080 with scanlines) and prints the
  average, the 95th percentile and the worst frame. Nothing on the TV changes.
* `on` installs the player in `/usr/local/lib/ufoscene-native`, the systemd unit
  `ufoscene-native.service` (tty1, like the kiosk), and `mpv` for the radio. It then stops the
  browser kiosk and starts the player. Log: `journalctl -u ufoscene-native -f`.
* `off` brings the browser kiosk back.

`try-native.sh` installs outside the release, so a nightly ufoscene-pi update starts the
player the release brings (the kiosk, unless `player.sh native` was chosen). To try your own build
again, run `on` once more.

## Options

```text
ufo-native [--card /dev/dri/cardN] [--mode WxH[@Hz]] [--server HOST:PORT | --no-remote]
           [--audio radio|phone|mic|none] [--demo N] [--tv] [--no-scanlines]
           [--bench SECONDS [--png FILE] [--size WxH]]
```

* **Screen.** The first `/dev/dri/card*` with a connected screen (on a Pi 5 that is `card1`).
  The mode is 1920×1080 at 60 Hz (or 50) when the TV has it: that scales 384×216 by exactly 5,
  and a 4K TV would mean four times the pixels to write. Otherwise the TV's preferred mode, with
  whole-number scaling and black borders. `--mode 1280x720` saves memory bandwidth on a slow Pi.
* **CRT look.** The page's scanlines and vignette, drawn while scaling. `--no-scanlines` turns
  them off. TV mode (`--tv`, or **TV** on the remote) is a steady 30 fps with no scanlines, as
  on the page.
* **Sound.** As set on the remote, kept in `/var/lib/ufoscene-native/settings`:
  * **radio**: `mpv` plays the station and the player listens to the output's monitor
  * **phone** (Bluetooth or AirPlay) and **mic**: the player listens to PipeWire's default
    input, which the Pi's server switches to the output's monitor or the microphone
* **Remote.** All of `tv.js`'s commands: next, previous, random, demo N, next scene, HUD, TV
  mode, the concert and the Ukraine show, skip (= the next station on the radio), sound source,
  connect card, self-test (fps in the corner, reported to the remote), now playing from the
  phone, Bluetooth pairing, Wi-Fi code and setup screens, reload (the player exits and systemd
  starts it again). The cards are drawn in the demos' pixel font.
* **`--bench`** works anywhere, a Mac included. `--png` writes the last frame as the TV would
  show it.

## Not (yet) in the native player

* **Spotify** (OAuth in the browser) and **Last.fm** song names (HTTPS from the page). Song
  names still come from the radio (ICY titles from mpv) and from a phone over Bluetooth or
  AirPlay.
* **Artist photos** (Wikimedia Commons, covers): they would need HTTPS and a JPEG decoder.
* **The radio's decade and genre choice** from radio-browser.info: the player rotates through
  the stations in `radio.rs` (those that send the song now playing).
* **A switch on the remote** between the kiosk and the player: for now `player.sh` over SSH.

## Files

| File | What |
|---|---|
| `src/main.rs` | options, the frame loop, the remote's commands, `--bench` |
| `src/drm.rs` | DRM/KMS: connector, mode, CRTC, two dumb buffers, page flips (Linux) |
| `src/scale.rs` | 384×216 → the TV, whole-number scaling, scanlines and vignette, XRGB8888 |
| `src/analyser.rs` | `AnalyserNode.getByteFrequencyData`, checked against Chromium |
| `src/capture.rs` | `parec` / `pw-record` → a ring of the latest 1024 samples |
| `src/radio.rs` | `mpv` and the station list, song titles from its output |
| `src/remote.rs` | `/api/events` (server-sent events) and `/api/state`, HTTP/1.0 over std |
| `src/overlay.rs` | messages, cards with QR codes, fps, the Ukraine corner |
| `src/json.rs` | the little JSON the server speaks |
| `pi/` | the systemd unit and `try-native.sh` |
