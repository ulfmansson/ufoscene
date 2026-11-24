// Spotify Web API from the browser: PKCE login (no client secret), "now playing"
// polling with artist pictures, and building a playlist from a text file.
//
// The app is registered by the user at developer.spotify.com with the redirect
// URI `${location.origin}/callback`; only its Client ID is needed here.

const AUTH = 'https://accounts.spotify.com/authorize';
const TOKEN = 'https://accounts.spotify.com/api/token';
const API = 'https://api.spotify.com/v1';
const SCOPES = [
  'user-read-currently-playing',
  'user-read-playback-state',
  'user-modify-playback-state',
  'playlist-modify-private',
  'playlist-modify-public',
].join(' ');

const store = {
  get: (k) => localStorage.getItem('ufo.spotify.' + k),
  set: (k, v) => localStorage.setItem('ufo.spotify.' + k, v),
  del: (k) => localStorage.removeItem('ufo.spotify.' + k),
};

export const redirectUri = () => new URL('callback', location.href).href;
export const clientId = () => store.get('client') || '';
export const setClientId = (id) => store.set('client', id.trim());
export const connected = () => !!store.get('refresh');

export function logout() {
  ['access', 'refresh', 'expires'].forEach(store.del);
}

const b64url = (buf) =>
  btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

export async function login() {
  if (!clientId()) throw new Error('Enter your Client ID first');
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(48)));
  const challenge = b64url(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier)));
  const state = b64url(crypto.getRandomValues(new Uint8Array(12)));
  store.set('verifier', verifier);
  store.set('state', state);
  const q = new URLSearchParams({
    response_type: 'code',
    client_id: clientId(),
    scope: SCOPES,
    redirect_uri: redirectUri(),
    code_challenge_method: 'S256',
    code_challenge: challenge,
    state,
  });
  location.assign(`${AUTH}?${q}`);
}

function saveTokens(j) {
  store.set('access', j.access_token);
  store.set('expires', String(Date.now() + (j.expires_in - 60) * 1000));
  if (j.refresh_token) store.set('refresh', j.refresh_token);
}

async function tokenRequest(params) {
  const r = await fetch(TOKEN, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ client_id: clientId(), ...params }),
  });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error_description || j.error || `token request failed (${r.status})`);
  saveTokens(j);
}

/** Finish the login if Spotify just sent us back here. Returns a message or null. */
export async function handleCallback() {
  if (!location.pathname.endsWith('/callback')) return null;
  const q = new URLSearchParams(location.search);
  history.replaceState(null, '', './');
  if (q.get('error')) return `Spotify login cancelled (${q.get('error')})`;
  if (!q.get('code') || q.get('state') !== store.get('state')) return 'Spotify login failed: bad state';
  await tokenRequest({
    grant_type: 'authorization_code',
    code: q.get('code'),
    redirect_uri: redirectUri(),
    code_verifier: store.get('verifier') || '',
  });
  store.del('verifier');
  store.del('state');
  return 'Connected to Spotify';
}

async function accessToken() {
  if (store.get('access') && Date.now() < Number(store.get('expires'))) return store.get('access');
  if (!store.get('refresh')) throw new Error('not connected to Spotify');
  await tokenRequest({ grant_type: 'refresh_token', refresh_token: store.get('refresh') });
  return store.get('access');
}

/** Call the Web API. Returns parsed JSON, or null for "204 No Content". */
export async function api(path, opts = {}, retry = true) {
  const r = await fetch(path.startsWith('http') ? path : API + path, {
    ...opts,
    headers: { Authorization: `Bearer ${await accessToken()}`, 'Content-Type': 'application/json', ...(opts.headers || {}) },
  });
  if (r.status === 401 && retry) {
    store.del('access');
    return api(path, opts, false);
  }
  if (r.status === 429 && retry) {
    const wait = Number(r.headers.get('Retry-After') || 2);
    await new Promise((ok) => setTimeout(ok, Math.min(wait, 30) * 1000));
    return api(path, opts, false);
  }
  if (r.status === 204) return null;
  const j = await r.json().catch(() => null);
  if (!r.ok) {
    const e = new Error(j?.error?.message || `Spotify ${r.status} on ${path}`);
    e.status = r.status;
    throw e;
  }
  return j;
}

// ------------------------------------------------------------------ now playing

/** Best picture of at least `min` pixels, else the biggest. */
function pick(images, min = 256) {
  if (!images?.length) return null;
  const big = images.filter((i) => (i.width || 640) >= min).sort((a, b) => (a.width || 640) - (b.width || 640));
  return (big[0] || images[0]).url;
}

/**
 * Poll what is playing. Calls `onTrack({ artist, title, artistImage, albumImage })`
 * when the song changes. Returns a stop function.
 */
export function watch(onTrack, onError, every = 5000) {
  let last = null;
  let stopped = false;
  async function tick() {
    if (stopped) return;
    try {
      const now = await api('/me/player/currently-playing');
      const item = now?.item;
      if (item && item.id !== last) {
        last = item.id;
        const artist = item.artists?.[0] || {};
        const albumImage = pick(item.album?.images);
        let artistImage = null;
        if (artist.id) {
          try {
            artistImage = pick((await api(`/artists/${artist.id}`)).images);
          } catch {
            // some apps cannot read artists; the album cover will do
          }
        }
        onTrack({
          artist: (item.artists || []).map((a) => a.name).join(', '),
          mainArtist: artist.name || '',
          title: item.name,
          artistImage: artistImage || albumImage,
          albumImage,
        });
      }
    } catch (e) {
      onError?.(e);
    }
    setTimeout(tick, every);
  }
  tick();
  return () => (stopped = true);
}

