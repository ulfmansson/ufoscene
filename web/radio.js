// Internet radio, for when nobody has set up Spotify or a music source: pick a decade
// (80s, 90s, 00s, 10s) and/or one of ten genres. Stations come from the community
// directory radio-browser.info, by tag,
// with a few known stations as a backup; stations that send the song now playing
// (artist and title) come first, so the demo can show them and find photos. Only streams that allow cross-origin access
// (CORS) can be analysed by the page, so a station that refuses is skipped.

const MIRRORS = ['https://de1.api.radio-browser.info', 'https://fi1.api.radio-browser.info', 'https://at1.api.radio-browser.info'];

const BACKUP = [
  { name: 'Heart 80s', url: 'https://media-ssl.musicradio.com/Heart80sMP3' },
  { name: 'Heart 90s', url: 'https://media-ssl.musicradio.com/Heart90sMP3' },
  { name: 'Retro Rádió', url: 'https://icast.connectmedia.hu/5001/live.mp3' },
];

/**
 * Stations known to send the song now playing inside the stream (ICY metadata) in a way
 * browsers may read, so the demo can show the artist and find photos. They go first when
 * they match the chosen decade or genre. `tags` uses the directory's tag names.
 */
const WITH_SONGS = [
  { name: 'Heart 70s', url: 'https://media-ssl.musicradio.com/Heart70sMP3', tags: ['70s'] },
  { name: 'Heart 80s', url: 'https://media-ssl.musicradio.com/Heart80sMP3', tags: ['80s'] },
  { name: 'Heart 90s', url: 'https://media-ssl.musicradio.com/Heart90sMP3', tags: ['90s'] },
  { name: 'Heart 00s', url: 'https://media-ssl.musicradio.com/Heart00sMP3', tags: ['00s', '2000s'] },
  { name: 'Heart Dance', url: 'https://media-ssl.musicradio.com/HeartDanceMP3', tags: ['house', 'eurodance', 'disco', '90s'] },
  { name: 'Capital Dance', url: 'https://media-ssl.musicradio.com/CapitalDanceMP3', tags: ['house', 'trance', '10s', '2010s'] },
  { name: 'Capital', url: 'https://media-ssl.musicradio.com/CapitalMP3', tags: ['10s', '2010s', 'hip hop'] },
  { name: 'Radio X Classic Rock', url: 'https://media-ssl.musicradio.com/RadioXClassicRockMP3', tags: ['classic rock', '70s', '80s'] },
  { name: 'Smooth', url: 'https://media-ssl.musicradio.com/SmoothUKMP3', tags: ['70s', '80s', '90s'] },
  { name: 'Gold', url: 'https://media-ssl.musicradio.com/GoldMP3', tags: ['60s', '70s'] },
];

/** What the radio plays until the visitor picks something else. */
export const DEFAULT_DECADE = '80s90s';
export const DEFAULT_GENRE = 'trance';

/** Decades to choose from: id -> [label, directory tags (any of them)]. */
export const DECADES = {
  '80s90s': ['80s & 90s', ['80s', '90s']],
  '70s': ['70s', ['70s']],
  '80s': ['80s', ['80s']],
  '90s': ['90s', ['90s']],
  '00s': ['00s', ['00s', '2000s']],
  '10s': ['10s', ['10s', '2010s']],
  any: ['Any decade', []],
};

/** Genres to choose from: id -> [label, directory tags (any of them)]. */
export const GENRES = {
  any: ['Any genre', []],
  synthpop: ['Synthpop & new wave', ['synthpop', 'new wave']],
  italo: ['Italo disco', ['italo disco']],
  eurodance: ['Eurodance', ['eurodance']],
  disco: ['Disco', ['disco']],
  synthwave: ['Synthwave', ['synthwave', 'retrowave']],
  trance: ['Trance', ['trance']],
  house: ['House', ['house']],
  rock: ['Classic rock', ['classic rock']],
  hiphop: ['Hip hop', ['hip hop']],
  chiptune: ['Chiptune & demoscene', ['chiptune', 'demoscene']],
};

