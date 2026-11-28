// Glue between the browser and the Rust demos in ufo.wasm: captures audio, hands
// FFT bins to Rust, blits the framebuffer, and (optionally) tells Rust what
// Spotify is playing and hands over the artist picture.

import * as spotify from './spotify.js';
import { artistPhotos } from './pictures.js';
import { Radio, DECADES, GENRES, DEFAULT_DECADE, DEFAULT_GENRE } from './radio.js';
import * as lastfm from './lastfm.js';
import { createGuide, audioKind, setAudioKind, canShare, songs } from './guide.js';
import { initPlaylists } from './playlists.js';
import { initEggs, christmasWeek } from './eggs.js';
import { initConcerts, secretFromHash, SCHEDULE_URL } from './concerts.js';

const $ = (id) => document.getElementById(id);
const canvas = $('c');
const ctx = canvas.getContext('2d');
const panel = $('panel');
const toast = $('toast');

const { instance } = await WebAssembly.instantiateStreaming(fetch('ufo.wasm'), {});
const ex = instance.exports;
const demo = ex.ufo_new();
ex.ufo_seed?.(demo, crypto.getRandomValues(new Uint32Array(1))[0]); // a new shuffle each start
const W = ex.ufo_width();
const H = ex.ufo_height();
canvas.width = W;
canvas.height = H;
const mem = () => ex.memory.buffer;
window.ufo = { ex, demo }; // for poking at it from the console

function say(text, ms = 6000) {
  toast.textContent = text;
  toast.style.opacity = 1;
  clearTimeout(say.timer);
  say.timer = setTimeout(() => (toast.style.opacity = 0), ms);
}

// Settings of a published site (site/config.json becomes site-config.json there); none locally.
const LOCAL = /^(127\.0\.0\.1|localhost|\[::1\])$/.test(location.hostname);
const siteConfig = LOCAL ? {} : await fetch('site-config.json').then((r) => (r.ok ? r.json() : {})).catch(() => ({}));
const lastfmKey = () => siteConfig.lastfmApiKey || localStorage.getItem('ufo.lastfm.key') || '';
let guide = null; // the setup guide, made once the Spotify login is settled (below)
let refreshPlaylists = null; // the ufoscene.net playlist picker in the Spotify section (below)

// ------------------------------------------------------------------ demo list

const demos = new TextDecoder()
  .decode(new Uint8Array(mem(), ex.ufo_list(demo), ex.ufo_list_len(demo)))
  .trim()
  .split('\n')
  .map((l) => l.split('\t'));
const list = $('demos');
demos.forEach(([name, group, parts], i) => {
  const el = document.createElement('div');
  el.innerHTML = `${i + 1}. ${name} <small>${group}</small>`;
  el.title = parts;
  el.onclick = () => {
    ex.ufo_set_demo(demo, i);
    start();
  };
  list.append(el);
});
function markCurrent() {
  const cur = ex.ufo_current_demo(demo);
  [...list.children].forEach((el, i) => el.classList.toggle('on', i === cur));
  const n = cur + 1;
  if (location.hash !== `#${n}`) history.replaceState(null, '', `#${n}`);
}
// #egg-<code>-<letters>: the secret show for a donor (concerts.js). The code leaves the address
// bar at once, so a screenshot or a shared link does not give it away.
const eggHash = location.hash.startsWith('#egg-') ? location.hash : '';
if (eggHash) history.replaceState(null, '', location.pathname + location.search);
const fromHash = parseInt(location.hash.slice(1), 10);
if (fromHash >= 1 && fromHash <= demos.length) ex.ufo_set_demo(demo, fromHash - 1);
// A show's own address starts it as soon as the page is ready (see the end of this file):
// ufoscene.net/kite, /queen (or /concert) and /ukraine, or #kite, #queen, #concert, #ukraine.
const SHOW_LINKS = { kite: 'kite', queen: 'rock', concert: 'rock', ukraine: 'ukraine' };
const linkWord = (location.pathname.match(/\/(kite|queen|concert|ukraine)\/?$/i)?.[1] || location.hash.slice(1)).toLowerCase();
let showOnStart = Object.hasOwn(SHOW_LINKS, linkWord) ? SHOW_LINKS[linkWord] : null;
const showFromLink = showOnStart;

const juke = $('juke');
juke.value = localStorage.getItem('ufo.juke') || '2'; // shuffle unless the visitor chose otherwise
ex.ufo_set_jukebox(demo, Number(juke.value));
juke.onchange = () => {
  localStorage.setItem('ufo.juke', juke.value);
  ex.ufo_set_jukebox(demo, Number(juke.value));
};
// with shuffle, a visit without #N in the address starts at a random demo too
if (!(fromHash >= 1 && fromHash <= demos.length) && juke.value === '2') ex.ufo_random_demo(demo);

// ------------------------------------------------------------------ audio

let analyser = null;
let freq = null;
let audioTried = false;
let ac = null;
let loopback = false;
// Untouched stereo: Chrome otherwise records mono with voice processing.
const PLAIN = { echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: { ideal: 2 } };

