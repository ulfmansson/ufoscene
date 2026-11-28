// The setup guide: shown on the first visit and on G (or the panel's "Setup guide" button).
// Three choices, easiest first: just play 80s & 90s radio; your own music (a shared tab, the
// screen's sound, BlackHole or the microphone); and where song names and artist photos come
// from (Last.fm for anyone, or Spotify with your own free developer app).
// The choices live in localStorage; ufo.js reads them when Start is pressed.

import { DECADES, GENRES, DEFAULT_DECADE, DEFAULT_GENRE } from './radio.js';

const LS = {
  get: (k) => {
    try {
      return localStorage.getItem('ufo.' + k);
    } catch {
      return null;
    }
  },
  set: (k, v) => {
    try {
      localStorage.setItem('ufo.' + k, v);
    } catch {}
  },
  del: (k) => {
    try {
      localStorage.removeItem('ufo.' + k);
    } catch {}
  },
};
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

export const AUDIO = {
  radio: 'internet radio',
  tab: 'a shared Chrome tab',
  screen: 'the shared screen (system audio)',
  blackhole: 'BlackHole, or the microphone without it',
  mic: 'the microphone',
};
export const audioKind = () => LS.get('audio') || '';
export const setAudioKind = (k) => LS.set('audio', k);
export const canShare = () => !!navigator.mediaDevices?.getDisplayMedia;
/** Where song names come from: 'lastfm', 'spotify' or 'none'. */
export const songs = () => LS.get('songs') || 'none';

/**
 * api: { start(), say(text, ms), spotify (module), lastfm (module), config, lastfmKey(),
 *        songsChanged(), radioChanged(), audioChanged() }
 */
