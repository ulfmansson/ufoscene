# UFO website (`site/`)

This folder turns the demos into a public website. `build.sh` builds `ufo.wasm`, copies `web/`
(the whole app, including the first-visit setup guide, 80s & 90s radio and Last.fm "now playing")
into `site/dist/` and adds only what a published site needs: its settings, an optional support
link, and description tags for link previews. The result is a static site: everything runs in the visitor's browser, with no server
code, database or secret. Nothing in this folder publishes anything until you run a deploy
script.

All commands below start in the repository root.

## What visitors get

| | Every visitor | Only the site owner (+ up to 5 people they add) |
|---|---|---|
| The 100 demos, keys, fullscreen, `#42` links, Cast tab | ✓ | |
| **80s & 90s internet radio**, no setup at all | ✓ | |
| Music reaction from a shared **Chrome tab** or the **screen** (no install) | ✓ Chrome/Edge on a computer | |
| Music reaction from the **microphone** | ✓ any browser (HTTPS) | |
| Music reaction from **BlackHole** loopback | ✓ if installed (macOS) | |
| Song names and covers via **Last.fm** | ✓ (the site needs a Last.fm API key) | |
| The public **ufoscene.net playlists** (decades, genres, Kite, computer music): open in Spotify | ✓ anyone, via their open.spotify.com links | create them: `node site/publish-playlists.mjs`, which writes the links into `web/playlists/index.json` |
| The same playlists: *Play in Spotify* from the page | with their own Spotify app (Premium) | ✓ (Premium) |
| **Spotify** (song names, artist pictures, skip, playlists from a text file) | with their *own* free Spotify developer app; the guide walks them through it | ✓ with the owner's Client ID |

## Build and look at it locally

```bash
cd site && ./build.sh
```

```bash
cargo build -p ufo-serve --release && PORT=8310 ./target/release/ufo-serve site/dist
```

Then open <http://127.0.0.1:8310/> (Spotify only accepts `127.0.0.1`, not `localhost`, in
redirect URIs). If cargo fails at link time with a stale `DEVELOPER_DIR` / `SDKROOT` (for example
from Nix), run `unset DEVELOPER_DIR SDKROOT` first.

## Files

| Path | What |
|------|------|
| `build.sh` | wasm build, copy of every file in `web/` (except `ufo.wasm`), `site-config.json`, `src/`, description tags, `404.html`, `_headers`, `.nojekyll` → `dist/` |
| `src/site.js`, `src/site.css` | website only: the support link and the optional extra scroll line |
| (`web/guides.html`) | the step-by-step guides for every setup, copied like the rest of `web/`; `build.sh` gives it its own description tags and canonical address (`<site>/guides.html`) |
| `config.example.json` | the site's settings; copy to `config.json` |
| `src/site.js` feature box | the one-line feature request at the bottom of the panel; posts to pi.ufoscene.net `/ideas` (`ideasUrl` in config.json, `""` turns it off) |
| `src/news.txt` | newest feature first; its top line shows as "New: …" in the panel, and for a few seconds on screen while the panel is closed. Add a line on top when a feature lands |
| `infra/` | **AWS**: Pulumi TypeScript project, S3 + CloudFront + optional domain |
| `deploy.sh` | AWS: `build.sh`, `pulumi up` for a stack, CloudFront invalidation |
| `register-domain.sh`, `domain-contact.example.json` | AWS: buy a domain in Route 53 Domains (asks first) |
| `wrangler.jsonc`, `deploy-cloudflare.sh` | alternative: Cloudflare Workers static assets |
| `netlify.toml` | alternative: Netlify |

## Settings

```bash
cd site && cp config.example.json config.json
```

`build.sh` copies `config.json` (or the empty example) to `dist/site-config.json`, so
**everything in it is public**. Keys left empty switch their feature off.