// Where the page plays the music it hears. "" = the tab's default output: Cast tab sends it to
// the TV (and Chrome mutes it on the Mac). A named device (speakers, AirPlay TV, HDMI) plays it
// there. "off" plays nothing. It is never played into BlackHole, which would loop it back in.
const sink = $('sink');
const savedSink = localStorage.getItem('ufo.sink') || '';

async function fillSinks() {
  const outs = (await navigator.mediaDevices.enumerateDevices()).filter(
    (d) => d.kind === 'audiooutput' && d.deviceId !== 'default' && d.deviceId !== 'communications' && !/blackhole/i.test(d.label),
  );
  [...sink.options].slice(2).forEach((o) => o.remove());
  for (const d of outs) sink.add(new Option(d.label || 'output device', d.label));
  sink.value = [...sink.options].some((o) => o.value === savedSink) ? savedSink : '';
}

async function applySink() {
  localStorage.setItem('ufo.sink', sink.value);
  if (!ac || !(loopback || radio)) return;
  try {
    if (sink.value === 'off') await ac.setSinkId({ type: 'none' });
    else if (sink.value === '') await ac.setSinkId('');
    else {
      const d = (await navigator.mediaDevices.enumerateDevices()).find((x) => x.kind === 'audiooutput' && x.label === sink.value);
      await ac.setSinkId(d ? d.deviceId : '');
    }
  } catch (e) {
    say(`Could not switch output (${e.message})`);
  }
}
sink.onchange = applySink;

// The sound source that runs now ('' until the first Start), and the stream it listens to.
let activeKind = '';
let inputStream = null;
/** Stop the running source, so Start can switch to the one picked since. */
function stopSource() {
  radio?.audio.pause();
  inputStream?.getTracks().forEach((t) => t.stop());
  inputStream = null;
  analyser = null;
  audioTried = false;
  activeKind = '';
}

async function startAudio() {
  // the music picked in the panel or the setup guide (G); radio when nothing was picked yet
  const kind = audioKind() || 'radio';
  if (audioTried && kind === activeKind) return; // already running: Start just carries on
  if (audioTried) stopSource();
  activeKind = kind;
  // these calls start inside the user's click or key, so the browser lets them ask
  if (kind === 'radio') return startRadio();
  if ((kind === 'tab' || kind === 'screen') && canShare()) return shareAudio();
  audioTried = true;
  try {
    ac ??= new AudioContext();
    let stream = await navigator.mediaDevices.getUserMedia({ audio: PLAIN });
    // Labels are only visible after permission was granted, so look for BlackHole now.
    const devices = await navigator.mediaDevices.enumerateDevices();
    const loop = devices.find((d) => d.kind === 'audioinput' && /blackhole/i.test(d.label));
    if (loop) {
      const current = stream.getAudioTracks()[0].getSettings().deviceId;
      if (current !== loop.deviceId) {
        stream.getTracks().forEach((t) => t.stop());
        stream = await navigator.mediaDevices.getUserMedia({ audio: { ...PLAIN, deviceId: { exact: loop.deviceId } } });
      }
      loopback = true;
    }
    inputStream = stream;
    const src = ac.createMediaStreamSource(stream);
    analyser = ac.createAnalyser();
    analyser.fftSize = 1024;
    analyser.smoothingTimeConstant = 0.55;
    freq = new Uint8Array(analyser.frequencyBinCount);
    src.connect(analyser);
    await fillSinks();
    if (loopback) {
      src.connect(ac.destination);
      await applySink();
      const where = sink.value === '' ? 'this tab — cast it with ⋮ → Cast… → Cast tab' : sink.selectedOptions[0].text;
      const def = (await navigator.mediaDevices.enumerateDevices()).find((d) => d.kind === 'audiooutput' && d.deviceId === 'default');
      if (sink.value === '' && /blackhole/i.test(def?.label || '')) {
        say('Music loops back into BlackHole until you cast this tab. Cast it now, or press P and pick your speakers under "Play the music on".', 12000);
      } else say(`Listening to ${loop.label}. Music plays on: ${where}. Change it with P → "Play the music on".`, 9000);
    } else {
      const label = stream.getAudioTracks()[0].label || 'microphone';
      say(`BlackHole not found — listening to ${label}. See README for loopback setup.`);
    }
    if (ac.state === 'suspended') await ac.resume();
  } catch (err) {
    say(`No audio (${err.name}). Running on autopilot at 125 BPM.`);
  }
}

