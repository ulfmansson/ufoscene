#!/usr/bin/env bash
# Try ufo-native on a ufoscene Pi in place of the browser kiosk, and go back again.
# Run on the Pi, in the folder native/build-pi.sh filled (target/pi-native, copied over):
#
#   sudo ./try-native.sh bench      how fast this Pi draws the demos (nothing is changed)
#   sudo ./try-native.sh on         install the player and show it on the TV instead of Chromium
#   sudo ./try-native.sh off        back to Chromium (the player stays installed)
#   sudo ./try-native.sh status
#
# The player lives in /usr/local/lib/ufoscene-native, outside /opt/ufoscene, so updates leave
# it alone. A nightly update of ufoscene-pi starts the browser kiosk again: `on` once more.
set -euo pipefail
cd "$(dirname "$0")"

KIOSK_USER=${KIOSK_USER:-ufo}
DIR=/usr/local/lib/ufoscene-native
UNIT=/etc/systemd/system/ufoscene-native.service

die() { echo "try-native: $*" >&2; exit 1; }
[[ $(id -u) -eq 0 ]] || die "run it with sudo"
id "$KIOSK_USER" >/dev/null 2>&1 || die "no user $KIOSK_USER: is this a ufoscene Pi? (KIOSK_USER=<name> to override)"

case "$(uname -m)" in
  aarch64) BIN=ufo-native-arm64 ;;
  armv7l | armv8l) BIN=ufo-native-armhf ;;
  *) die "no ufo-native for $(uname -m) (a Pi 1 or Zero is too slow anyway)" ;;
esac

install_player() {
  [[ -f $BIN ]] || die "$BIN is not next to this script: run native/build-pi.sh and copy target/pi-native over"
  install -d "$DIR"
  install -m 755 "$BIN" "$DIR/ufo-native"
  local kuid; kuid=$(id -u "$KIOSK_USER")
  sed -e "s/@KUID@/$kuid/g" -e "s/@USER@/$KIOSK_USER/g" -e "s#@BIN@#$DIR/ufo-native#g" ufoscene-native.service > "$UNIT"
  systemctl daemon-reload
  # the radio: mpv plays it into PipeWire, the player listens to the output
  if ! command -v mpv >/dev/null; then
    echo "Installing mpv for the radio"
    apt-get install -y -qq --no-install-recommends mpv || echo "try-native: no mpv: the radio stays off, the phone and microphone work"
  fi
}

case "${1:-}" in
  bench)
    [[ -f $BIN ]] || die "$BIN is not next to this script"
    echo "Timing the demos for 60 s on this Pi, at 1920x1080 (the TV keeps showing what it shows)"
    nice -n 5 "./$BIN" --bench 60
    ;;
  on)
    install_player
    systemctl disable ufoscene-kiosk.service >/dev/null 2>&1 || true
    systemctl enable ufoscene-native.service
    systemctl stop ufoscene-kiosk.service || true
    systemctl restart ufoscene-native.service
    echo "ufo-native is on the TV. Log: journalctl -u ufoscene-native -f   Back: sudo $0 off"
    ;;
  off)
    systemctl disable ufoscene-native.service >/dev/null 2>&1 || true
    systemctl stop ufoscene-native.service || true
    systemctl enable ufoscene-kiosk.service
    systemctl restart ufoscene-kiosk.service
    echo "The browser kiosk is back on the TV."
    ;;
  status)
    systemctl --no-pager status ufoscene-native.service ufoscene-kiosk.service || true
    ;;
  *)
    sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
