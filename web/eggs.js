// Easter eggs: secret key codes, the calendar and the clock decide when; the drawing is
// in Rust (demo/src/eggs.rs). A UFO also flies by on its own now and then, from Rust.
// Shh: the README only gives hints.

/** Egg kinds, as numbered in eggs.rs. */
export const EGG = { flyby: 1, cheat: 2, flip: 3, fireworks: 4, midnight: 5, xmas: 6, midsummer: 7, greetings: 8 };

/** Key codes typed while a demo runs. Keys after the first are swallowed while they fit. */
const CODES = [
  { keys: ['arrowup', 'arrowup', 'arrowdown', 'arrowdown', 'arrowleft', 'arrowright', 'arrowleft', 'arrowright', 'b', 'a'], egg: 'cheat' },
  { keys: ['u', 'f', 'o'], egg: 'flyby' },
];
const KEY_GAP_MS = 1500;
/** Calendar eggs come 1 minute into the show, then every 30 minutes. */
const DAY_FIRST_S = 60;
const DAY_EVERY_S = 30 * 60;
const TICK_S = 20;

/** Midsummer Eve: the Friday between 19 and 25 June. */
function isMidsummerEve(d) {
  return d.getMonth() === 5 && d.getDay() === 5 && d.getDate() >= 19 && d.getDate() <= 25;
}

/** The egg the calendar has for day `d`, if any. */
export function dayEgg(d) {
  if (d.getMonth() === 11 && d.getDate() === 24) return 'xmas';
  if (d.getMonth() === 3 && d.getDate() === 1) return 'flip';
  if (isMidsummerEve(d)) return 'midsummer';
  return null;
}

/** From 23 to 28 December the show opens with the Christmas scene. */
export const christmasWeek = (d) => d.getMonth() === 11 && d.getDate() >= 23 && d.getDate() <= 28;

/** Snow from 1 December to Twelfth Night (6 January). */
export const snowy = (d) => d.getMonth() === 11 || (d.getMonth() === 0 && d.getDate() <= 6);

/**
 * `start(kind, arg)` starts an egg in the demo, `snow(on)` turns snow on or off, and
 * `isRunning()` says whether a demo is on screen (eggs wait while the panel is open).
 */
export function initEggs({ start, snow, isRunning }) {
  const play = (name, arg = 0) => EGG[name] && start(EGG[name], arg);

  // ---- key codes
  const progress = CODES.map(() => 0);
  let lastKey = 0;
  window.addEventListener(
    'keydown',
    (e) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement;
      if (typing || e.repeat || !isRunning() || e.timeStamp - lastKey > KEY_GAP_MS) progress.fill(0);
      lastKey = e.timeStamp;
      if (typing || e.repeat || !isRunning()) return;
      const k = e.key.toLowerCase();
      let swallow = false;
      CODES.forEach((c, i) => {
        if (c.keys[progress[i]] === k) {
          progress[i]++;
          if (progress[i] > 1) swallow = true;
          if (progress[i] === c.keys.length) {
            progress[i] = 0;
            play(c.egg);
          }
        } else {
          progress[i] = c.keys[0] === k ? 1 : 0;
        }
      });
      if (swallow) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    },
    true,
  );

  // ---- the calendar and the clock
  let shown = 0; // seconds of show time
  let nextDay = DAY_FIRST_S;
  let lastMidnight = '';
  setInterval(() => {
    const now = new Date();
    snow(snowy(now));
    if (!isRunning()) return;
    shown += TICK_S;
    const today = now.toDateString();
    if (now.getHours() === 0 && now.getMinutes() === 0 && lastMidnight !== today) {
      lastMidnight = today;
      if (now.getMonth() === 0 && now.getDate() === 1) play('fireworks', now.getFullYear());
      else play('midnight');
      return;
    }
    const egg = dayEgg(now);
    if (egg && shown >= nextDay) {
      nextDay = shown + DAY_EVERY_S;
      play(egg);
    }
  }, TICK_S * 1000);
  snow(snowy(new Date()));

  // ---- for the curious, in the browser console
  console.log(
    '%c\n        .-"""""-.\n   _.-\'  U F O  \'-._\n  (_________________)\n     \'   \'   \'   \'\n',
    'color:#ff9a1f;font-family:monospace;font-size:13px',
  );
  console.log(
    '%cHej scener! You found the console.%c\nTry ufo.egg("greetings") with a demo running. There are more: a cheat code every gamer knows, a word of three letters, and the calendar.\nThe code: https://github.com/ulfmansson/ufoscene',
    'color:#ff9a1f;font-weight:bold',
    'color:inherit',
  );

  return play;
}