// No-install alternative to BlackHole: share a Chrome tab that plays the music (such as
// open.spotify.com) or the entire screen with its system audio (Windows; macOS 14.2+ with a
// recent Chrome). Chrome only offers audio together with video, so ask for a tiny video track.
async function shareAudio() {
  audioTried = true; // do not also open the microphone
  try {
    const stream = await navigator.mediaDevices.getDisplayMedia({
      video: { frameRate: 1, width: 320 },
      audio: { ...PLAIN, suppressLocalAudioPlayback: true },
      systemAudio: 'include',
      selfBrowserSurface: 'exclude',
    });
    const track = stream.getAudioTracks()[0];
    const surface = stream.getVideoTracks()[0]?.getSettings().displaySurface;
    if (!track) {
      stream.getTracks().forEach((t) => t.stop());
      audioTried = false; // the next Start asks again
      activeKind = '';
      say('No sound was shared. Pick a tab or Entire screen and switch on its "share audio" option.', 10000);
      return;
    }
    inputStream = stream;
    ac ??= new AudioContext();
    if (ac.state === 'suspended') await ac.resume();
    const src = ac.createMediaStreamSource(new MediaStream([track]));
    analyser = ac.createAnalyser();
    analyser.fftSize = 1024;
    analyser.smoothingTimeConstant = 0.55;
    freq = new Uint8Array(analyser.frequencyBinCount);
    src.connect(analyser);
    // A shared tab goes quiet (suppressLocalAudioPlayback), so play it from this tab instead:
    // Cast tab then takes the music to the TV. Screen audio is already playing on the computer.
    if (surface === 'browser') src.connect(ac.destination);
    track.onended = () => {
      analyser = null;
      audioTried = false; // the next Start asks again
      activeKind = '';
      say('Audio sharing stopped. Running on autopilot at 125 BPM.');
    };
    say(surface === 'browser' ? 'Listening to the shared tab. Its music now plays from this tab.' : 'Listening to the computer\'s sound.', 8000);
  } catch (err) {
    audioTried = false; // the next Start asks again
    activeKind = '';
    say(`No audio shared (${err.name}). Running on autopilot at 125 BPM.`);
  }
}

// The settings panel (P) pauses everything: the demo stands still, the radio stops, the sound
// this tab plays goes quiet, and Spotify pauses (Premium). Closing the panel carries on.
let paused = false;
let pausedAt = 0;
let pausedTotal = 0; // ms the demo stood still, taken off its clock so parts keep their length
let spotifyPausedByUs = false;
function setPaused(on) {
  if (on === paused) return;
  paused = on;
  if (on) {
    pausedAt = performance.now();
    radio?.audio.pause();
    ac?.suspend().catch(() => {});
    if (songs() === 'spotify' && spotify.connected()) {
      spotify.pause().then(() => (spotifyPausedByUs = true)).catch(() => {});
    }
  } else {
    pausedTotal += performance.now() - pausedAt;
    ac?.resume().catch(() => {});
    if (activeKind === 'radio' && radio?.i >= 0) radio.audio.play().catch(() => {});
    if (spotifyPausedByUs) {
      spotifyPausedByUs = false;
      spotify.resume().catch(() => {});
    }
  }
}

// Now and then, while a demo runs: where the settings are.
const touchScreen = matchMedia('(pointer: coarse)').matches;
let hintTimer = null;
function scheduleHint(ms) {
  clearTimeout(hintTimer);
  hintTimer = setTimeout(() => {
    // never over another message (a station or song name): try again a little later
    if (toast.style.opacity === '1') return scheduleHint(10000);
    if (panel.style.display === 'none' && !guide?.isOpen()) say(touchScreen ? 'Tap the screen for settings' : 'Press P for settings · G for the setup guide', 4000);
    scheduleHint(4 * 60 * 1000);
  }, ms);
}

let christmasShown = false;
function start() {
  // restart the chosen demo so its title card shows
  if (panel.style.display !== 'none' && !paused) ex.ufo_set_demo(demo, ex.ufo_current_demo(demo));
  // from 23 to 28 December, the first start of the evening opens with the Christmas scene
  if (!christmasShown && christmasWeek(new Date())) ex.ufo_christmas(demo);
  christmasShown = true;
  const show = showOnStart;
  if (show) {
    showOnStart = null;
    beginShow(show);
  }
  panel.style.display = 'none';
  document.body.classList.add('running');
  setPaused(false);
  scheduleHint(12000);
  // a show with a setlist brings its own music (on Spotify, or what the visitor puts on): the
  // tab's radio would play over it and its station messages would hide the show's. A shared
  // tab or screen can only be asked for in a click, so a show opened by its link leaves that
  // to the next Start. The secret show has no setlist: whatever plays carries on.
  const kind = audioKind() || 'radio';
  const needsClick = (kind === 'tab' || kind === 'screen') && !(navigator.userActivation?.isActive ?? true);
  const ownMusic = show && SHOWS[show].setlist;
  if (!(ownMusic && kind === 'radio') && !(show && needsClick)) startAudio();
}
function togglePanel() {
  const show = panel.style.display === 'none';
  panel.style.display = show ? '' : 'none';
  document.body.classList.toggle('running', !show);
  setPaused(show);
  if (show) list.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
}

$('go').addEventListener('click', start);

// ------------------------------------------------------------------ the concert

