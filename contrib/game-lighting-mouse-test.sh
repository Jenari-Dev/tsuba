#!/usr/bin/env bash
# Mouse-only game lighting check: three ways a game can colour the mouse, 4 s each.
set -euo pipefail
PORT="${PORT:-54235}"
json() { curl -s -X "$1" -H 'Content-Type: application/json' --data "$3" "$2"; }
INIT='{"title":"Tsuba mouse test","description":"contrib/game-lighting-mouse-test.sh","author":{"name":"Tsuba","contact":"localhost"},"device_supported":["mouse"],"category":"application"}'
URI=$(json POST "http://127.0.0.1:$PORT/razer/chromasdk" "$INIT" | sed -n 's/.*"uri" *: *"\([^"]*\)".*/\1/p' | sed 's/localhost/127.0.0.1/')
echo "session $URI"
echo "1) mouse solid RED (direct static effect)"
json PUT "$URI/mouse" '{"effect":"CHROMA_STATIC","param":{"color":255}}'; echo; sleep 4
echo "2) mouse solid GREEN (stored effect applied by id)"
ID=$(json POST "$URI/mouse" '{"effect":"CHROMA_STATIC","param":{"color":65280}}' | sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p')
json PUT "$URI/effect" "{\"id\":\"$ID\"}"; echo; sleep 4
echo "3) mouse BLUE grid with a WHITE logo (9x7 custom grid)"
R='[16711680,16711680,16711680,16711680,16711680,16711680,16711680]'
L='[16711680,16711680,16711680,16777215,16711680,16711680,16711680]'
json PUT "$URI/mouse" "{\"effect\":\"CHROMA_CUSTOM2\",\"param\":[$R,$R,$R,$R,$R,$R,$R,$L,$R]}"; echo; sleep 4
json DELETE "$URI" '' >/dev/null; echo "done — normal lighting should be back"