export function createGuide(api) {
  const { spotify, lastfm, config, say } = api;
  const $ = (id) => document.getElementById(id);

  // Someone who connected Spotify in the panel before the guide existed keeps Spotify.
  if (!LS.get('songs') && spotify.connected()) LS.set('songs', 'spotify');

  function songsLabel() {
    if (songs() === 'lastfm') return `Last.fm (${LS.get('lastfm.user') || '?'})`;
    if (songs() === 'spotify') return spotify.connected() ? 'Spotify ✓' : 'Spotify (not connected)';
    return 'none';
  }

  // ---------------------------------------------------------------- panel status line

  const status = $('setup-status');
  let problem = '';
  function showStatus() {
    if (!status) return;
    status.textContent = `Song names: ${songsLabel()}${problem ? ` — ${problem}` : ''}`;
  }
  $('guide-open')?.addEventListener('click', () => open(0));

  // ---------------------------------------------------------------- the overlay

  const guide = document.createElement('div');
  guide.id = 'guide';
  guide.hidden = true;
  guide.innerHTML = '<div class="box" role="dialog" aria-modal="true" aria-label="UFO setup"></div>';
  document.body.append(guide);
  const box = guide.firstElementChild;
  let step = 0;

  function opt(name, value, current, title, how, { tag = '', off = '' } = {}) {
    return `<label class="opt${off ? ' off' : ''}"><input type="radio" name="${name}" value="${value}"${value === current ? ' checked' : ''}${off ? ' disabled' : ''}>
    <b>${title}</b>${tag ? `<span class="tag">${tag}</span>` : ''}${off ? ` <span class="note">(${off})</span>` : ''}
    <div class="how">${how}</div></label>`;
  }

  // Decade and genre for the radio (the same choice as in the panel).
  function radioPicker() {
    const cur = (key, table, dflt) => (table[LS.get(key)] ? LS.get(key) : dflt);
    const sel = (id, key, table, dflt) =>
      `<select id="${id}" data-key="${key}">${Object.entries(table)
        .map(([v, [label]]) => `<option value="${v}"${v === cur(key, table, dflt) ? ' selected' : ''}>${esc(label)}</option>`)
        .join('')}</select>`;
    return `<span class="picker">Decade ${sel('g-decade', 'radio.decade', DECADES, DEFAULT_DECADE)}
      Genre ${sel('g-genre', 'radio.genre', GENRES, DEFAULT_GENRE)}</span>`;
  }

  const nav = (back = true) =>
    `<div class="nav"><button data-act="close">Close</button><span class="spacer"></span>${back ? '<button data-act="back">Back</button>' : ''}<button class="primary" data-act="next">Next</button></div>`;

  const STEPS = [
    // 0: welcome
    () => `
    <h1>UFOSCENE.NET <small>· 100 DEMOS</small></h1>
    <p class="tagline">A hymn to the Swedish demo scene</p>
    <p>Amiga-style demos that dance to music, made to be cast to a TV.</p>
    <p><b>No setup:</b> press <i>Just play</i> for internet radio — 80s &amp; 90s, or pick a decade and a
       genre: ${radioPicker()} <kbd>Space</kbd> changes the station.</p>
    <p><b>Your own music, song names and artist photos:</b> <i>Set it up</i> takes a minute
       (Last.fm or Spotify).</p>
    <p class="note">This guide comes back with <kbd>G</kbd>; the settings are under <kbd>P</kbd>.
       <a href="guides.html" target="_blank" rel="noopener">Step-by-step guides</a> for Mac, Windows, Linux, phones,
       the TV and a Raspberry Pi.</p>
    <div class="nav"><button data-act="close">Just watch</button><span class="spacer"></span>
      <button data-act="setup">Set it up</button><button class="primary" data-act="radio">Just play</button></div>`,

    // 1: music
    () => {
      const cur = audioKind() || 'radio';
      const noShare = canShare() ? '' : 'needs Chrome or Edge on a computer';
      const mac = /Mac/i.test(navigator.userAgentData?.platform || navigator.platform || '');
      return `
    <h2>1 · The music <span class="dots"><b>●</b>●●</span></h2>
    ${opt('g-audio', 'radio', cur, 'Just play: internet radio',
      `${radioPicker()}<br>
       Stations from <a href="https://www.radio-browser.info" target="_blank" rel="noopener">radio-browser.info</a>.
       No account, nothing to install. <kbd>Space</kbd> = next station, <kbd>B</kbd> = previous.
       The station's name shows instead of song names.`,
      { tag: 'no setup' })}
    ${opt('g-audio', 'tab', cur, 'Your music in a Chrome tab',
      `For example <a href="https://open.spotify.com" target="_blank" rel="noopener">open.spotify.com</a>, YouTube or SoundCloud in another tab.
       When you press Start, Chrome asks what to share: choose <b>Chrome tab</b>, pick the music tab and keep
       <b>Also share tab audio</b> on. The music then plays from this tab, so <b>Cast tab</b> takes it to the TV.`,
      { tag: 'easiest for your own music', off: noShare })}
    ${opt('g-audio', 'screen', cur, 'Your music in an app (Spotify app, Music, any player)',
      `When you press Start, choose <b>Entire screen</b> and switch on <b>Also share system audio</b>.
       ${mac ? 'On a Mac this needs macOS 14.2 or newer and a current Chrome; the first time, macOS asks you to allow Chrome under <b>Screen &amp; System Audio Recording</b> (then restart Chrome).' : 'Works in Chrome and Edge on Windows.'}
       The sound keeps playing on the computer, so a Cast TV shows the demo without the music.`,
      { off: noShare })}
    ${opt('g-audio', 'blackhole', cur, 'BlackHole loopback (Mac)',
      `Best when you cast the Spotify app's music. Install it once:
       <code>brew install --cask blackhole-2ch</code> (or from <a href="https://existential.audio/blackhole/" target="_blank" rel="noopener">existential.audio</a>),
       then System Settings → Sound → Output → <b>BlackHole 2ch</b>. Start finds it by itself and plays the
       music from this tab for Cast tab. Switch the output back when you are done.`)}
    ${opt('g-audio', 'mic', cur, 'The microphone',
      'No setup: the demos listen to the room. Loud music from speakers works well; quiet music makes them twitchy.')}
    <p class="note">Start asks for permission the first time. Without any sound the demos run on a 125 BPM autopilot.
       Windows and Linux have loopbacks too: see the <a href="guides.html#windows" target="_blank" rel="noopener">step-by-step guides</a>.</p>
    ${nav()}`;
    },

    // 2: song names and artist photos
    () => {
      const guest = config.spotifyClientId && spotify.clientId() === config.spotifyClientId;
      const cur = songs() === 'spotify' && guest ? 'guest' : songs();
      const user = esc(LS.get('lastfm.user') || '');
      const keyField = config.lastfmApiKey
        ? ''
        : `<li>Get a free Last.fm API key (a minute) at
         <a href="https://www.last.fm/api/account/create" target="_blank" rel="noopener">last.fm/api/account/create</a>
         and paste the <b>API key</b> (not the shared secret): <input type="text" id="g-lfm-key" spellcheck="false" value="${esc(LS.get('lastfm.key') || '')}"></li>`;
      const connected = spotify.connected() ? ' <span class="ok">connected ✓</span>' : '';
      const radioNote = audioKind() === 'radio'
        ? '<p class="note">With the radio, the station name is shown; this is for your own music.</p>'
        : '';
      return `
    <h2>2 · Song names and artist photos <small>(optional)</small> <span class="dots">●<b>●</b>●</span></h2>
    <p>Every new song then starts a new demo, its name dances in, and photos of the artist show up in the graphics.</p>
    ${radioNote}
    ${opt('g-songs', 'lastfm', cur, 'Last.fm — free, works for everyone',
      `<ol>
        <li>Sign in or sign up at <a href="https://www.last.fm/join" target="_blank" rel="noopener">last.fm</a>.</li>
        <li>Let Spotify tell Last.fm what you play: <a href="https://www.last.fm/settings/applications" target="_blank" rel="noopener">Settings → Applications</a>
            → Spotify Scrobbling → <b>Connect</b>. (Apple Music, YouTube Music and other players can scrobble too, with their own apps.)</li>
        <li>Keep your listening visible: <a href="https://www.last.fm/settings/privacy" target="_blank" rel="noopener">Settings → Privacy</a>,
            <i>Hide recent listening information</i> off.</li>
        ${keyField}
        <li>Your Last.fm username: <input type="text" id="g-lfm-user" spellcheck="false" autocomplete="username" value="${user}">
            <button data-act="lfm-check">Check</button> <span id="g-lfm-result"></span></li>
      </ol>
      <p class="note">Song names come a few seconds after a song starts.</p>`,
      { tag: 'recommended' })}
    ${opt('g-songs', 'spotify', cur, 'Spotify, with your own free developer app',
      `Needs Spotify Premium and about five minutes. Adds skipping songs with <kbd>Space</kbd> and playlists from a text file.
      <ol>
        <li>Open the <a href="https://developer.spotify.com/dashboard" target="_blank" rel="noopener">Spotify developer dashboard</a>,
            log in, accept the terms and press <b>Create app</b>.</li>
        <li>Any name and description. <b>Redirect URI</b>: <code id="g-redir">${esc(spotify.redirectUri())}</code>
            <button data-act="copy">Copy</button> — add it, tick <b>Web API</b>, save.</li>
        <li>Copy the app's <b>Client ID</b> (not the secret) here:
            <input type="text" id="g-cid" spellcheck="false" value="${esc(guest ? '' : spotify.clientId())}">
            <button data-act="sp-connect">Connect</button>${connected}<br>
            Spotify asks you to agree, then sends you back here.</li>
      </ol>
      <p class="note">The app stays in Spotify's development mode, where only its owner (you) can use it — all this needs.
         Once connected, the <i>ufoscene.net playlists</i> under <kbd>P</kbd> → <i>Your music in a tab</i> play on your Spotify app (Premium).</p>`)}
    ${config.spotifyClientId ? opt('g-songs', 'guest', cur, "Spotify, on this site's guest list",
      `Only if the site owner added the e-mail of your Spotify account to their app (it holds five people).
       <button data-act="guest-connect">Connect</button>${connected}`) : ''}
    ${opt('g-songs', 'none', cur, 'No song names', 'The demos run on their own and still react to the music.')}
    ${nav()}`;
    },

    // 3: ready
    () => `
    <h2>3 · Ready <span class="dots">●●<b>●</b></span></h2>
    <p>Music: <b>${esc(AUDIO[audioKind()] || 'not chosen')}</b><br>Song names: <b>${esc(songsLabel())}</b></p>
    <p><b>On the TV:</b> Chrome ⋮ → <i>Cast, save and share</i> → <i>Cast…</i> → Sources → <b>Cast tab</b>.
       Press <kbd>T</kbd> for the smoother TV mode and <kbd>F</kbd> for fullscreen.
       HDMI, AirPlay and a Raspberry Pi: <a href="guides.html#tv" target="_blank" rel="noopener">guides</a>.</p>
    <p>Keys: <kbd>Space</kbd> next song or station · <kbd>←</kbd><kbd>→</kbd> demo · <kbd>N</kbd> next part ·
       <kbd>R</kbd> random demo · <kbd>H</kbd> info · <kbd>P</kbd> settings (pauses) · <kbd>G</kbd> this guide.</p>
    <div class="nav"><button data-act="back">Back</button><button data-act="close">Close</button><button class="primary" data-act="start">Start</button></div>`,
  ];

  function render() {
    box.innerHTML = STEPS[step]();
    box.querySelector('.primary')?.focus();
  }
  function open(at = 0) {
    step = at;
    render();
    guide.hidden = false;
  }
  function close() {
    guide.hidden = true;
    LS.set('guided', '1');
    showStatus();
    api.audioChanged?.(); // the panel's sound picker shows what was chosen here
  }

  const checked = (name) => box.querySelector(`input[name="${name}"]:checked`)?.value;
  const field = (id) => box.querySelector('#' + id)?.value.trim() || '';

  function saveLastfmFields() {
    const user = field('g-lfm-user');
    if (user) LS.set('lastfm.user', user);
    if (box.querySelector('#g-lfm-key')) {
      const key = field('g-lfm-key');
      if (key) LS.set('lastfm.key', key);
      else LS.del('lastfm.key');
    }
    return user;
  }

  async function checkLastfm() {
    const out = box.querySelector('#g-lfm-result');
    const user = saveLastfmFields();
    if (!user) return (out.innerHTML = '<span class="bad">Enter your username.</span>'), false;
    if (!api.lastfmKey()) return (out.innerHTML = '<span class="bad">Paste an API key first (step above).</span>'), false;
    out.textContent = 'Asking Last.fm…';
    try {
      const t = await lastfm.recent(user, api.lastfmKey());
      if (!t) out.innerHTML = '<span class="ok">✓ Found you.</span> Nothing played yet: start a song and it shows up here.';
      else {
        out.innerHTML = `<span class="ok">✓ ${t.nowPlaying ? 'Now playing' : 'Found you. Last played'}:</span> `;
        out.append(`${t.artist} — ${t.title}`);
      }
      return true;
    } catch (e) {
      out.innerHTML = '<span class="bad"></span>';
      out.firstChild.textContent = lastfm.explain(e);
      return false;
    }
  }

  async function connectSpotify(id) {
    if (!id) return say('Paste the Client ID first.');
    spotify.setClientId(id);
    const cid = $('cid');
    if (cid) cid.value = id;
    LS.set('songs', 'spotify');
    LS.set('guide.resume', '1');
    api.songsChanged();
    try {
      await spotify.login(); // leaves for Spotify and comes back on …/callback
    } catch (e) {
      LS.del('guide.resume');
      say(e.message);
    }
  }

  box.addEventListener('change', (e) => {
    const key = e.target.dataset?.key;
    if (!key) return;
    LS.set(key, e.target.value);
    api.radioChanged?.();
  });

  box.addEventListener('click', async (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (!act) return;
    if (act === 'close') return close();
    if (act === 'back') return (step = Math.max(0, step - 1)), render();
    if (act === 'setup') return (step = 1), render();
    if (act === 'radio' || act === 'start') {
      if (act === 'radio') LS.set('audio', 'radio');
      close();
      api.start(); // still inside the click, so the browser lets radio, tab or screen sharing start
      return;
    }
    if (act === 'copy') {
      try {
        await navigator.clipboard.writeText(spotify.redirectUri());
        say('Redirect URI copied', 2000);
      } catch {
        say('Select the address and copy it by hand.', 4000);
      }
      return;
    }
    if (act === 'lfm-check') return checkLastfm();
    if (act === 'sp-connect') return connectSpotify(field('g-cid'));
    if (act === 'guest-connect') return connectSpotify(config.spotifyClientId);
    if (act !== 'next') return;

    if (step === 1) LS.set('audio', checked('g-audio') || 'radio');
    if (step === 2) {
      const pick = checked('g-songs') || 'none';
      if (pick === 'lastfm') {
        if (!(await checkLastfm())) return;
        if (spotify.connected()) {
          spotify.logout(); // Last.fm takes over; Spotify can be connected again any time
          $('connect') && ($('connect').textContent = 'Connect');
          $('spstate') && ($('spstate').textContent = '');
        }
        LS.set('songs', 'lastfm');
      } else if (pick === 'spotify' || pick === 'guest') {
        if (!spotify.connected()) return say('Press Connect first, or pick another option.', 4000);
        LS.set('songs', 'spotify');
      } else {
        LS.set('songs', 'none');
      }
      api.songsChanged();
    }
    step = Math.min(STEPS.length - 1, step + 1);
    render();
  });

  // ---------------------------------------------------------------- first visit

  /** `welcome: false` (a show's own address) skips opening the guide on a first visit. */
  async function init({ welcome = true } = {}) {
    showStatus();
    if (LS.get('guide.resume')) {
      // back from the Spotify login: ufo.js has finished it by now; show the last step
      LS.del('guide.resume');
      if (!spotify.connected()) LS.set('songs', 'none');
      api.songsChanged();
      showStatus();
      open(spotify.connected() ? 3 : 2);
    } else if (!LS.get('guided') && welcome) {
      open(0);
    }
  }

  return {
    open,
    close,
    init,
    isOpen: () => !guide.hidden,
    showStatus,
    setProblem: (text) => {
      if (text !== problem) {
        problem = text;
        showStatus();
      }
    },
  };
}