// Shows on a pixel stage (demo/src/fx/concert.rs), then the demos carry on:
// C: half an hour of rock concert, Queen's songs (the setlist in playlists/concert.txt).
// U: ten minutes of Ukraine's three Eurovision winners (playlists/ukraine.txt).
// K: Kite at Dalhalla, as long as the record Live At Dalhalla (playlists/kite-dalhalla.txt).
// Also the panel's buttons, the shows' addresses (ufoscene.net/kite, /queen, /ukraine, or
// #kite …), or ufo.concert('kite'). The secret show has no key: a donor's secret link or a
// booked public concert opens it (concerts.js), with a dedication on the LED wall.
// With Spotify connected (Premium) the show's music starts in order on the Spotify app, and
// every new song brings the next act. The same key again ends the show.
const SHOWS = {
  rock: { id: 0, setlist: 'playlists/concert.txt', key: 'C', name: 'Concert' },
  ukraine: { id: 1, setlist: 'playlists/ukraine.txt', key: 'U', name: 'Ukraine at Eurovision' },
  secret: { id: 2, setlist: null, key: '', name: 'Secret show' },
  kite: { id: 3, setlist: 'playlists/kite-dalhalla.txt', key: 'K', name: 'Kite at Dalhalla' },
};
let secretOpen = false; // a right secret link, or a public concert, opened it on this page
let showNote = ''; // the dedication for the next show's LED wall
let currentShow = null;
const inConcert = () => ex.ufo_concert_left(demo) > 0;

async function playSetlist(show) {
  const { setlist, key, name } = SHOWS[show];
  if (!setlist) {
    say(`${name}! Put some music on. Thank you for supporting Ukraine's defence 💙💛`, 9000);
    return;
  }
  if (!spotify.connected()) {
    say(`${name} on! Put the music on, or connect Spotify (G) to start the setlist from here.`, 9000);
    return;
  }
  say(`${name} on! Finding the setlist in Spotify…`, 5000);
  try {
    const text = await (await fetch(setlist)).text();
    const { name: title, songs, album } = spotify.parsePlaylist(text, name);
    // a setlist that is a whole record (a live album): play the record itself, in order
    const artist = album && songs[0]?.split(/\s+[-–—]\s+/)[0];
    const record = artist && (await spotify.findAlbum(artist, album).catch(() => null));
    if (record) {
      await spotify.play(record.uri, { shuffle: false });
      say(`${record.label}: the whole record, in order. Enjoy the show!`, 6000);
      return;
    }
    const hits = (await Promise.all(songs.map((s) => spotify.findTrack(s).catch(() => null)))).filter(Boolean);
    if (!hits.length) throw new Error('none of its songs were found');
    await spotify.play(hits.map((h) => h.uri), { shuffle: false });
    say(`${title}: ${hits.length} songs, in order. Enjoy the show!`, 6000);
  } catch (e) {
    if (e.status === 404) say(`Open the Spotify app (or open.spotify.com) and play anything once, then press ${key} twice.`, 9000);
    else if (e.status === 403) say('Starting songs from here needs Spotify Premium. Start the music in Spotify yourself.', 9000);
    else if (e.status === 409) say(`The setlist is loaded in ${e.device || 'Spotify'}, but Spotify did not start it. Press play there once.`, 10000);
    else say(`Could not start the setlist (${e.message}).`, 8000);
  }
}

function beginShow(show) {
  currentShow = show;
  if (ex.ufo_set_concert_note) {
    const bytes = new TextEncoder().encode(show === 'secret' ? showNote : '').slice(0, 1024);
    new Uint8Array(mem(), ex.ufo_text_buf(demo), 1024).set(bytes);
    ex.ufo_set_concert_note(demo, bytes.length);
  }
  ex.ufo_concert(demo, SHOWS[show].id);
  // a show with a setlist brings its own music: stop the tab's radio, which would play over it,
  // and the song the panel paused must not come back on over the setlist (start() resumes it).
  // The secret show has no setlist, so whatever plays keeps playing.
  if (SHOWS[show].setlist) {
    if (activeKind === 'radio') stopSource();
    if (spotify.connected()) spotifyPausedByUs = false;
  }
  playSetlist(show);
}

/** Start `show` ('rock', 'ukraine', 'kite', or 'secret' once unlocked); the same show again ends it. */
function concert(show = 'rock') {
  if (!SHOWS[show] || (show === 'secret' && !secretOpen)) return;
  if (inConcert() && currentShow === show) {
    ex.ufo_set_demo(demo, ex.ufo_current_demo(demo));
    say(`${SHOWS[show].name} over: back to the demos`, 3000);
    return;
  }
  // from the panel: Start closes it and begins the show, without its usual radio
  if (panel.style.display !== 'none') {
    showOnStart = show;
    start();
    return;
  }
  showOnStart = null; // started here, so Start must not start it again
  beginShow(show);
}
window.ufo.concert = concert;

// The secret link: Start opens the secret show, with the donor's letters on the stage.
if (eggHash) {
  secretFromHash(eggHash).then((letters) => {
    if (letters === null) {
      say('That secret link did not work. Copy the whole link from pi.ufoscene.net.', 9000);
      return;
    }
    secretOpen = true;
    showNote = letters ? `THANK YOU ${letters}` : 'THANK YOU';
    if (panel.style.display === 'none') beginShow('secret'); // already started
    else showOnStart = 'secret';
    say('🛸 The secret show is unlocked. Press Start!', 9000);
  });
}
$('concert-go').addEventListener('click', () => concert('rock'));
$('ukraine-go').addEventListener('click', () => concert('ukraine'));
$('kite-go').addEventListener('click', () => concert('kite'));

