// More photos of an artist than Spotify's single one, from Wikidata and Wikimedia
// Commons. Both answer browsers directly (CORS), and upload.wikimedia.org images
// can be drawn into a canvas. Coverage varies: big artists have many photos,
// small ones may have none.

const WD = 'https://www.wikidata.org/w/api.php';
const COMMONS = 'https://commons.wikimedia.org/w/api.php';
// Wikimedia asks API clients to say who they are; browsers may not set User-Agent.
const HEADERS = { 'Api-User-Agent': 'UFO-demo/1.0 (music visualiser)' };
const MUSIC = /band|group|singer|musician|rapper|duo|trio|dj|composer|producer|songwriter|vocalist|artist|orchestra|ensemble/i;

async function get(base, params) {
  const r = await fetch(`${base}?${new URLSearchParams({ format: 'json', origin: '*', ...params })}`, { headers: HEADERS });
  if (!r.ok) throw new Error(`${base} ${r.status}`);
  return r.json();
}

/** The Wikidata item of a music artist called `name`, or null. */
async function findArtist(name) {
  const j = await get(WD, { action: 'wbsearchentities', search: name, language: 'en', type: 'item', limit: '7' });
  const hits = j.search || [];
  return hits.find((h) => MUSIC.test(h.description || '')) || null;
}

/** Plain text of a Commons metadata field (they are HTML). */
function plain(html) {
  const d = new DOMParser().parseFromString(html || '', 'text/html');
  return (d.body.textContent || '').replace(/\s+/g, ' ').trim();
}

/**
 * Photos (`width` px wide thumbnails) for Commons files, as { url, credit }. Most Commons
 * photos are CC BY or CC BY-SA, which require naming the author and the licence.
 */
async function thumbs(params, width) {
  const j = await get(COMMONS, {
    action: 'query',
    prop: 'imageinfo',
    iiprop: 'url|mime|size|extmetadata',
    iiextmetadatafilter: 'Artist|LicenseShortName',
    iiurlwidth: String(width),
    ...params,
  });
  return Object.values(j.query?.pages || {})
    .map((p) => p.imageinfo?.[0])
    .filter((i) => i && /^image\/(jpeg|png|webp)$/.test(i.mime) && i.width >= 300 && i.height >= 300 && i.thumburl)
    .map((i) => {
      const who = plain(i.extmetadata?.Artist?.value).slice(0, 30);
      const lic = plain(i.extmetadata?.LicenseShortName?.value);
      const credit = ['PHOTO: ' + (who || 'UNKNOWN'), lic, 'WIKIMEDIA COMMONS'].filter(Boolean).join(', ');
      return { url: i.thumburl, credit };
    });
}

/**
 * Up to `max` photos ({ url, credit }) of `artist` (the main Wikidata image first, then photos from
 * its Commons category in random order). Never throws: returns [] when nothing is found.
 */
export async function artistPhotos(artist, max = 8) {
  try {
    const hit = await findArtist(artist);
    if (!hit) return [];
    const e = await get(WD, { action: 'wbgetentities', ids: hit.id, props: 'claims' });
    const claims = e.entities?.[hit.id]?.claims || {};
    const files = (claims.P18 || []).map((c) => c.mainsnak?.datavalue?.value).filter(Boolean);
    const category = claims.P373?.[0]?.mainsnak?.datavalue?.value;
    const out = [];
    if (files.length) out.push(...(await thumbs({ titles: files.map((f) => `File:${f}`).join('|') }, 512)));
    if (category) {
      const more = await thumbs(
        { generator: 'categorymembers', gcmtitle: `Category:${category}`, gcmtype: 'file', gcmlimit: '40' },
        512,
      );
      for (let i = more.length - 1; i > 0; i--) {
        const k = Math.floor(Math.random() * (i + 1));
        [more[i], more[k]] = [more[k], more[i]];
      }
      out.push(...more);
    }
    const seen = new Set();
    return out.filter((p) => !seen.has(p.url) && seen.add(p.url)).slice(0, max);
  } catch (e) {
    console.warn('artist photos', e);
    return [];
  }
}
