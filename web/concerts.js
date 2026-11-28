// Concerts for donations to Ukraine's defence (pi.ufoscene.net/concert). SECRET SHOW is the
// third show on the concert stage (demo/src/fx/concert.rs), and two ways open it:
//
// - A secret link, #egg-<code>-<letters>, which donors of 500 kr or more get on pi.ufoscene.net.
//   Only the code's SHA-256 is here, so reading this file does not give the code away. Like the
//   Pi image it is an honour gate: the point is that every concert comes with a donation.
// - A public concert, booked with 5000 kr or more: pi.ufoscene.net/concerts.json lists the
//   confirmed ones, and at that time the show plays for everyone with a demo running, with the
//   donor's letters and dedication on the stage's LED wall.

/** SHA-256 of the secret show's code (EGG_CONCERT_CODE on pi.ufoscene.net), in hex. */
export const SECRET_SHA256 = '3a2bcfdee1b508c1230c2aac948c58852eada1544b59b0a3d56eae2e3422f52a';
export const SCHEDULE_URL = 'https://pi.ufoscene.net/concerts.json';
const REFRESH_MS = 5 * 60 * 1000;
/** After a failed fetch the wait doubles, up to this. */
const BACKOFF_MAX_MS = 60 * 60 * 1000;
const TICK_MS = 15 * 1000;
/** Say so a few minutes before a public concert starts. */
const ANNOUNCE_MS = 5 * 60 * 1000;
/** Someone who opens the page in the last minute of a concert does not get it. */
const TOO_LATE_MS = 60 * 1000;
const PLAYED_KEY = 'ufo.concerts.played';

async function sha256Hex(text) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * The secret link: `#egg-<code>` or `#egg-<code>-<letters>`. Resolves to the donor's letters
 * (upper case, '' when the link has none) when the code is right, otherwise to null.
 */
export async function secretFromHash(hash, sha = SECRET_SHA256) {
  const m = /^#egg-([A-Za-z0-9]{8,40})(?:-([^-]{1,40}))?$/.exec(hash || '');
  if (!m || !globalThis.crypto?.subtle) return null; // subtle needs https (or 127.0.0.1)
  if ((await sha256Hex(m[1])) !== sha) return null;
  let letters = '';
  try {
    letters = decodeURIComponent(m[2] || '');
  } catch {
    letters = '';
  }
  return letters.toUpperCase().slice(0, 3);
}

/** What the LED wall says during a public concert. */
export const dedication = (c) => [c.initials, c.message].filter(Boolean).join(': ');

/** The concert that should start now, if any: begun, not nearly over, not played here yet. */
export function due(list, now, lengthMs, played) {
  return list.find((c) => !played.has(c.id) && now >= c.at && now < c.at + lengthMs - TOO_LATE_MS) || null;
}

/** The next concert to announce: starting within ANNOUNCE_MS. */
export function upcoming(list, now, announced) {
  return list.find((c) => !announced.has(c.id) && c.at > now && c.at - now <= ANNOUNCE_MS) || null;
}

/**
 * Watch the schedule. `play(note)` starts the secret show with `note` on the LED wall,
 * `isRunning()` says whether a demo is on screen (a concert waits while the panel is open),
 * `say(text, ms)` shows a message.
 */
export function initConcerts({ url = SCHEDULE_URL, play, isRunning, say }) {
  if (!url) return;
  let list = [];
  let lengthMs = 8 * 60 * 1000;
  const announced = new Set();
  let played;
  try {
    played = new Set(JSON.parse(localStorage.getItem(PLAYED_KEY) || '[]'));
  } catch {
    played = new Set();
  }

  let wait = REFRESH_MS;
  async function refresh() {
    try {
      const r = await fetch(url, { cache: 'no-store' });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const j = await r.json();
      if (j.show !== 'secret' || !Array.isArray(j.concerts)) throw new Error('not a schedule');
      lengthMs = (Number(j.length_s) || 480) * 1000;
      list = j.concerts
        .map((c) => ({ id: String(c.id), at: Date.parse(c.start), initials: String(c.initials || ''), message: String(c.message || '') }))
        .filter((c) => Number.isFinite(c.at));
      // forget concerts that are over, so the stored list stays short
      const ids = new Set(list.map((c) => c.id));
      played = new Set([...played].filter((id) => ids.has(id)));
      wait = REFRESH_MS;
    } catch {
      // offline, or the marketplace is down: ask less often until it answers again
      wait = Math.min(wait * 2, BACKOFF_MAX_MS);
    }
    setTimeout(refresh, wait);
  }

  function tick() {
    if (!isRunning()) return;
    const now = Date.now();
    const c = due(list, now, lengthMs, played);
    if (c) {
      played.add(c.id);
      try {
        localStorage.setItem(PLAYED_KEY, JSON.stringify([...played]));
      } catch {
        // private mode: it may play again after a reload, no harm done
      }
      play(dedication(c));
      say(`A public concert for Ukraine, from ${c.initials}! Book one at pi.ufoscene.net/concert`, 10000);
      return;
    }
    const next = upcoming(list, now, announced);
    if (next) {
      announced.add(next.id);
      const min = Math.max(1, Math.round((next.at - now) / 60000));
      say(`In ${min} min: a public concert from ${next.initials}, for everyone on ufoscene.net`, 8000);
    }
  }

  refresh();
  setInterval(tick, TICK_MS);
}