// Share: the system share sheet where there is one, otherwise copy the link. A page on this
// computer (127.0.0.1) shares https://ufoscene.net instead, since nobody else can open it.
const SITE = 'https://ufoscene.net/';
$('show-qr').addEventListener('click', () => {
  start();
  ex.ufo_show_qr(demo);
});
$('share-ufo').addEventListener('click', async () => {
  const local = /^(127\.0\.0\.1|localhost|\[::1\])$/.test(location.hostname);
  const url = local ? SITE : `${location.origin}${location.pathname.replace(/(callback|kite|queen|concert|ukraine)\/?$/i, '')}`;
  const data = { title: 'ufoscene.net — a hymn to the Swedish demo scene', text: '100 Amiga-style demos that dance to your music #ufoscene', url };
  try {
    if (navigator.share) await navigator.share(data);
    else {
      await navigator.clipboard.writeText(url);
      say(`Link copied: ${url}`, 4000);
    }
  } catch (e) {
    if (e.name !== 'AbortError') say(`Could not share (${e.message}). The link: ${url}`, 8000);
  }
});
// No setup at all: internet radio, by decade and genre (80s & 90s by default).
// Space / B change station.
let radio = null;
const decadeSel = $('decade');
const genreSel = $('genre');
const radioChoice = () => ({
  decade: DECADES[localStorage.getItem('ufo.radio.decade')] ? localStorage.getItem('ufo.radio.decade') : DEFAULT_DECADE,
  genre: GENRES[localStorage.getItem('ufo.radio.genre')] ? localStorage.getItem('ufo.radio.genre') : DEFAULT_GENRE,
});
const radioLabel = ({ decade, genre }) =>
  genre === 'any' ? `${DECADES[decade][0]} radio` : `${DECADES[decade][0]} ${GENRES[genre][0]} radio`;
for (const [id, [label]] of Object.entries(DECADES)) decadeSel.add(new Option(label, id));
for (const [id, [label]] of Object.entries(GENRES)) genreSel.add(new Option(label, id));
/** After the decade or genre changed (here or in the guide): show it, and retune if the radio plays. */
function radioChanged() {
  const c = radioChoice();
  decadeSel.value = c.decade;
  genreSel.value = c.genre;
  if (activeKind === 'radio' && radio && radio.i >= 0) {
    say(`Tuning in ${radioLabel(c)} …`, 9000);
    radio.choose(c.decade, c.genre).catch((e) => say(`No station found for ${radioLabel(c)} (${e.message}).`, 8000));
  }
}
decadeSel.onchange = () => {
  localStorage.setItem('ufo.radio.decade', decadeSel.value);
  radioChanged();
};
genreSel.onchange = () => {
  localStorage.setItem('ufo.radio.genre', genreSel.value);
  radioChanged();
};
radioChanged();
async function startRadio() {
  audioTried = true; // do not also open the microphone
  try {
    ac ??= new AudioContext();
    if (ac.state === 'suspended') await ac.resume();
    if (!radio) {
      radio = new Radio(ac, (st) => {
        sendTrack(st.name, 'RADIO - SPACE FOR NEXT STATION');
        // a new station brings a new demo: the next one, or a random one with shuffle (J)
        if (follow.checked && !inConcert()) {
          if (juke.value === '2') ex.ufo_random_demo(demo);
          else ex.ufo_next_demo(demo);
        }
        firstSong = true; // the station's first song keeps the demo the station just got
        say(`Radio: ${st.name}. Space = next station, B = previous.`, 6000);
      });
      // Stations that send the song: dancing artist and title, a new demo per song,
      // artist photos from Wikimedia Commons and the cover from iTunes.
      radio.onSong = async (artist, title) => {
        const main = artist.split(/\s+(?:feat\.?|ft\.?|featuring|x|vs\.?|with)\s+/i)[0];
        const art = await lastfm.cover(artist, title).catch(() => null);
        const cover = art ? { url: art, credit: 'COVER: ITUNES' } : null;
        await songStarted(artist, title, main, null, cover);
      };
      radio.analyser = ac.createAnalyser();
      radio.analyser.fftSize = 1024;
      radio.analyser.smoothingTimeConstant = 0.55;
      radio.node.connect(radio.analyser);
      radio.node.connect(ac.destination);
      await fillSinks();
      await applySink();
    }
    analyser = radio.analyser;
    freq = new Uint8Array(analyser.frequencyBinCount);
    const c = radioChoice();
    // the first time, or after another choice: that list; back from another source: the same station
    if (radio.i < 0 || radio.decade !== c.decade || radio.genre !== c.genre) {
      say(`Tuning in ${radioLabel(c)} …`, 9000);
      await radio.choose(c.decade, c.genre);
    } else await radio.audio.play();
  } catch (e) {
    say(`No radio station could be played (${e.message}). Running on autopilot.`, 8000);
  }
}

