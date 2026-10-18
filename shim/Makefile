# Tsuba game lighting bridge: Windows DLLs for games running in Wine / Proton.
# Needs mingw-w64 (x86_64-w64-mingw32-gcc, i686-w64-mingw32-gcc). `make` builds both into build/.
#   build/x64/RzChromaSDK64.dll   64-bit (system32 of a 64-bit prefix)
#   build/x86/RzChromaSDK.dll     32-bit (syswow64 of a 64-bit prefix, system32 of a 32-bit one)
# The file names are the ones games load; the code is Tsuba's own.

CC64 ?= x86_64-w64-mingw32-gcc
CC32 ?= i686-w64-mingw32-gcc
RC64 ?= x86_64-w64-mingw32-windres
RC32 ?= i686-w64-mingw32-windres
# Own flag variables: the host's CFLAGS/LDFLAGS (e.g. makepkg's -march=x86-64, -z relro) don't
# apply to the Windows cross builds.
SHIM_CFLAGS ?= -O2 -std=c11 -Wall -Wextra -Wno-unused-parameter -municode -DUNICODE -D_UNICODE
SHIM_LDFLAGS ?= -shared -static-libgcc -Wl,--enable-stdcall-fixup -Wl,--no-insert-timestamp -s
LIBS = -lws2_32 -lole32
B ?= build
SRC = src/bridge.c src/abi.h src/exports.def src/version.rc

all: $(B)/x64/RzChromaSDK64.dll $(B)/x86/RzChromaSDK.dll

$(B)/x64/version.o: src/version.rc
	@mkdir -p $(@D)
	$(RC64) -DBITS='\"64\"' -O coff -o $@ $<

$(B)/x86/version.o: src/version.rc
	@mkdir -p $(@D)
	$(RC32) -DBITS='\"32\"' -O coff -o $@ $<

$(B)/x64/RzChromaSDK64.dll: $(SRC) $(B)/x64/version.o
	$(CC64) $(SHIM_CFLAGS) -o $@ src/bridge.c src/exports.def $(B)/x64/version.o $(SHIM_LDFLAGS) $(LIBS)

$(B)/x86/RzChromaSDK.dll: $(SRC) $(B)/x86/version.o
	$(CC32) $(SHIM_CFLAGS) -o $@ src/bridge.c src/exports.def $(B)/x86/version.o $(SHIM_LDFLAGS) $(LIBS)

# Test harness (run by test/run.sh under wine).
test-bin: $(B)/x64/harness.exe $(B)/x86/harness.exe
$(B)/x64/harness.exe: test/harness.c src/abi.h
	@mkdir -p $(@D)
	$(CC64) -O1 -Wall -Isrc -o $@ test/harness.c
$(B)/x86/harness.exe: test/harness.c src/abi.h
	@mkdir -p $(@D)
	$(CC32) -O1 -Wall -Isrc -o $@ test/harness.c

test: all test-bin
	test/run.sh

clean:
	rm -rf $(B)

.PHONY: all test test-bin clean