/** Station names can be whole ads: keep the part before " @ ", " - ", " | " and so on. */
function shortName(name) {
  const first = name.replace(/^[#\s\-–—]+/, '').split(/\s+[@|–—-]\s+|\s*[|@]\s*/)[0].trim();
  return first.length > 36 ? first.slice(0, 35).trimEnd() + '…' : first || 'Radio';
}

async function search(base, params) {
  const q = new URLSearchParams({ order: 'clickcount', reverse: 'true', limit: '15', hidebroken: 'true', is_https: 'true', ...params });
  const r = await fetch(`${base}/json/stations/search?${q}`);
  if (!r.ok) throw new Error(`radio-browser ${r.status}`);
  return (await r.json())
    .filter((s) => !s.hls && !/\.m3u8?(\?|$)|\.pls(\?|$)/i.test(s.url_resolved) && /^https:/.test(s.url_resolved))
    .map((s) => ({ name: shortName(s.name), url: s.url_resolved }));
}

/** Interleave lists: a1, b1, c1, a2, b2, ... */
const mix = (lists) => Array.from({ length: Math.max(0, ...lists.map((l) => l.length)) }, (_, i) => lists.map((l) => l[i])).flat();

/**
 * Stations for a decade and a genre (ids from DECADES and GENRES), most listened first.
 * Both set: stations tagged with both; if fewer than 3, the genre alone. The 80s/90s
 * backups come last when the choice includes those decades.
 */
export async function stations(decade = DEFAULT_DECADE, genre = DEFAULT_GENRE) {
  const dTags = (DECADES[decade] || DECADES['80s90s'])[1];
  const gTags = (GENRES[genre] || GENRES.any)[1];
  let found = [];
  for (const base of MIRRORS) {
    try {
      if (dTags.length && gTags.length) {
        found = mix(await Promise.all(dTags.flatMap((d) => gTags.map((g) => search(base, { tagList: `${d},${g}` })))));
        if (found.length < 3) found = [];
      }
      if (!found.length) {
        const tags = gTags.length ? gTags : dTags.length ? dTags : ['80s', '90s'];
        found = mix(await Promise.all(tags.map((tag) => search(base, { tag }))));
      }
      break;
    } catch {
      found = []; // try the next mirror
    }
  }
  const backups = genre === 'any' && ['80s90s', '80s', '90s', 'any'].includes(decade) ? BACKUP : [];
  // stations that tell us the song go first when they fit the choice
  const want = gTags.length ? gTags : dTags;
  const songs = WITH_SONGS.filter((st) => !want.length || st.tags.some((t) => want.includes(t)));
  const seen = new Set();
  return [...songs, ...found, ...backups].filter((s) => s && !seen.has(s.url) && seen.add(s.url));
}

/**
 * The song now playing on an internet radio stream, read from the stream's own ICY
 * metadata ("StreamTitle='Artist - Title';"), or null if the station does not send it or
 * does not let browsers ask for it. Reads only up to the first metadata block (~8 kB).
 */
export async function nowPlaying(url) {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 8000);
  try {
    const r = await fetch(url, { headers: { 'Icy-MetaData': '1' }, signal: ctl.signal, cache: 'no-store' });
    const every = parseInt(r.headers.get('icy-metaint') || '0', 10);
    if (!r.ok || !every) return null;
    const reader = r.body.getReader();
    let buf = new Uint8Array(0);
    for (;;) {
      const { value, done } = await reader.read();
      if (done) return null;
      const n = new Uint8Array(buf.length + value.length);
      n.set(buf);
      n.set(value, buf.length);
      buf = n;
      if (buf.length > every && buf.length >= every + 1 + buf[every] * 16) break;
    }
    const meta = new TextDecoder().decode(buf.subarray(every + 1, every + 1 + buf[every] * 16));
    const m = meta.match(/StreamTitle='(.*?)';/);
    const [artist, ...rest] = (m?.[1] || '').split(' - ');
    const title = rest.join(' - ').trim();
    return artist.trim() && title ? { artist: artist.trim(), title } : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
    ctl.abort();
  }
}

/**
 * Plays stations through one <audio> element whose sound the caller routes through
 * Web Audio (`node` is its MediaElementSource). `onStation(station)` is called when
 * a station really plays.
 */
export class Radio {
  constructor(ac, onStation) {
    this.audio = new Audio();
    this.audio.crossOrigin = 'anonymous'; // needed so the analyser may read the sound
    this.audio.preload = 'none';
    this.node = ac.createMediaElementSource(this.audio);
    this.onStation = onStation;
    this.list = [];
    this.i = -1;
    this.decade = DEFAULT_DECADE;
    this.genre = DEFAULT_GENRE;
    /** Set by the page: called with (artist, title) when the station's song changes. */
    this.onSong = null;
    this.songTimer = null;
  }

  /** Ask the station what it is playing every 15 s; give up quietly if it never says. */
  watchSongs(st) {
    clearTimeout(this.songTimer);
    let last = '';
    let misses = 0;
    const poll = async () => {
      if (this.list[this.i] !== st) return; // tuned away
      const song = await nowPlaying(st.url);
      if (this.list[this.i] !== st) return;
      if (song) {
        misses = 0;
        const key = `${song.artist} - ${song.title}`;
        if (key !== last) {
          last = key;
          this.onSong?.(song.artist, song.title, st);
        }
      } else if (++misses >= 2 && !last) {
        return; // this station does not tell
      }
      this.songTimer = setTimeout(poll, 15000);
    };
    this.songTimer = setTimeout(poll, 1500);
  }

  /** Pick a decade and genre (ids from DECADES / GENRES) and tune to the first station. */
  async choose(decade, genre) {
    this.decade = decade;
    this.genre = genre;
    this.list = await stations(decade, genre);
    this.i = -1;
    return this.next();
  }

  /** Try stations from index `i` on (wrapping), until one plays. */
  async tune(i) {
    if (!this.list.length) this.list = await stations(this.decade, this.genre);
    for (let n = 0; n < this.list.length; n++) {
      const k = (((i + n) % this.list.length) + this.list.length) % this.list.length;
      if (await this.tryOne(this.list[k])) {
        this.i = k;
        this.onStation(this.list[k]);
        this.watchSongs(this.list[k]);
        return this.list[k];
      }
    }
    throw new Error('no station could be played');
  }

  tryOne(st) {
    const a = this.audio;
    return new Promise((ok) => {
      const done = (good) => {
        clearTimeout(timer);
        a.removeEventListener('playing', yes);
        a.removeEventListener('error', no);
        ok(good);
      };
      const yes = () => done(true);
      const no = () => done(false);
      const timer = setTimeout(no, 9000);
      a.addEventListener('playing', yes);
      a.addEventListener('error', no);
      a.src = st.url;
      a.play().catch(no);
    });
  }

  next() {
    return this.tune(this.i + 1);
  }

  prev() {
    return this.tune(this.i - 1);
  }
}