| Key | What |
|-----|------|
| `lastfmApiKey` | a Last.fm API key, so visitors only type their username. Create one at <https://www.last.fm/api/account/create>. Only the *API key* goes here, never the shared secret. Empty: the guide asks each visitor for a key of their own. |
| `spotifyClientId` | your Spotify app's Client ID, for the up to 5 people on its user list (the guide's "guest list" option). Empty: that option is hidden. |
| `supportUrl` | your tip page (Ko-fi, Buy Me a Coffee, GitHub Sponsors, …). Only `http(s)` links are shown, as a small link in the settings panel, never over a running demo. |
| `supportText` | the link text, default `Support UFO ♥` |
| `scrollLine` | one line added to every scroll text. It is used only if the wasm exports `ufo_set_scroll_extra`; otherwise it is ignored. |

## The setup guide

The guide is part of the app (`web/guide.js`), so it is the same locally (`./run.sh`) and on the
site. It opens on the first visit, and again with <kbd>G</kbd> or *Setup guide* in the panel
(<kbd>P</kbd>, which also pauses the demo and the music). On a published site it uses
`lastfmApiKey` and `spotifyClientId` from the settings; locally visitors paste their own.

1. **Just play:** 80s & 90s internet radio from radio-browser.info, no setup.
   <kbd>Space</kbd>/<kbd>B</kbd> change the station.
2. **Your own music**:
   - **a Chrome tab:** for example open.spotify.com; the music then plays from the UFO tab, so
     Cast tab takes it to the TV;
   - **an app:** *Entire screen* with system audio;
   - **BlackHole;**
   - **the microphone.**

   Start then uses that choice.
3. **Song names and artist photos**:
   - **Last.fm:** sign up, connect Spotify scrobbling, keep listening public, then a username
     with a *Check* button;
   - **Spotify with your own app:** dashboard link, the exact redirect URI with a Copy button,
     the Client ID, Connect;
   - **the owner's guest list;**
   - **none.**

Choosing Last.fm disconnects Spotify. While Spotify is connected, Last.fm stays off, so the two
never fight over the demo.

## Spotify on a public site

- Spotify apps in **development mode** allow **5 users**, each added by hand under *User
  Management*. Everybody else gets 403. The app owner needs **Spotify Premium**. These are the
  rules since February 2026.
- **Extended quota** (no allowlist) is only for organisations: a registered business, a
  launched service and at least 250,000 monthly active users.
- Redirect URIs must be **HTTPS**; only loopback IPs such as `http://127.0.0.1:8100` may use
  HTTP. `localhost` is refused, and the match is exact. One app can hold several redirect URIs,
  so a local copy and the website can share one Client ID.

So the owner and up to 5 people use the owner's Client ID. Any other visitor with Premium can
create their own free developer app, register the site's `https://<site>/callback` and paste
their Client ID. They are the owner of that app, so the 5-user cap doesn't get in their way.
Everyone else uses Last.fm.

## Audio without BlackHole

- **Tab audio** (`getDisplayMedia`): Chrome and Edge on macOS, Windows, Linux and ChromeOS. The
  shared tab goes quiet and UFO plays its sound, so Cast tab still carries the music.
- **Screen + system audio:** Windows (Chrome/Edge). On macOS it needs 14.2 or newer, a current
  Chrome, and the *Screen & System Audio Recording* permission. The sound keeps playing on the
  computer, so a Cast TV gets no music.
- Safari and Firefox don't offer audio in screen sharing; there only the microphone works.
- The **microphone** works everywhere on HTTPS. It hears the room, which is good enough to drive
  the demos with music from speakers.

## Hosting options

| | **AWS S3 + CloudFront** (`infra/`) | Cloudflare Workers static assets | GitHub Pages | Netlify |
|---|---|---|---|---|
| Deploy | `deploy.sh` (Pulumi) | `deploy-cloudflare.sh` (wrangler) | push `dist/` to a Pages branch or use an Action | `netlify-cli deploy` |
| `/callback` returns the page | CloudFront Function, status 200 | `single-page-application`, status 200 | `404.html` copy, status 404 (works) | rewrite rule, status 200 |
| Custom domain + HTTPS | ACM + Route 53 (automatic with a hosted zone) | zone in the same Cloudflare account | CNAME | yes |
| Cost | cents/month; CloudFront free tier 1 TB + 10 M requests/month; hosted zone US$0.50/month | free for static assets | free for public repos | free plan: 300 credits/month |

