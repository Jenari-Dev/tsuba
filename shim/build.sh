#!/bin/sh
# Build both bridge DLLs without make: ./build.sh [outdir]   (default: build/)
set -eu
cd "$(dirname "$0")"
B="${1:-build}"
mkdir -p "$B/x64" "$B/x86"
CFLAGS="-O2 -std=c11 -Wall -Wextra -Wno-unused-parameter -municode -DUNICODE -D_UNICODE"
LDFLAGS="-shared -static-libgcc -Wl,--enable-stdcall-fixup -Wl,--no-insert-timestamp -s"
x86_64-w64-mingw32-windres -DBITS='\"64\"' -O coff -o "$B/x64/version.o" src/version.rc
i686-w64-mingw32-windres -DBITS='\"32\"' -O coff -o "$B/x86/version.o" src/version.rc
x86_64-w64-mingw32-gcc $CFLAGS -o "$B/x64/RzChromaSDK64.dll" src/bridge.c src/exports.def "$B/x64/version.o" $LDFLAGS -lws2_32 -lole32
i686-w64-mingw32-gcc $CFLAGS -o "$B/x86/RzChromaSDK.dll" src/bridge.c src/exports.def "$B/x86/version.o" $LDFLAGS -lws2_32 -lole32
echo "built $B/x64/RzChromaSDK64.dll $B/x86/RzChromaSDK.dll"
