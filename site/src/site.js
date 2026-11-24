// Website-only additions, added by site/build.sh (the app itself, setup guide included, is
// all in web/): a "Support UFO ♥" link in the settings panel and an optional extra
// scroll-text line, both from site-config.json (made from site/config.json).

const config = await fetch('site-config.json')
  .then((r) => (r.ok ? r.json() : {}))
  .catch(() => ({}));

/** window.ufo once ufo.js has loaded the wasm and set up its helpers. */
async function ready() {
  while (!window.ufo?.sendPixels) await new Promise((ok) => setTimeout(ok, 100));
  return window.ufo;
}
const ufo = await ready();

// A small link in the panel (P). The panel is hidden while a demo runs, so it never covers one.
if (/^https?:\/\//i.test(config.supportUrl || '')) {
  const a = document.createElement('a');
  a.id = 'support';
  a.href = config.supportUrl;
  a.target = '_blank';
  a.rel = 'noopener';
  a.textContent = config.supportText || 'Support UFO ♥';
  const row = document.getElementById('setup-status')?.parentElement || document.querySelector('#panel .row');
  row?.append(a);
}

// One line at the end of every scroll text, only if the wasm exports ufo_set_scroll_extra.
if (config.scrollLine && ufo.ex.ufo_set_scroll_extra) {
  const bytes = new TextEncoder().encode(config.scrollLine).slice(0, 1024);
  new Uint8Array(ufo.ex.memory.buffer, ufo.ex.ufo_text_buf(ufo.demo), 1024).set(bytes);
  ufo.ex.ufo_set_scroll_extra(ufo.demo, bytes.length);
}