All of them serve `.wasm` as `application/wasm`. The page works under a sub-path too (such as
`user.github.io/ufo/`): it uses relative URLs and builds the redirect URI from its own address.

## Deploy to AWS (Pulumi)

`infra/` creates:

| Resource | What |
|----------|------|
| S3 bucket | private (all Block Public Access settings), `BucketOwnerEnforced`, `forceDestroy` |
| Origin Access Control + bucket policy | only this distribution may `s3:GetObject` (condition on `AWS:SourceArn`) |
| Files | every file in `dist/` except dot-files and `_headers`, as `aws.s3.BucketObjectv2` with an explicit `contentType` and an MD5 `sourceHash` (changed files upload again), `Cache-Control: max-age=60` because the names are not hashed (`ufo:maxAge`) |
| CloudFront Function (viewer request) | `/callback` and `/callback/` → `/index.html`, so the Spotify login lands on the page with status 200; `www.<domain>` → 301 to the bare domain |
| Cache policy | TTL 0 / 60 / 300 s, gzip + Brotli, no cookies, headers or query strings in the key |
| Response headers policy | `nosniff`, `X-Frame-Options: SAMEORIGIN`, `Referrer-Policy`, HSTS, and a `Permissions-Policy` that allows the microphone and screen capture for the page only |
| Distribution | HTTPS only (redirect), HTTP/2 + HTTP/3, IPv6, compression, root `index.html`, `PriceClass_100` (`ufo:priceClass`) |
| Optional domain | `ufo:domain` → ACM certificate in **us-east-1** for the domain and `www.` (`ufo:www`, default on); with `ufo:route53 true` (zone looked up by name) or `ufo:hostedZoneId`, also the validation records and `A`/`AAAA` aliases for both names in Route 53 |

Outputs: `url`, `spotifyRedirectUri` (exactly `https://<host>/callback`), `cloudfrontUrl`,
`distributionId`, `bucketName`, `fileCount`, `hostedZoneId`, plus `certificateValidationRecords` / `dnsToAdd`
when the domain's DNS is not in Route 53. `infra/Pulumi.dev.yaml` sets the bucket region
(`eu-north-1`) for a stack called `dev`; change it as you like.

Needs Node.js, the Pulumi CLI (logged in to a backend of your choice) and AWS credentials. Put
your own AWS profile name in place of `<your-profile>`.

```bash
cd site/infra && npm install
```

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi stack init dev
```

```bash
cd site && ./build.sh
```

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi preview -s dev
```

```bash
cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev
```

`deploy.sh` builds the site, runs `pulumi up` (with its usual confirmation), invalidates
CloudFront's cache (`/*`) and prints the site URL and the Spotify redirect URI. Add that URI
in the Spotify dashboard under your app → *Edit settings* → *Redirect URIs*.

**Buy the domain in Route 53.** `register-domain.sh` checks that the name is free, shows the
price and the AWS account, and buys it only after you type the domain name. A registration
cannot be refunded. It turns on auto-renew and WHOIS privacy, and Route 53 creates the domain's
hosted zone. ICANN emails the registrant address, and that link must be clicked within 15 days.
The script is not part of Pulumi, so `pulumi destroy` never touches the domain.

```bash
cd site && cp domain-contact.example.json domain-contact.json
```

Fill in your details (the file is git-ignored), then:

```bash
cd site && AWS_PROFILE=<your-profile> ./register-domain.sh example.net
```

```bash
cd site && AWS_PROFILE=<your-profile> ./register-domain.sh --status <operation-id>
```

**Custom domain in one command: `--domain`.** This sets up Route 53 DNS for the domain and
deploys:

```bash
cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev --domain ufoscene.net
```

- **Hosted zone:** if a public hosted zone for the domain exists in the account (for example from
  `register-domain.sh`), it is used. Otherwise Pulumi creates one (`ufo:createZone`), and
  `pulumi destroy` removes it again.
- **Registered in Route 53, same account:** Pulumi points the registration's name servers at that
  zone (`ufo:fixNameServers`), so DNS, the certificate and the `A`/`AAAA` records for the domain
  and `www` are all done in one run. This also repairs a registration whose name servers point
  somewhere else. It never buys, renews or deletes the domain.
