#!/usr/bin/env node
// Publish the ufoscene.net playlists (web/playlists/*.txt) as public Spotify playlists in your
// account, and write their links into web/playlists/index.json. For the site owner, once (and
// again for any list that has no link yet). Visitors only ever see the finished links.
//
//   node site/publish-playlists.mjs [--client-id <id>] [--port 8100] [--dry-run]
//
// The Client ID comes from --client-id, SPOTIFY_CLIENT_ID or site/config.json
// (spotifyClientId). Your Spotify app needs the redirect URI http://127.0.0.1:<port>/callback,
// the same one the local page uses, so stop ./run.sh while this runs. Login is PKCE: no client
// secret, and the tokens only live in this process.

import { createServer } from 'node:http';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash, randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { parsePlaylist, makePlaylist } from '../web/spotify.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const LISTS = join(ROOT, 'web', 'playlists');
const INDEX = join(LISTS, 'index.json');
const API = 'https://api.spotify.com/v1';
const SCOPES = 'playlist-modify-public playlist-modify-private';
const playlistId = (link) => link?.match(/playlist[/:]([A-Za-z0-9]{22})/)?.[1] || null;

// ---------------------------------------------------------------- arguments

const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
if (args.includes('--help') || args.includes('-h')) {
  console.log('node site/publish-playlists.mjs [--client-id <id>] [--port 8100] [--dry-run]');
  process.exit(0);
}
const dryRun = args.includes('--dry-run');
const port = Number(opt('--port') || 8100);
const redirectUri = `http://127.0.0.1:${port}/callback`;
const siteConfig = await readFile(join(ROOT, 'site', 'config.json'), 'utf8').then(JSON.parse).catch(() => ({}));
const clientId = opt('--client-id') || process.env.SPOTIFY_CLIENT_ID || siteConfig.spotifyClientId || '';

const index = JSON.parse(await readFile(INDEX, 'utf8'));
const todo = index.filter((p) => !playlistId(p.spotify));
console.log(`${index.length} lists in web/playlists/index.json, ${todo.length} without a Spotify link.`);
if (!todo.length) process.exit(0);
for (const p of todo) console.log(`  - ${p.title} (${p.file})`);
if (dryRun) process.exit(0);
if (!clientId) {
  console.error('\nNo Client ID: pass --client-id, set SPOTIFY_CLIENT_ID, or add spotifyClientId to site/config.json.');
  process.exit(1);
}

// ---------------------------------------------------------------- login (PKCE)

const b64url = (buf) => buf.toString('base64url');
const verifier = b64url(randomBytes(48));
const challenge = b64url(createHash('sha256').update(verifier).digest());
const state = b64url(randomBytes(12));

const code = await new Promise((resolve, reject) => {
  const server = createServer((req, res) => {
    const url = new URL(req.url, redirectUri);
    if (url.pathname !== '/callback') return res.writeHead(404).end();
    const done = (status, text) => {
      res.writeHead(status, { 'Content-Type': 'text/plain; charset=utf-8' }).end(text);
      server.close();
    };
    if (url.searchParams.get('error')) {
      done(400, 'Login cancelled. You can close this tab.');
      return reject(new Error(`login cancelled (${url.searchParams.get('error')})`));
    }
    if (url.searchParams.get('state') !== state) {
      done(400, 'Bad state. Run the script again.');
      return reject(new Error('bad state in the callback'));
    }
    done(200, 'Logged in. You can close this tab; the playlists are being made in the terminal.');
    resolve(url.searchParams.get('code'));
  });
  server.on('error', (e) =>
    reject(e.code === 'EADDRINUSE' ? new Error(`port ${port} is in use: stop ./run.sh, or pass --port`) : e),
  );
  server.listen(port, '127.0.0.1', () => {
    const q = new URLSearchParams({
      response_type: 'code',
      client_id: clientId,
      scope: SCOPES,
      redirect_uri: redirectUri,
      code_challenge_method: 'S256',
      code_challenge: challenge,
      state,
    });
    console.log(`\nOpen this in your browser and log in to the Spotify account that should own the playlists:\n\n  https://accounts.spotify.com/authorize?${q}\n`);
  });
});

const tokenRes = await fetch('https://accounts.spotify.com/api/token', {
  method: 'POST',
  headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
  body: new URLSearchParams({ client_id: clientId, grant_type: 'authorization_code', code, redirect_uri: redirectUri, code_verifier: verifier }),
});
const token = await tokenRes.json();
if (!tokenRes.ok) throw new Error(token.error_description || token.error || `token request failed (${tokenRes.status})`);

/** The Web API call makePlaylist() uses: JSON in and out, one wait on 429. */
async function call(path, opts = {}, retry = true) {
  const r = await fetch(path.startsWith('http') ? path : API + path, {
    ...opts,
    headers: { Authorization: `Bearer ${token.access_token}`, 'Content-Type': 'application/json', ...(opts.headers || {}) },
  });
  if (r.status === 429 && retry) {
    const wait = Number(r.headers.get('Retry-After') || 2);
    await new Promise((ok) => setTimeout(ok, Math.min(wait, 30) * 1000));
    return call(path, opts, false);
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

const me = await call('/me');
console.log(`Logged in as ${me.display_name || me.id}.\n`);

// ---------------------------------------------------------------- make them

let made = 0;
const problems = [];
for (const entry of todo) {
  try {
    const list = parsePlaylist(await readFile(join(LISTS, entry.file), 'utf8'), entry.title);
    const pl = await makePlaylist(
      list.name,
      list.songs,
      (i, n) => process.stdout.write(`\r${entry.title}: looking up songs ${i}/${n}   `),
      { public: list.public, description: list.description },
      call,
    );
    entry.spotify = pl.url || '';
    made++;
    // written after every list, so a stop halfway keeps the links made so far
    await writeFile(INDEX, JSON.stringify(index, null, 2) + '\n');
    console.log(`\r${entry.title}: ${pl.found.length} songs → ${pl.url}`);
    if (pl.missing.length) problems.push(`${entry.title}: not found: ${pl.missing.join('; ')}`);
  } catch (e) {
    console.log(`\r${entry.title}: NOT CREATED (${e.message})`);
    problems.push(`${entry.title}: NOT CREATED (${e.message})`);
  }
}

console.log(`\nMade ${made} of ${todo.length} playlists; their links are in web/playlists/index.json.`);
if (problems.length) console.log(`\nNot found or failed:\n  ${problems.join('\n  ')}`);
if (made) console.log('\nCommit web/playlists/index.json and deploy.');
