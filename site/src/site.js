// Website-only additions, added by site/build.sh (the app itself, setup guide included, is
// all in web/): a "Support UFO ♥" link in the settings panel and an optional extra
// scroll-text line, both from site-config.json (made from site/config.json); the newest line
// of news.txt for a few seconds at the start; and a one-line feature-request box at the
// bottom of the panel.

const config = await fetch('site-config.json')
  .then((r) => (r.ok ? r.json() : {}))
  .catch(() => ({}));

// At the start: the newest feature, from news.txt. It goes in the panel under the intro line,
// and when the panel is closed (a demo already running) also on screen for a few seconds,
// in a box that never takes clicks.
fetch('news.txt')
  .then((r) => (r.ok ? r.text() : ''))
  .catch(() => '')
  .then((text) => {
    const latest = text
      .split('\n')
      .map((l) => l.trim())
      .find((l) => l && !l.startsWith('#'));
    if (!latest) return;
    const panel = document.getElementById('panel');
    const line = document.createElement('div');
    line.id = 'news-line';
    line.innerHTML = '<b>New:</b> ';
    line.append(latest);
    panel?.querySelector('h1')?.nextElementSibling?.after(line);
    if (panel && getComputedStyle(panel).display !== 'none') return;
    const el = document.createElement('div');
    el.id = 'news';
    el.setAttribute('role', 'status');
    el.textContent = `New: ${latest}`;
    document.body.append(el);
    setTimeout(() => el.classList.add('gone'), 9000);
    setTimeout(() => el.remove(), 10000);
  });

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

// The feature-request box, last in the panel. ufoscene.net is static, so the line goes to the
// marketplace backend (ufoscene-site, POST /ideas), which keeps only the text. A urlencoded
// body makes it a CORS simple request: no preflight. config.ideasUrl changes the address and
// "" turns the box off (for copies of the site elsewhere).
const ideasUrl = config.ideasUrl ?? 'https://pi.ufoscene.net/ideas';
const panel = document.getElementById('panel');
if (panel && /^https:\/\//i.test(ideasUrl)) {
  const form = document.createElement('form');
  form.id = 'idea';
  form.className = 'row';
  form.innerHTML =
    '<label for="idea-text">Feature request</label>' +
    '<input type="text" id="idea-text" name="text" minlength="3" maxlength="300" required autocomplete="off" placeholder="What should UFO do next?">' +
    '<button type="submit">Send</button><span id="idea-status" role="status"></span>';
  panel.append(form);
  const input = form.querySelector('input');
  const button = form.querySelector('button');
  const status = form.querySelector('#idea-status');
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    button.disabled = true;
    status.textContent = 'Sending…';
    try {
      const r = await fetch(ideasUrl, { method: 'POST', body: new URLSearchParams({ text: input.value }) });
      if (r.ok) {
        input.value = '';
        status.textContent = 'Thanks! ♥';
      } else if (r.status === 429) {
        status.textContent = 'Thanks, that is plenty for now. Try again in an hour.';
      } else if (r.status === 422) {
        status.textContent = 'Write 3 to 300 characters.';
      } else {
        status.textContent = 'Could not send it. Try again later.';
      }
    } catch {
      status.textContent = 'Could not send it. Try again later.';
    }
    button.disabled = false;
  });
}