// The sound picker: Radio, a shared tab, or whatever the setup guide chose (microphone,
// BlackHole, an app's sound). The choice is the guide's own (localStorage ufo.audio), so the two
// always agree; Start (or Space) then switches to it.
const soundInputs = [...document.querySelectorAll('#sound input')];
const OTHER = { screen: 'App sound', blackhole: 'BlackHole', mic: 'Microphone' };
function showSound() {
  const kind = audioKind() || 'radio';
  const other = kind in OTHER;
  for (const i of soundInputs) i.checked = i.value === (other ? 'other' : kind);
  $('sound-other').querySelector('span').textContent = other ? `${OTHER[kind]} (G)` : 'Microphone… (G)';
  const tab = $('sound-tab').querySelector('input');
  tab.disabled = !canShare();
  $('sound-tab').title = canShare() ? '' : 'Needs Chrome or Edge on a computer';
  $('radio-opts').hidden = kind !== 'radio';
  $('tab-opts').hidden = kind !== 'tab';
}
for (const i of soundInputs) {
  i.onchange = () => {
    if (i.value === 'other') {
      showSound(); // stays on the current choice until the guide picks another
      guide?.open(1);
      return;
    }
    setAudioKind(i.value);
    showSound();
    guide?.showStatus();
  };
}
showSound();
// easter eggs (see eggs.js); its key codes listen before the keys below
window.ufo.egg = initEggs({
  start: (kind, arg) => ex.ufo_egg(demo, kind, arg),
  snow: (on) => ex.ufo_set_snow(demo, on ? 1 : 0),
  isRunning: () => panel.style.display === 'none' && !guide?.isOpen(),
});
window.ufo.christmas = () => ex.ufo_christmas(demo); // to see the Christmas scene any day
// Public concerts booked on pi.ufoscene.net play here at their time (concerts.js).
initConcerts({
  url: siteConfig.concertsUrl ?? SCHEDULE_URL,
  play: (note) => {
    secretOpen = true;
    showNote = note;
    if (!(inConcert() && currentShow === 'secret')) beginShow('secret');
  },
  isRunning: () => panel.style.display === 'none' && !guide?.isOpen(),
  say,
});
// A phone has no P key: on a touch screen a tap on the running demo opens the settings and a
// double tap goes fullscreen. The tap waits a moment, so a double tap doesn't open them as well.
let touchTap = false;
let tapTimer = null;
canvas.addEventListener('pointerdown', (e) => (touchTap = e.pointerType !== 'mouse'));
canvas.addEventListener('click', () => {
  if (!touchTap || panel.style.display !== 'none' || guide?.isOpen()) return;
  if (tapTimer) {
    clearTimeout(tapTimer);
    tapTimer = null;
    document.documentElement.requestFullscreen?.().catch(() => {});
    return;
  }
  tapTimer = setTimeout(() => {
    tapTimer = null;
    if (panel.style.display === 'none') togglePanel();
  }, 300);
});
canvas.addEventListener('dblclick', () => {
  if (!touchTap) document.documentElement.requestFullscreen();
});
window.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
  const k = e.key.toLowerCase();
  if (guide?.isOpen()) {
    // the guide is on top: its buttons take Space and Enter, Escape closes it
    if (k === 'escape') guide.close();
    return;
  }
  const running = panel.style.display === 'none';
  if (activeKind === 'radio' && radio && ((k === ' ' && running) || k === 'b')) (k === 'b' ? radio.prev() : radio.next()).catch(() => {});
  else if ((k === ' ' && running) || k === 'b') nextSong(k === 'b');
  else if (k === ' ' || k === 'enter') start();
  else if (k === 'f') {
    if (document.fullscreenElement) document.exitFullscreen();
    else document.documentElement.requestFullscreen();
  } else if (k === 'n') ex.ufo_next_scene(demo);
  else if (k === 'h') ex.ufo_toggle_hud(demo);
  else if (k === 'q') ex.ufo_show_qr(demo);
  else if (k === 'c') concert('rock');
  else if (k === 'u') concert('ukraine'); // U-F-O also starts the flyby egg
  else if (k === 'k') concert('kite');
  else if (k === 't') {
    setTv(!tvmode.checked);
    say(tvmode.checked ? 'TV mode on: steady 30 fps, no scanlines' : 'TV mode off: 60 fps with scanlines', 2500);
  }
  else if (k === 'arrowright') ex.ufo_next_demo(demo);
  else if (k === 'arrowleft') ex.ufo_prev_demo(demo);
  else if (k === 'r') ex.ufo_random_demo(demo);
  else if (k === 'p' || k === 'escape') togglePanel();
  else if (k === 'g') guide?.open(0);
  else if (k === 'j') {
    juke.value = String((Number(juke.value) + 1) % 3);
    juke.onchange();
    say(`After a demo: ${juke.selectedOptions[0].text}`, 2500);
  } else return;
  e.preventDefault();
});

// ------------------------------------------------------------------ Spotify

const spstate = $('spstate');
const result = $('result');
const follow = $('follow');
const pics = $('pics');
follow.checked = localStorage.getItem('ufo.follow') !== '0';
pics.checked = localStorage.getItem('ufo.pics') !== '0';
follow.onchange = () => localStorage.setItem('ufo.follow', follow.checked ? '1' : '0');
pics.onchange = () => localStorage.setItem('ufo.pics', pics.checked ? '1' : '0');

