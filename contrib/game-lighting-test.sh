#!/bin/sh
# Game lighting smoke test: acts like a game that supports Razer Chroma, using tsubad's
# local REST server. Opens a session, paints the keyboard red, then green, highlights a few
# keys, paints the mouse, then ends the session (your own lighting comes back).
#
#   contrib/game-lighting-test.sh            # tsubad on the default port 54235
#   PORT=54299 contrib/game-lighting-test.sh # a test daemon started with TSUBA_GAME_LIGHTING_PORT=54299
#
# Needs curl. Writes nothing itself: tsubad decides which devices it may light.
set -eu

PORT=${PORT:-54235}
BASE="http://127.0.0.1:$PORT/razer/chromasdk"
STEP=${STEP:-3}

json() { curl -sf -X "$1" -H 'Content-Type: application/json' ${3:+--data "$3"} "$2"; }

echo "tsubad game lighting on port $PORT:"
json GET "$BASE" || { echo "no answer — is tsubad running with game lighting on (GetGameLighting)?" >&2; exit 1; }
echo

INIT='{"title":"Tsuba game lighting test","description":"contrib/game-lighting-test.sh","author":{"name":"Tsuba","contact":"localhost"},"device_supported":["keyboard","mouse"],"category":"application"}'
REPLY=$(json POST "$BASE" "$INIT")
echo "session: $REPLY"
URI=$(printf '%s' "$REPLY" | sed -n 's/.*"uri" *: *"\([^"]*\)".*/\1/p')
[ -n "$URI" ] || { echo "no session uri in the reply" >&2; exit 1; }
# The uri (http://localhost:<port>/chromasdk/<session token>) is used verbatim; only localhost is
# swapped for the IPv4 loopback the server listens on.
URI=$(printf '%s' "$URI" | sed 's#//localhost:#//127.0.0.1:#')

# Heartbeat every second while the test runs (sessions expire after 15 s of silence).
( while :; do json PUT "$URI/heartbeat" >/dev/null 2>&1 || exit 0; sleep 1; done ) &
BEAT=$!
cleanup() { kill "$BEAT" 2>/dev/null || true; json DELETE "$URI" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

echo "keyboard red";   json PUT "$URI/keyboard" '{"effect":"CHROMA_STATIC","param":{"color":255}}'; echo; sleep "$STEP"
echo "keyboard green"; json PUT "$URI/keyboard" '{"effect":"CHROMA_STATIC","param":{"color":65280}}'; echo; sleep "$STEP"

# Dim blue keyboard with W A S D (and Esc) highlighted white via the key overlay
# (6×22 grid; key colour = 0x01000000 | BGR).
ROW='[4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304,4194304]'
COLOR="[$ROW,$ROW,$ROW,$ROW,$ROW,$ROW]"
Z='0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0'
K=33554431  # 0x01000000 | 0xFFFFFF
KEYS="[[0,$K,0,0,$Z],[0,0,0,0,$Z],[0,0,0,$K,$Z],[0,0,$K,$K,$K,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],[0,0,0,0,$Z],[0,0,0,0,$Z]]"
echo "keyboard blue + W A S D / Esc"
json PUT "$URI/keyboard" "{\"effect\":\"CHROMA_CUSTOM_KEY\",\"param\":{\"color\":$COLOR,\"key\":$KEYS}}"; echo; sleep "$STEP"

echo "mouse purple (stored effect, then applied by id)"
EFFECT=$(json POST "$URI/mouse" '{"effect":"CHROMA_STATIC","param":{"color":16711935}}')
ID=$(printf '%s' "$EFFECT" | sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p')
json PUT "$URI/effect" "{\"id\":\"$ID\"}"; echo; sleep "$STEP"
json DELETE "$URI/effect" "{\"id\":\"$ID\"}" >/dev/null

echo "ending the session — your own lighting should come back"
