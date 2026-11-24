// The ufoscene.net playlists: public Spotify playlists by decade, genre, Kite and computer
// music. The song lists are in playlists/*.txt; playlists/index.json lists them and, once
// they are published, holds each one's Spotify link.
//
// Visitors with Spotify connected pick one and play it on their Spotify app (Premium) or open
// it in Spotify. The site owner makes them, once, with site/publish-playlists.mjs, which
// writes the links into index.json; the page itself never creates playlists.

const GROUPS = { decade: 'Decades', genre: 'Genres', artist: 'Artists', computer: 'Computer music' };
const playlistId = (link) => link?.match(/playlist[/:]([A-Za-z0-9]{22})/)?.[1] || null;

/** Load playlists/index.json: [{ id, group, title, file, spotify }]. */
export async function loadIndex() {
  const r = await fetch('playlists/index.json');
  if (!r.ok) throw new Error(`playlists/index.json: ${r.status}`);
  return r.json();
}

/**
 * Show the picker in `container`. `onStart()` is called first when Play is pressed: the
 * panel pauses everything, so playing must also close it and carry on (and, with tab audio
 * chosen, ask for the tab while the click still counts).
 * Returns a refresh() to call when the Spotify connection changes.
 */
export function initPlaylists({ spotify, say, container, onStart }) {
  if (!container) return () => {};
  let index = [];

  async function play(entry) {
    const id = playlistId(entry.spotify);
    if (!id) return;
    try {
      await spotify.play(`spotify:playlist:${id}`);
      say(`Playing ${entry.title} in Spotify.`, 5000);
    } catch (e) {
      if (e.status === 404) say('Open the Spotify app (or open.spotify.com) and play anything once, then try again.', 9000);
      else if (e.status === 403) say('Starting playback from here needs Spotify Premium. Use "Open in Spotify" instead.', 9000);
      else say(`Could not start it (${e.message}).`, 8000);
    }
  }

  function render() {
    container.textContent = '';
    const published = index.filter((p) => playlistId(p.spotify));
    const row = document.createElement('div');
    row.className = 'row';
    if (!published.length) {
      row.textContent = 'ufoscene.net Spotify playlists: not published yet.';
    } else {
      const label = document.createElement('label');
      label.textContent = 'Spotify playlist ';
      const sel = document.createElement('select');
      sel.id = 'ufo-playlist';
      for (const [group, heading] of Object.entries(GROUPS)) {
        const items = published.filter((p) => p.group === group);
        if (!items.length) continue;
        const og = document.createElement('optgroup');
        og.label = heading;
        for (const p of items) og.append(new Option(p.title, p.id));
        sel.append(og);
      }
      try {
        const saved = localStorage.getItem('ufo.playlist');
        if (published.some((p) => p.id === saved)) sel.value = saved;
      } catch {}
      label.append(sel);
      const current = () => published.find((p) => p.id === sel.value);
      const playBtn = document.createElement('button');
      playBtn.textContent = 'Play in Spotify';
      playBtn.disabled = !spotify.connected();
      playBtn.title = spotify.connected() ? 'Plays on your Spotify app (Premium)' : 'Connect Spotify first';
      playBtn.onclick = () => {
        onStart?.();
        play(current());
      };
      const open = document.createElement('a');
      open.target = '_blank';
      open.rel = 'noopener';
      open.textContent = 'Open in Spotify';
      const update = () => {
        open.href = current()?.spotify || '#';
        try {
          localStorage.setItem('ufo.playlist', sel.value);
        } catch {}
      };
      sel.onchange = update;
      update();
      row.append(label, playBtn, open);
    }
    container.append(row);
    if (published.length) {
      const note = document.createElement('div');
      note.className = 'note';
      note.textContent = spotify.connected()
        ? '"Play in Spotify" starts it in your open Spotify tab or app (needs Premium).'
        : 'Anyone can open these public playlists in Spotify. To start them from here, connect Spotify in the setup guide (G); that needs Premium.';
      container.append(note);
    }
  }

  loadIndex()
    .then((list) => {
      index = list;
      render();
    })
    .catch((e) => {
      container.textContent = '';
      console.warn('playlists', e);
    });

  return render;
}