function showSpotifyState() {
  spstate.textContent = spotify.connected() ? 'connected ✓' : 'not connected';
  $('connect').textContent = spotify.connected() ? 'Disconnect' : 'Connect…';
  refreshPlaylists?.();
}
if (location.hostname === 'localhost') {
  spstate.textContent = `Open http://127.0.0.1:${location.port}/ instead — Spotify only accepts 127.0.0.1`;
}

// Connecting (Client ID, redirect URI) is explained in the setup guide's step 2.
$('connect').onclick = () => {
  if (!spotify.connected()) return guide?.open(2);
  spotify.logout();
  showSpotifyState();
};

// Space while a demo runs: next song in Spotify (B: previous). The song change then
// brings a new demo through the "now playing" watcher as usual.
async function nextSong(back) {
  if (!spotify.connected()) {
    say('Connect Spotify in the setup guide (G) to skip songs with Space.', 5000);
    return;
  }
  try {
    await spotify.skip(back);
    say(back ? 'Previous song' : 'Next song', 1500);
  } catch (e) {
    say(e.status === 403 ? 'Spotify only allows skipping songs from other apps with Premium.' : `Could not skip (${e.message})`, 7000);
  }
}

function sendTrack(artist, title) {
  // (also used from the console to test: ufo.sendTrack('ARTIST', 'SONG'))
  const bytes = new TextEncoder().encode(`${artist}\n${title}`).slice(0, 1024);
  new Uint8Array(mem(), ex.ufo_text_buf(demo), 1024).set(bytes);
  ex.ufo_set_track(demo, bytes.length);
}

window.ufo.sendTrack = (a, t) => sendTrack(a, t);
// Hand a 256×256 RGBA picture to the demo, with its credit line (photographer, licence, source).
function sendPixels(px, credit = '') {
  new Uint8Array(mem(), ex.ufo_image_buf(demo), 256 * 256 * 4).set(px);
  const bytes = new TextEncoder().encode(credit).slice(0, 1024);
  new Uint8Array(mem(), ex.ufo_text_buf(demo), 1024).set(bytes);
  ex.ufo_image_ready(demo, bytes.length);
}
window.ufo.sendPixels = sendPixels;

/** `pic` is { url, credit }. */
async function sendPicture(pic) {
  if (!pic?.url || !pics.checked) return;
  try {
    sendPixels(await spotify.picture(pic.url), pic.credit || '');
  } catch (e) {
    console.warn('picture failed', e);
  }
}

// Pictures for the song now playing: Spotify's artist photo first, then more photos of the
// artist from Wikimedia Commons, then the album cover, one every PICTURE_EVERY ms. Each new
// picture gets its own picture part at the next part change.
const PICTURE_EVERY = 45000;
let pictureTimer = null;
let songToken = 0;
/** `first` and `album` are { url, credit } or null. */
async function showPictures(mainArtist, first, album) {
  const token = ++songToken;
  clearTimeout(pictureTimer);
  if (!pics.checked) return;
  const list = [first].filter(Boolean);
  if (first) await sendPicture(first);
  const more = mainArtist ? await artistPhotos(mainArtist) : [];
  if (token !== songToken) return; // the song changed meanwhile
  list.push(...more);
  if (album && !list.some((p) => p.url === album.url)) list.push(album);
  if (!first && list.length) await sendPicture(list[0]);
  let i = 0;
  const next = () => {
    if (token !== songToken || list.length < 2) return;
    i = (i + 1) % list.length;
    sendPicture(list[i]);
    pictureTimer = setTimeout(next, PICTURE_EVERY);
  };
  pictureTimer = setTimeout(next, PICTURE_EVERY);
}

// Try it without Spotify from the console: ufo.showArtist('Roxette', 'Joyride')
window.ufo.showArtist = (artist, title = '') => {
  sendTrack(artist, title);
  return showPictures(artist, null, null);
};

// A new song (from Spotify or Last.fm): its name, a new demo, the artist's photos.
let firstSong = true;
async function songStarted(artist, title, mainArtist, first, album) {
  sendTrack(artist, title);
  // the first song keeps the demo that is already running; the concert keeps its stage
  if (follow.checked && !firstSong && !inConcert()) {
    if (juke.value === '2') ex.ufo_random_demo(demo);
    else ex.ufo_next_demo(demo);
  }
  firstSong = false;
  await showPictures(mainArtist, first, album);
}

function startWatching() {
  spotify.watch(
    async (song) => {
      // Spotify asks apps to credit Spotify for its images; Commons photos carry their own credit.
      const { artistImage: a, albumImage: c } = song;
      const first = a ? { url: a, credit: a === c ? 'COVER: SPOTIFY' : 'IMAGE: SPOTIFY' } : null;
      const album = c ? { url: c, credit: 'COVER: SPOTIFY' } : null;
      await songStarted(song.artist, song.title, song.mainArtist, first, album);
    },
    (e) => {
      if (e.status === 403) spstate.textContent = `Spotify refused: ${e.message}`;
      console.warn('spotify', e);
    },
  );
}