/** Load a picture URL into 256×256 RGBA pixels (centre-cropped square). */
export async function picture(url) {
  const r = await fetch(url, { mode: 'cors' });
  const bmp = await createImageBitmap(await r.blob());
  const c = new OffscreenCanvas(256, 256);
  const g = c.getContext('2d');
  const s = Math.min(bmp.width, bmp.height);
  g.drawImage(bmp, (bmp.width - s) / 2, (bmp.height - s) / 2, s, s, 0, 0, 256, 256);
  return g.getImageData(0, 0, 256, 256).data;
}

// ------------------------------------------------------------------ playlists

/**
 * Lines of a playlist file: "Artist - Title", a Spotify track link/URI, or
 * "# name: My playlist". Blank lines and other "#" lines are skipped.
 */
export function parsePlaylist(text, fallbackName) {
  let name = fallbackName;
  let isPublic = false;
  let description = '';
  const songs = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const m = line.match(/^#\s*(name|public|description)\s*:\s*(.+)$/i);
    if (m) {
      const [, key, value] = m;
      if (/^name$/i.test(key)) name = value.trim();
      else if (/^public$/i.test(key)) isPublic = /^(yes|true|1)$/i.test(value.trim());
      else description = value.trim();
      continue;
    }
    if (line.startsWith('#')) continue;
    songs.push(line);
  }
  return { name, songs, public: isPublic, description };
}

/** `call` is api() here; site/publish-playlists.mjs passes its own for Node. */
async function findTrack(line, call = api) {
  const id = line.match(/(?:track[/:])([A-Za-z0-9]{22})/)?.[1];
  if (id) return { uri: `spotify:track:${id}`, label: line };
  const parts = line.split(/\s+[-–—]\s+/);
  const queries = [];
  if (parts.length >= 2) {
    const [artist, ...rest] = parts;
    queries.push(`track:${rest.join(' - ')} artist:${artist}`);
  }
  queries.push(line);
  for (const q of queries) {
    const j = await call(`/search?${new URLSearchParams({ q, type: 'track', limit: '1' })}`);
    const t = j?.tracks?.items?.[0];
    if (t) return { uri: t.uri, label: `${t.artists.map((a) => a.name).join(', ')} - ${t.name}` };
  }
  return null;
}

/**
 * Create a private playlist from `songs` (lines as parsed above).
 * `progress(done, total)` is called as songs are looked up.
 * Returns { id, url, uri, found: [labels], missing: [lines] }.
 * `call` does the Web API requests: api() in the browser, its own in site/publish-playlists.mjs.
 */
export async function makePlaylist(name, songs, progress, { public: isPublic = false, description = '' } = {}, call = api) {
  const found = [];
  const missing = [];
  const uris = [];
  for (let i = 0; i < songs.length; i++) {
    const hit = await findTrack(songs[i], call).catch(() => null);
    if (hit) {
      if (!uris.includes(hit.uri)) uris.push(hit.uri);
      found.push(hit.label);
    } else missing.push(songs[i]);
    progress?.(i + 1, songs.length);
  }
  if (!uris.length) throw new Error('none of the songs were found on Spotify');
  const body = JSON.stringify({ name, public: isPublic, description: description || 'Made by the UFO demo from a text file' });
  let pl;
  try {
    pl = await call('/me/playlists', { method: 'POST', body });
  } catch (e) {
    // older form of the same endpoint
    const me = await call('/me');
    pl = await call(`/users/${encodeURIComponent(me.id)}/playlists`, { method: 'POST', body });
  }
  for (let i = 0; i < uris.length; i += 100) {
    const chunk = JSON.stringify({ uris: uris.slice(i, i + 100) });
    try {
      await call(`/playlists/${pl.id}/items`, { method: 'POST', body: chunk });
    } catch {
      await call(`/playlists/${pl.id}/tracks`, { method: 'POST', body: chunk });
    }
  }
  return { id: pl.id, url: pl.external_urls?.spotify, uri: pl.uri, found, missing };
}

/** Start a playlist on the active Spotify device (needs Premium and an open Spotify app). */
export async function play(uri) {
  await api('/me/player/play', { method: 'PUT', body: JSON.stringify({ context_uri: uri }) });
}

/** Skip to the next (or previous) song on the active Spotify device (needs Premium). */
export async function skip(back = false) {
  await api(`/me/player/${back ? 'previous' : 'next'}`, { method: 'POST' });
}

/**
 * Pause the active Spotify device, if something is playing (needs Premium). Resolves only
 * when it really paused, so the caller knows to resume later.
 */
export async function pause() {
  const state = await api('/me/player');
  if (!state?.is_playing) throw new Error('nothing playing');
  await api('/me/player/pause', { method: 'PUT' });
}

/** Carry on where pause() stopped (needs Premium). */
export async function resume() {
  await api('/me/player/play', { method: 'PUT' });
}
