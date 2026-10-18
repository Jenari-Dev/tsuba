#!/bin/sh
# Runs the bridge DLLs (64- and 32-bit) under wine against the mock REST server and checks the
# requests that arrive. Uses a throwaway prefix: TSUBA_TEST_WINEPREFIX (default build/wineprefix).
# Never point it at a real game prefix.
set -eu
cd "$(dirname "$0")/.."
B="${B:-build}"
export WINEPREFIX="${TSUBA_TEST_WINEPREFIX:-$PWD/$B/wineprefix}"
export WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" WINEARCH=win64
work=$(mktemp -d)
trap 'kill $mock $evil 2>/dev/null || true; rm -rf "$work"' EXIT
evil=
[ -d "$WINEPREFIX" ] || wineboot -i >/dev/null 2>&1
python3 test/mock_server.py serve "$work/port" "$work/requests.jsonl" &
mock=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$work/port" ] && break; sleep 0.2; done
export TSUBA_GAME_LIGHTING_PORT="$(cat "$work/port")"
export TSUBA_GAME_LIGHTING_LOG="$work/bridge.log"
cp "$B/x64/RzChromaSDK64.dll" "$B/x86/RzChromaSDK.dll" "$work/"
echo "== 64-bit"
wine "$B/x64/harness.exe" "$(winepath -w "$work/RzChromaSDK64.dll")" full
echo "== 32-bit"
wine "$B/x86/harness.exe" "$(winepath -w "$work/RzChromaSDK.dll")" full
python3 test/mock_server.py check "$work/requests.jsonl"
echo "== offline (nothing listening)"
TSUBA_GAME_LIGHTING_PORT=9 wine "$B/x64/harness.exe" "$(winepath -w "$work/RzChromaSDK64.dll")" offline
TSUBA_GAME_LIGHTING_PORT=9 wine "$B/x86/harness.exe" "$(winepath -w "$work/RzChromaSDK.dll")" offline
echo "== hostile server (malformed / chunked replies must be refused, calls must stay fast)"
python3 test/evil_server.py "$work/evil-port" "$work/evil-count" &
evil=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$work/evil-port" ] && break; sleep 0.2; done
ncases=$(python3 test/evil_server.py cases)
: > "$work/evil.log"
runs=0
while [ "$(cat "$work/evil-count" 2>/dev/null || echo 0)" -lt "$ncases" ]; do
    runs=$((runs + 1))
    [ "$runs" -le $((ncases * 2)) ] || { echo "FAIL: hostile server saw only $(cat "$work/evil-count" 2>/dev/null) connections"; exit 1; }
    if [ $((runs % 2)) -eq 0 ]; then dll="$B/x86/RzChromaSDK.dll" exe="$B/x86/harness.exe"; else dll="$B/x64/RzChromaSDK64.dll" exe="$B/x64/harness.exe"; fi
    TSUBA_GAME_LIGHTING_PORT="$(cat "$work/evil-port")" TSUBA_GAME_LIGHTING_LOG="$work/evil.log" \
        WINEDEBUG=-all timeout 60 wine "$exe" "$(winepath -w "$work/$(basename "$dll")")" offline >"$work/evil-run.txt" 2>&1 \
        || { cat "$work/evil-run.txt"; echo "FAIL: harness against the hostile server (crashed, hung or a call failed)"; exit 1; }
done
bad=$(grep "not reachable" "$work/evil.log" | grep -vc "(-1)" || true)
ok=$(grep "not reachable" "$work/evil.log" | grep -c "(-1)" || true)
if [ "$bad" -ne 0 ] || [ "$ok" -lt "$ncases" ] || grep -q " session " "$work/evil.log"; then
    cat "$work/evil.log"
    echo "FAIL: the bridge accepted a malformed reply ($bad accepted, $ok refused)"
    exit 1
fi
echo "hostile server: $ncases malformed replies refused over $runs harness runs"
if [ -n "${KEEP_LOG:-}" ]; then cp "$work/bridge.log" "$work/requests.jsonl" "$KEEP_LOG"/; fi
echo "all bridge tests passed"