try {
  const msg = await spotify.handleCallback();
  if (msg) say(msg);
} catch (e) {
  say(`Spotify login failed: ${e.message}`, 10000);
}
showSpotifyState();
if (spotify.connected()) startWatching();

// Last.fm "now playing" (chosen in the guide), for anyone without Spotify. Spotify wins when it
// is connected: two watchers would fight over the demo.
let stopLastfm = null;
function startLastfm() {
  stopLastfm?.();
  stopLastfm = null;
  guide?.setProblem('');
  const user = localStorage.getItem('ufo.lastfm.user');
  if (songs() !== 'lastfm' || !user || !lastfmKey() || spotify.connected()) return;
  stopLastfm = lastfm.watch(
    user,
    lastfmKey(),
    (t) => {
      const cover = t.image ? { url: t.image, credit: `COVER: ${t.imageSource || 'LAST.FM'}` } : null;
      songStarted(t.artist, t.title, t.artist, null, cover);
    },
    (e) => guide?.setProblem(e ? lastfm.explain(e) : ''),
  );
}

guide = createGuide({
  start,
  say,
  spotify,
  lastfm,
  config: siteConfig,
  lastfmKey,
  radioChanged,
  audioChanged: showSound,
  songsChanged: () => {
    startLastfm();
    showSpotifyState();
  },
});
startLastfm();
// a show's own address goes straight into the show, without the welcome
guide.init({ welcome: !showFromLink });
refreshPlaylists = initPlaylists({ spotify, say, container: $('playlists'), onStart: () => start() });
if (showOnStart && !guide.isOpen()) start();

async function playlistFrom(text, fallbackName) {
  if (!spotify.connected()) {
    result.textContent = 'Connect to Spotify first.';
    return;
  }
  const { name, songs, public: isPublic, description } = spotify.parsePlaylist(text, fallbackName);
  if (!songs.length) {
    result.textContent = 'No songs in that file.';
    return;
  }
  result.textContent = `Looking up ${songs.length} songs…`;
  try {
    const pl = await spotify.makePlaylist(name, songs, (i, n) => (result.textContent = `Looking up songs: ${i}/${n}`), {
      public: isPublic,
      description,
    });
    result.innerHTML = '';
    const head = document.createElement('div');
    head.innerHTML = `Created <b></b> with ${pl.found.length} songs. <a target="_blank">Open in Spotify</a> <button>Play it now</button>`;
    head.querySelector('b').textContent = name;
    head.querySelector('a').href = pl.url || '#';
    head.querySelector('button').onclick = async () => {
      try {
        await spotify.play(pl.uri);
        say('Playing — set Sound output to BlackHole if you have not already.');
      } catch (e) {
        say(`Could not start it (${e.message}). Start the playlist in Spotify instead.`, 9000);
      }
    };
    result.append(head);
    if (pl.missing.length) {
      const miss = document.createElement('div');
      miss.textContent = `Not found (${pl.missing.length}):\n  ` + pl.missing.join('\n  ');
      result.append(miss);
    }
  } catch (e) {
    result.textContent = `Could not make the playlist: ${e.message}`;
  }
}

$('file').onchange = async (e) => {
  const f = e.target.files[0];
  if (f) await playlistFrom(await f.text(), f.name.replace(/\.[^.]+$/, ''));
  e.target.value = '';
};
$('builtin').onclick = async () => {
  const text = await (await fetch('playlist.txt')).text();
  await playlistFrom(text, 'UFO demo mix');
};

// ------------------------------------------------------------------ frames

// TV mode: Cast tab films the tab at about 30 fps and compresses it like video. A steady
// 30 fps (instead of 60 with frames landing unevenly) and no fine scanline overlay give the
// encoder an easier job, so the TV stutters much less.
const tvmode = $('tvmode');
function setTv(on) {
  tvmode.checked = on;
  document.body.classList.toggle('tv', on);
  localStorage.setItem('ufo.tv', on ? '1' : '0');
}
setTv(localStorage.getItem('ufo.tv') === '1');
tvmode.onchange = () => setTv(tvmode.checked);
const STEP = 1000 / 30;
let due = 0;

let shown = -1;
function frame(now) {
  if (tvmode.checked) {
    if (now < due - 2) {
      requestAnimationFrame(frame);
      return;
    }
    // keep a fixed 30 fps grid; resync if we fell far behind (tab was hidden)
    due = now - due > 100 ? now + STEP : due + STEP;
  }
  if (paused) {
    // the panel is open: keep the last picture
    requestAnimationFrame(frame);
    return;
  }
  if (analyser) {
    analyser.getByteFrequencyData(freq);
    new Uint8Array(mem(), ex.ufo_bins(demo), 512).set(freq.subarray(0, 512));
  }
  ex.ufo_frame(demo, now - pausedTotal, analyser ? 1 : 0);
  // Views must be recreated each frame: the wasm memory buffer can be replaced when it grows.
  const px = new Uint8ClampedArray(mem(), ex.ufo_pixels(demo), W * H * 4);
  ctx.putImageData(new ImageData(px, W, H), 0, 0);
  const cur = ex.ufo_current_demo(demo);
  if (cur !== shown) {
    shown = cur;
    markCurrent();
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
