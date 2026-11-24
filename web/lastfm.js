// Last.fm "now playing" for any visitor: no login and no allowlist, only the visitor's
// username and one public API key for the site. Spotify (and many other players) report
// what plays to Last.fm once the visitor connects them in their Last.fm settings.
// Covers come from Last.fm, else from the iTunes Search API (both allow browser calls).

const API = 'https://ws.audioscrobbler.com/2.0/';
// Last.fm returns this grey star instead of a real picture
const PLACEHOLDER = '2a96cbd8b46e442fc41c2b86b821562f';

/** The user's latest track: { artist, title, album, nowPlaying, image } or null. */
export async function recent(user, key) {
  const q = new URLSearchParams({ method: 'user.getrecenttracks', user, api_key: key, format: 'json', limit: '1' });
  const r = await fetch(`${API}?${q}`);
  const j = await r.json().catch(() => null);
  if (!r.ok || j?.error) {
    const e = new Error(j?.message || `Last.fm answered ${r.status}`);
    e.code = j?.error;
    throw e;
  }
  let t = j?.recenttracks?.track;
  if (Array.isArray(t)) t = t[0];
  if (!t) return null;
  const image =
    (t.image || [])
      .map((i) => i['#text'])
      .filter((u) => u && !u.includes(PLACEHOLDER))
      .pop() || null; // the list goes small → extralarge
  return {
    artist: t.artist?.['#text'] || t.artist?.name || '',
    title: t.name || '',
    album: t.album?.['#text'] || '',
    nowPlaying: t['@attr']?.nowplaying === 'true',
    image,
  };
}

/** A 600×600 cover for the song from the iTunes Search API, or null. */
export async function cover(artist, title) {
  const q = new URLSearchParams({ term: `${artist} ${title}`, entity: 'song', limit: '1' });
  const r = await fetch(`https://itunes.apple.com/search?${q}`);
  const j = await r.json();
  const art = j?.results?.[0]?.artworkUrl100;
  return art ? art.replace(/\/\d+x\d+bb\./, '/600x600bb.') : null;
}

/** A sentence for the visitor about a failed Last.fm call. */
export function explain(e) {
  switch (e.code) {
    case 6:
      return 'Last.fm does not know that username.';
    case 10:
    case 26:
      return 'The Last.fm API key is not valid.';
    case 17:
      return 'That profile hides its listening: Last.fm → Settings → Privacy.';
    case 29:
      return 'Last.fm says too many requests; trying again shortly.';
    default:
      return e.message || String(e);
  }
}

/**
 * Poll what `user` is playing. Calls `onTrack({ artist, title, album, image, imageSource })` when a new
 * song starts (not for songs that already ended). Returns a stop function.
 */
export function watch(user, key, onTrack, onError, every = 10000) {
  let last = null;
  let stopped = false;
  let timer = null;
  async function tick() {
    if (stopped) return;
    try {
      const t = await recent(user, key);
      onError?.(null);
      if (t?.nowPlaying) {
        const id = `${t.artist}\n${t.title}`;
        if (id !== last) {
          last = id;
          let image = t.image;
          let imageSource = image ? 'LAST.FM' : '';
          if (!image) {
            image = await cover(t.artist, t.title).catch(() => null);
            imageSource = image ? 'ITUNES' : '';
          }
          if (!stopped) onTrack({ ...t, image, imageSource });
        }
      }
    } catch (e) {
      onError?.(e);
    }
    if (!stopped) timer = setTimeout(tick, every);
  }
  tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}
