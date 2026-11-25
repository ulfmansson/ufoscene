#!/usr/bin/env zsh
# Build the UFO website into site/dist/: the wasm build (same as run.sh) plus a
# copy of web/ with the website-only adjustments applied to the copies.
# web/ itself is never touched.
#
#   cd site && ./build.sh
#
# Then deploy dist/ (see site/README.md). Nothing here publishes anything.
set -euo pipefail
SITE="${0:A:h}"
ROOT="${SITE:h}"
DIST="$SITE/dist"
cd "$ROOT"

# A stale Nix SDK path in the environment breaks the macOS linker (same guard as run.sh).
if [[ -n "${DEVELOPER_DIR:-}" && ! -d "$DEVELOPER_DIR" ]]; then unset DEVELOPER_DIR SDKROOT; fi

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  rustup target add wasm32-unknown-unknown
fi

cargo build -p ufo-demo --release --target wasm32-unknown-unknown

rm -rf "$DIST"
mkdir -p "$DIST"
# everything in web/ (new modules such as pictures.js and folders such as playlists/ come
# along), except the wasm that run.sh copies there (the fresh one comes from target/) and
# dot-files/-folders (a glob without D skips those)
for f in web/*(N); do
  [[ "${f:t}" == ufo.wasm ]] || cp -R "$f" "$DIST/"
done
for f in index.html ufo.js spotify.js; do
  [[ -f "$DIST/$f" ]] || { print -u2 "build.sh: web/$f is missing"; exit 1; }
done
cp target/wasm32-unknown-unknown/release/ufo_demo.wasm "$DIST/ufo.wasm"

# --- website-only adjustments -------------------------------------------------
# Each change is skipped when the copy already has it, and the build stops if it
# cannot be made, so a change in web/ cannot slip through silently.
fix() { # fix <file> <perl substitution> <text that must exist afterwards>
  local file="$DIST/$1" subst="$2" want="$3"
  grep -qF -- "$want" "$file" && return 0
  perl -0pi -e "$subst" "$file"
  if ! grep -qF -- "$want" "$file"; then
    print -u2 "build.sh: could not adjust $1 (looked for: $want); web/ changed, update site/build.sh"
    exit 1
  fi
}

# Website-only layer (site/src): support link and the optional extra scroll line.
cp "$SITE"/src/site.js "$SITE"/src/site.css "$DIST/"
fix index.html 's#</head>#<link rel="stylesheet" href="site.css">\n</head>#' '<link rel="stylesheet" href="site.css">'
fix index.html 's#</body>#<script type="module" src="site.js"></script>\n</body>#' '<script type="module" src="site.js"></script>'
# Search results and link previews. The page title itself comes from web/index.html.
# SITE_URL=https://other.example ./build.sh for a copy on another address.
SITE_URL="${SITE_URL:-https://ufoscene.net/}"
SITE_NAME="${SITE_URL#https://}"
SITE_NAME="${SITE_NAME%/}"
TAGLINE='100 Amiga-style demos that dance to your music. A hymn to the Swedish demo scene.'
META="<link rel=\"canonical\" href=\"$SITE_URL\">
<meta name=\"description\" content=\"$TAGLINE\">
<meta property=\"og:type\" content=\"website\">
<meta property=\"og:url\" content=\"$SITE_URL\">
<meta property=\"og:site_name\" content=\"$SITE_NAME\">
<meta property=\"og:title\" content=\"$SITE_NAME · A hymn to the Swedish demo scene\">
<meta property=\"og:description\" content=\"$TAGLINE\">"
META="$META" perl -0pi -e 's#(</title>)#$1\n$ENV{META}#' "$DIST/index.html"
grep -qF '<meta property="og:url"' "$DIST/index.html" || { print -u2 "build.sh: could not add the meta tags"; exit 1; }
# The step-by-step guides (web/guides.html) are a page of their own: same tags, own address.
if [[ -f "$DIST/guides.html" ]]; then
  GUIDES_URL="${SITE_URL%/}/guides.html"
  GUIDES_TAG='Step by step: run the demos in the browser, on a Mac, Windows, Linux, a phone, the TV or a Raspberry Pi, or host your own copy.'
  META="<link rel=\"canonical\" href=\"$GUIDES_URL\">
<meta name=\"description\" content=\"$GUIDES_TAG\">
<meta property=\"og:type\" content=\"article\">
<meta property=\"og:url\" content=\"$GUIDES_URL\">
<meta property=\"og:site_name\" content=\"$SITE_NAME\">
<meta property=\"og:title\" content=\"Guides · $SITE_NAME\">
<meta property=\"og:description\" content=\"$GUIDES_TAG\">"
  META="$META" perl -0pi -e 's#(</title>)#$1\n$ENV{META}#' "$DIST/guides.html"
  grep -qF '<meta property="og:url"' "$DIST/guides.html" || { print -u2 "build.sh: could not add the meta tags to guides.html"; exit 1; }
fi
# Settings for this site: site/config.json (yours, see config.example.json) or the empty example.
CONFIG="$SITE/config.json"
[[ -f "$CONFIG" ]] || CONFIG="$SITE/config.example.json"
python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$CONFIG" || { print -u2 "build.sh: $CONFIG is not valid JSON"; exit 1; }
cp "$CONFIG" "$DIST/site-config.json"
print "settings: ${CONFIG:t}"

# The page must also answer on …/callback, where Spotify sends the browser back.
#  - Cloudflare (site/wrangler.jsonc) serves index.html for it (single-page-application).
#  - Netlify (site/netlify.toml) rewrites it to index.html.
#  - GitHub Pages has no rewrites: it serves 404.html for unknown paths, so 404.html
#    is the page itself (status 404, but the browser runs it all the same).
cp "$DIST/index.html" "$DIST/404.html"
touch "$DIST/.nojekyll" # GitHub Pages: publish as-is, no Jekyll

# Response headers (Cloudflare and Netlify read _headers; other hosts ignore the file).
# Microphone and screen/tab capture stay allowed for the page itself, nothing else.
cat > "$DIST/_headers" <<'EOF'
/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
  Permissions-Policy: microphone=(self), display-capture=(self), camera=(), geolocation=()
  X-Frame-Options: SAMEORIGIN
EOF

print "site/dist ready: $(( $(wc -c < "$DIST/ufo.wasm") / 1024 )) KiB wasm, $(ls "$DIST" | wc -l | tr -d ' ') files"