- **Registered elsewhere:** the run creates the zone and prints its name servers under *Next*. Set
  them at your registrar, then finish:

  ```bash
  cd site && AWS_PROFILE=<your-profile> ./deploy.sh dev --name-servers-set
  ```

  The certificate validates once the name servers have propagated (usually minutes; Pulumi waits
  up to 2 hours), and the domain then points to CloudFront.

The settings `--domain` writes to the stack are listed in `infra/Pulumi.yaml`. You can also set
them by hand, as in the next paragraphs.

**Custom domain with DNS in Route 53, by hand** (bought there, or any domain with a public
hosted zone in the same account). When the registration says `SUCCESSFUL`:

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi config set ufo:domain ufoscene.net -s dev
```

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi config set ufo:route53 true -s dev
```

Then deploy again. The certificate validates in a few minutes, `ufoscene.net` and
`www.ufoscene.net` point to CloudFront, and `www` sends visitors on to the bare domain. To give
the zone by ID instead, set `ufo:hostedZoneId` (without `/hostedzone/`).

**Custom domain with DNS elsewhere**:

1. Set only `ufo:domain` and deploy.
2. Add the CNAMEs from the output `certificateValidationRecords` at your DNS provider.
3. When ACM (us-east-1) shows the certificate as *Issued*, set the flag below and deploy again.

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi config set ufo:certificateReady true -s dev
```

The output `dnsToAdd` then shows the records for the domain and `www`. The bare domain needs CNAME
flattening or ALIAS at your DNS provider.

Remove everything (the domain registration itself stays):

```bash
cd site/infra && AWS_PROFILE=<your-profile> pulumi destroy -s dev
```

## Deploy to Cloudflare (alternative)

`wrangler.jsonc` serves `dist/` as static assets with `single-page-application` fallback.
`deploy-cloudflare.sh [domain]` builds the site, adds a `custom_domain` route to a generated
`wrangler.deploy.jsonc` and runs `npx wrangler@4 deploy`. The domain must be a zone in the same
Cloudflare account.

```bash
cd site && npx wrangler@4 login
```

```bash
cd site && ./deploy-cloudflare.sh --dry-run
```

```bash
cd site && ./deploy-cloudflare.sh ufoscene.net
```

## Other hosts

- Netlify: `cd site && ./build.sh && npx netlify-cli deploy --prod --dir dist` (reads `netlify.toml`).
- GitHub Pages: publish the contents of `dist/` (it contains `404.html` and `.nojekyll`). The
  redirect URI is then `https://<user>.github.io/<repo>/callback`.

## Domain notes (November 2026)

- There is **no `.ufo` top-level domain** (not in the IANA root zone).
- **`.com`**: Route 53 charges US$16 a year. Cloudflare Registrar sells at cost, about US$10–11.
- **`.music`** is a community TLD (DotMusic):
  - You declare a connection to music, the registry can audit it, and an ID check is due
    within a year.
  - Prices vary widely by registrar (roughly US$14–60 a year).
  - Neither Route 53 nor Cloudflare Registrar sells it; buy it elsewhere and use "DNS elsewhere"
    above.
- To check whether a name is registered, ask the registry's RDAP server (`404` = not registered):

  ```bash
  curl -s -o /dev/null -w "%{http_code}\n" https://rdap.verisign.com/net/v1/domain/ufoscene.net
  ```

## Sources

- Spotify: [quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes),
  [redirect URIs](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri),
  [February 2026 migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide)
- Chrome: [screen sharing controls](https://developer.chrome.com/docs/web-platform/screen-sharing-controls)
- Cloudflare: [single-page application routing](https://developers.cloudflare.com/workers/static-assets/routing/single-page-application/),
  [limits](https://developers.cloudflare.com/workers/platform/limits/#static-assets)
- Netlify: [rewrites](https://docs.netlify.com/routing/redirects/rewrites-proxies/)
- AWS: [Route 53 supported TLDs](https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/registrar-tld-list.html)
- IANA: [root zone TLD list](https://data.iana.org/TLD/tlds-alpha-by-domain.txt)
