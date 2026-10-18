/* SPDX-License-Identifier: GPL-2.0-or-later
 *
 * Tsuba game lighting bridge.
 *
 * A Windows DLL that games load (inside Wine / Proton) in place of the native lighting SDK of
 * "games that support Razer Chroma". It answers the SDK's C entry points and forwards the
 * effects to Tsuba's local REST server (tsubad, 127.0.0.1:54235 by default) over plain HTTP/1.1.
 *
 * Design
 *  - Every entry point returns immediately: effects are turned into REST JSON bodies on the
 *    caller's thread (pure CPU work), stored under an id, and SetEffect only drops the body into a
 *    one-slot-per-device mailbox. A single worker thread does all networking: it creates the
 *    REST session, sends the newest frame per device (older unsent frames are simply replaced,
 *    i.e. coalesced / dropped), sends a heartbeat every second, and deletes the session after
 *    UnInit.
 *  - If the server can't be reached the game still gets success; the worker retries quietly
 *    with back-off and, once connected, re-sends the last frame of every device.
 *  - The worker holds its own reference on this DLL and leaves through FreeLibraryAndExitThread,
 *    so FreeLibrary() by the game can never unload code that is still running.
 *
 * Environment: TSUBA_GAME_LIGHTING_PORT (default 54235), TSUBA_GAME_LIGHTING_LOG (file to append
 * a debug log to).
 */
#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <objbase.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

#include "abi.h"

#define API __declspec(dllexport) tb_result __cdecl

/* ---- devices -------------------------------------------------------------------------------- */

enum { DEV_KEYBOARD, DEV_MOUSE, DEV_MOUSEPAD, DEV_HEADSET, DEV_KEYPAD, DEV_LINK, DEV_COUNT, DEV_NONE = -1 };
static const char *const DEV_PATH[DEV_COUNT] = { "keyboard", "mouse", "mousepad", "headset", "keypad", "chromalink" };

/* Published device ids → device kind (needed so generic CreateEffect / QueryDevice calls work).
 * Systems and speakers are driven through the Chroma Link endpoint. */
static const struct { GUID id; int kind; } KNOWN_IDS[] = {
    { { 0x2ea1bb63, 0xca28, 0x428d, { 0x9f, 0x06, 0x19, 0x6b, 0x88, 0x33, 0x0b, 0xbb } }, TB_KIND_KEYBOARD },
    { { 0xed1c1b82, 0xbfbe, 0x418f, { 0xb4, 0x9d, 0xd0, 0x3f, 0x05, 0xb1, 0x49, 0xdf } }, TB_KIND_KEYBOARD },
    { { 0x18c5ad9b, 0x4326, 0x4828, { 0x92, 0xc4, 0x26, 0x69, 0xa6, 0x6d, 0x22, 0x83 } }, TB_KIND_KEYBOARD },
    { { 0x872ab2a9, 0x7959, 0x4478, { 0x9f, 0xed, 0x15, 0xf6, 0x18, 0x6e, 0x72, 0xe4 } }, TB_KIND_KEYBOARD },
    { { 0x5af60076, 0xade9, 0x43d4, { 0xb5, 0x74, 0x52, 0x59, 0x92, 0x93, 0xb5, 0x54 } }, TB_KIND_KEYBOARD },
    { { 0x2d84dd51, 0x3290, 0x4aac, { 0x9a, 0x89, 0xd8, 0xaf, 0xde, 0x38, 0xb5, 0x7c } }, TB_KIND_KEYBOARD },
    { { 0x803378c1, 0xcc48, 0x4970, { 0x85, 0x39, 0xd8, 0x28, 0xcc, 0x1d, 0x42, 0x0a } }, TB_KIND_KEYBOARD },
    { { 0xc83bdfe8, 0xe7fc, 0x40e0, { 0x99, 0xdb, 0x87, 0x2e, 0x23, 0xf1, 0x98, 0x91 } }, TB_KIND_KEYBOARD },
    { { 0xf2bedfaf, 0xa0fe, 0x4651, { 0x9d, 0x41, 0xb6, 0xce, 0x60, 0x3a, 0x3d, 0xdd } }, TB_KIND_KEYBOARD },
    { { 0xa73ac338, 0xf0e5, 0x4bf7, { 0x91, 0xae, 0xdd, 0x1f, 0x7e, 0x17, 0x37, 0xa5 } }, TB_KIND_KEYBOARD },
    { { 0xf85e7473, 0x8f03, 0x45b6, { 0xa1, 0x6e, 0xce, 0x26, 0xcb, 0x8d, 0x24, 0x41 } }, TB_KIND_KEYBOARD },
    { { 0x16bb5abd, 0xc1cd, 0x4cb3, { 0xbd, 0xf7, 0x62, 0x43, 0x87, 0x48, 0xbd, 0x98 } }, TB_KIND_KEYBOARD },
    { { 0xaec50d91, 0xb1f1, 0x452f, { 0x8e, 0x16, 0x7b, 0x73, 0xf3, 0x76, 0xfd, 0xf3 } }, TB_KIND_MOUSE },
    { { 0x7ec00450, 0xe0ee, 0x4289, { 0x89, 0xd5, 0x0d, 0x87, 0x9c, 0x19, 0x06, 0x1a } }, TB_KIND_MOUSE },
    { { 0xff8a5929, 0x4512, 0x4257, { 0x8d, 0x59, 0xc6, 0x47, 0xbf, 0x99, 0x35, 0xd0 } }, TB_KIND_MOUSE },
    { { 0xd527cbdc, 0xeb0a, 0x483a, { 0x9e, 0x89, 0x66, 0xd5, 0x04, 0x63, 0xec, 0x6c } }, TB_KIND_MOUSE },
    { { 0xd714c50b, 0x7158, 0x4368, { 0xb9, 0x9c, 0x60, 0x1a, 0xcb, 0x98, 0x5e, 0x98 } }, TB_KIND_MOUSE },
    { { 0xf1876328, 0x6ca4, 0x46ae, { 0xbe, 0x04, 0xbe, 0x81, 0x2b, 0x41, 0x44, 0x33 } }, TB_KIND_MOUSE },
    { { 0x52c15681, 0x4ece, 0x4dd9, { 0x8a, 0x52, 0xa1, 0x41, 0x84, 0x59, 0xeb, 0x34 } }, TB_KIND_MOUSE },
    { { 0x195d70f5, 0xf285, 0x4cff, { 0x99, 0xf2, 0xb8, 0xc0, 0xe9, 0x65, 0x8d, 0xb4 } }, TB_KIND_MOUSE },
    { { 0x77834867, 0x3237, 0x4a9f, { 0xad, 0x77, 0x4a, 0x46, 0xc4, 0x18, 0x30, 0x03 } }, TB_KIND_MOUSE },
    { { 0xcd1e09a5, 0xd5e6, 0x4a6c, { 0xa9, 0x3b, 0xe6, 0xd9, 0xbf, 0x1d, 0x20, 0x92 } }, TB_KIND_HEADSET },
    { { 0xdf3164d7, 0x5408, 0x4a0e, { 0x8a, 0x7f, 0xa7, 0x41, 0x2f, 0x26, 0xbe, 0xbf } }, TB_KIND_HEADSET },
    { { 0x7fb8a36e, 0x9e74, 0x4bb3, { 0x8c, 0x86, 0xca, 0xc7, 0xf7, 0x89, 0x1e, 0xbd } }, TB_KIND_HEADSET },
    { { 0xfb357780, 0x4617, 0x43a7, { 0x96, 0x0f, 0xd1, 0x19, 0x0e, 0xd5, 0x48, 0x06 } }, TB_KIND_HEADSET },
    { { 0x80f95a94, 0x73d2, 0x48ca, { 0xae, 0x9a, 0x09, 0x86, 0x78, 0x9a, 0x9a, 0xf2 } }, TB_KIND_MOUSEPAD },
    { { 0x00f0545c, 0xe180, 0x4ad1, { 0x8e, 0x8a, 0x41, 0x90, 0x61, 0xce, 0x50, 0x5e } }, TB_KIND_KEYPAD },
    { { 0x9d24b0ab, 0x0162, 0x466c, { 0x96, 0x40, 0x7a, 0x92, 0x4a, 0xa4, 0xd9, 0xfd } }, TB_KIND_KEYPAD },
    { { 0x35f6f18d, 0x1ae5, 0x436c, { 0xa5, 0x75, 0xab, 0x44, 0xa1, 0x27, 0x90, 0x3a } }, TB_KIND_SYSTEM },
    { { 0x47db1fa7, 0x6b9b, 0x4ee6, { 0xb6, 0xf4, 0x40, 0x71, 0xa3, 0xb2, 0x05, 0x3b } }, TB_KIND_SYSTEM },
    { { 0x0201203b, 0x62f3, 0x4c50, { 0x83, 0xdd, 0x59, 0x8b, 0xab, 0xd2, 0x08, 0xe0 } }, TB_KIND_CHROMALINK },
    { { 0xbb2e9c9b, 0xb0d2, 0x461a, { 0xba, 0x52, 0x23, 0x0b, 0x5d, 0x6c, 0x36, 0x09 } }, TB_KIND_CHROMALINK },
    { { 0x45b308f2, 0xcd44, 0x4594, { 0x83, 0x75, 0x4d, 0x59, 0x45, 0xad, 0x88, 0x0e } }, TB_KIND_SPEAKERS },
    { { 0x3017280b, 0xd7f9, 0x4d7b, { 0x93, 0x0e, 0x7b, 0x47, 0x18, 0x1b, 0x46, 0xb5 } }, TB_KIND_SPEAKERS },
};

static int kind_of_id(const GUID *id) {
    for (size_t i = 0; i < sizeof(KNOWN_IDS) / sizeof(KNOWN_IDS[0]); i++) {
        if (memcmp(&KNOWN_IDS[i].id, id, sizeof(GUID)) == 0) {
            return KNOWN_IDS[i].kind;
        }
    }
    return TB_KIND_INVALID;
}

static int dev_of_kind(int kind) {
    switch (kind) {
    case TB_KIND_KEYBOARD: return DEV_KEYBOARD;
    case TB_KIND_MOUSE: return DEV_MOUSE;
    case TB_KIND_MOUSEPAD: return DEV_MOUSEPAD;
    case TB_KIND_HEADSET: return DEV_HEADSET;
    case TB_KIND_KEYPAD: return DEV_KEYPAD;
    case TB_KIND_CHROMALINK:
    case TB_KIND_SYSTEM:
    case TB_KIND_SPEAKERS: return DEV_LINK;
    default: return DEV_NONE;
    }
}

/* ---- state ---------------------------------------------------------------------------------- */

typedef struct effect {
    GUID id;
    int dev;   /* DEV_* or DEV_NONE (accepted but nothing to show) */
    char *body; /* REST JSON body for PUT /<device> */
    struct effect *next;
} effect;

#define EFFECT_BUCKETS 512
#define EFFECT_LIMIT 20000 /* a game that never deletes its effects can't eat all memory */

static CRITICAL_SECTION g_lock;
static effect *g_effects[EFFECT_BUCKETS];
static unsigned g_effect_count;
static char *g_pending[DEV_COUNT]; /* newest unsent frame per device (owned) */
static char *g_appinfo;            /* REST init body (owned) */
static int g_wanted;               /* between Init and UnInit */
static int g_worker_running;
static HANDLE g_wake;
/* Which device classes QueryDevice reports as connected. The REST interface has no device query,
 * so every class counts as connected (tsubad decides what each class drives) unless a server reply
 * carries an optional "devices": [...] list. */
static volatile LONG g_connected_mask = (1 << DEV_COUNT) - 1;
static FILE *g_log;

static void logf_(const char *fmt, ...) {
    if (!g_log) {
        return;
    }
    va_list ap;
    va_start(ap, fmt);
    SYSTEMTIME t;
    GetLocalTime(&t);
    fprintf(g_log, "%02d:%02d:%02d.%03d ", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds);
    vfprintf(g_log, fmt, ap);
    fputc('\n', g_log);
    fflush(g_log);
    va_end(ap);
}

static unsigned bucket(const GUID *id) {
    const unsigned char *b = (const unsigned char *)id;
    unsigned h = 2166136261u;
    for (int i = 0; i < 16; i++) {
        h = (h ^ b[i]) * 16777619u;
    }
    return h % EFFECT_BUCKETS;
}

static effect *find_effect(const GUID *id) {
    for (effect *e = g_effects[bucket(id)]; e; e = e->next) {
        if (memcmp(&e->id, id, sizeof(GUID)) == 0) {
            return e;
        }
    }
    return NULL;
}

static void free_all_effects(void) {
    for (int i = 0; i < EFFECT_BUCKETS; i++) {
        effect *e = g_effects[i];
        while (e) {
            effect *n = e->next;
            free(e->body);
            free(e);
            e = n;
        }
        g_effects[i] = NULL;
    }
    g_effect_count = 0;
}

/* ---- JSON building -------------------------------------------------------------------------- */

typedef struct {
    char *p;
    size_t n, cap;
    int bad;
} jbuf;

static void jb_raw(jbuf *b, const char *s, size_t len) {
    if (b->bad) {
        return;
    }
    if (b->n + len + 1 > b->cap) {
        size_t cap = b->cap ? b->cap : 256;
        while (b->n + len + 1 > cap) {
            cap *= 2;
        }
        char *p = realloc(b->p, cap);
        if (!p) {
            b->bad = 1;
            return;
        }
        b->p = p;
        b->cap = cap;
    }
    memcpy(b->p + b->n, s, len);
    b->n += len;
    b->p[b->n] = 0;
}
static void jb_s(jbuf *b, const char *s) { jb_raw(b, s, strlen(s)); }
static void jb_u(jbuf *b, unsigned long v) {
    char t[16];
    int n = snprintf(t, sizeof t, "%lu", v);
    jb_raw(b, t, (size_t)n);
}
/* A JSON string from UTF-8 text. */
static void jb_str(jbuf *b, const char *s) {
    jb_s(b, "\"");
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') {
            char t[2] = { '\\', (char)c };
            jb_raw(b, t, 2);
        } else if (c < 0x20) {
            char t[8];
            snprintf(t, sizeof t, "\\u%04x", c);
            jb_s(b, t);
        } else {
            jb_raw(b, (const char *)&c, 1);
        }
    }
    jb_s(b, "\"");
}
static char *jb_done(jbuf *b) {
    if (b->bad) {
        free(b->p);
        return NULL;
    }
    return b->p;
}

/* 2-D colour array; `mask` keeps the flag bits callers want (0x01FFFFFF for key grids). */
static void jb_grid(jbuf *b, const tb_color *cells, int rows, int cols, tb_color mask) {
    jb_s(b, "[");
    for (int r = 0; r < rows; r++) {
        jb_s(b, r ? ",[" : "[");
        for (int c = 0; c < cols; c++) {
            if (c) {
                jb_s(b, ",");
            }
            jb_u(b, cells[r * cols + c] & mask);
        }
        jb_s(b, "]");
    }
    jb_s(b, "]");
}
static void jb_list(jbuf *b, const tb_color *cells, int n, int pad_to) {
    jb_s(b, "[");
    for (int i = 0; i < (n > pad_to ? n : pad_to); i++) {
        if (i) {
            jb_s(b, ",");
        }
        jb_u(b, i < n ? (cells[i] & 0xFFFFFF) : 0);
    }
    jb_s(b, "]");
}

#define RGB_MASK 0x00FFFFFFu
#define KEY_MASK 0x01FFFFFFu

static char *body_simple(const char *effect) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"");
    jb_s(&b, effect);
    jb_s(&b, "\"}");
    return jb_done(&b);
}
static char *body_static(tb_color c) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"CHROMA_STATIC\",\"param\":{\"color\":");
    jb_u(&b, c & RGB_MASK);
    jb_s(&b, "}}");
    return jb_done(&b);
}
static char *body_breathing(int type, tb_color c1, tb_color c2) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"CHROMA_BREATHING\",\"param\":{\"type\":");
    jb_u(&b, (unsigned long)(type == 2 ? 2 : 1));
    jb_s(&b, ",\"color1\":");
    jb_u(&b, c1 & RGB_MASK);
    jb_s(&b, ",\"color2\":");
    jb_u(&b, c2 & RGB_MASK);
    jb_s(&b, "}}");
    return jb_done(&b);
}
static char *body_wave(int direction /* 1 = left→right / front→back, 2 = the other way */) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"CHROMA_WAVE\",\"param\":{\"direction\":");
    jb_u(&b, (unsigned long)(direction == 2 ? 2 : 1));
    jb_s(&b, "}}");
    return jb_done(&b);
}
static char *body_custom_grid(const char *effect, const tb_color *cells, int rows, int cols) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"");
    jb_s(&b, effect);
    jb_s(&b, "\",\"param\":");
    jb_grid(&b, cells, rows, cols, RGB_MASK);
    jb_s(&b, "}");
    return jb_done(&b);
}
static char *body_custom_list(const tb_color *cells, int n, int pad_to) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"CHROMA_CUSTOM\",\"param\":");
    jb_list(&b, cells, n, pad_to);
    jb_s(&b, "}");
    return jb_done(&b);
}
static char *body_custom_keys(const char *effect, const tb_color *colour, int rows, int cols, const tb_color *key) {
    jbuf b = { 0 };
    jb_s(&b, "{\"effect\":\"");
    jb_s(&b, effect);
    jb_s(&b, "\",\"param\":{\"color\":");
    jb_grid(&b, colour, rows, cols, RGB_MASK);
    jb_s(&b, ",\"key\":");
    jb_grid(&b, key, TB_KB_ROWS, TB_KB_COLS, KEY_MASK);
    jb_s(&b, "}}");
    return jb_done(&b);
}

/* Old single-LED mouse ids → cell in the 9×7 mouse grid (row << 8 | column). */
static int mouse_led_cell(int led) {
    switch (led) {
    case 1: return 0x0203; /* scroll wheel */
    case 2: return 0x0703; /* logo */
    case 3: return 0x0403; /* backlight */
    default:
        if (led >= 4 && led <= 10) {
            return (led - 3) << 8; /* left strip rows 1..7, column 0 */
        }
        if (led >= 11 && led <= 17) {
            return ((led - 10) << 8) | 6; /* right strip rows 1..7, column 6 */
        }
        return -1;
    }
}

/* A colour on one mouse LED, or the whole mouse for "all"/unknown ids. */
static char *mouse_led_body(int led, tb_color c) {
    int cell = mouse_led_cell(led);
    if (led == TB_MOUSE_LED_ALL || cell < 0) {
        return body_static(c);
    }
    tb_color g[TB_MOUSE_ROWS][TB_MOUSE_COLS];
    memset(g, 0, sizeof g);
    g[cell >> 8][cell & 0xff] = c;
    return body_custom_grid("CHROMA_CUSTOM2", &g[0][0], TB_MOUSE_ROWS, TB_MOUSE_COLS);
}

/* ---- effect → REST body, per device --------------------------------------------------------- */

#define NEED(p) \
    do { \
        if (!(p)) { \
            *rc = TB_INVALID_PARAMETER; \
            return NULL; \
        } \
    } while (0)

static char *keyboard_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_KB_NONE: return body_simple("CHROMA_NONE");
    case TB_KB_STATIC: NEED(p); return body_static(((const tb_one_color *)p)->color);
    case TB_KB_CUSTOM: NEED(p); return body_custom_grid("CHROMA_CUSTOM", &((const tb_kb_custom *)p)->color[0][0], TB_KB_ROWS, TB_KB_COLS);
    case TB_KB_CUSTOM_KEY: {
        NEED(p);
        const tb_kb_custom_key *k = p;
        return body_custom_keys("CHROMA_CUSTOM_KEY", &k->color[0][0], TB_KB_ROWS, TB_KB_COLS, &k->key[0][0]);
    }
    case TB_KB_CUSTOM2: {
        NEED(p);
        const tb_kb_custom2 *k = p;
        return body_custom_keys("CHROMA_CUSTOM2", &k->color[0][0], TB_KB2_ROWS, TB_KB2_COLS, &k->key[0][0]);
    }
    case TB_KB_BREATHING: {
        NEED(p);
        const tb_two_colors *t = p;
        return body_breathing(t->type, t->color1, t->color2);
    }
    case TB_KB_SPECTRUM: return body_simple("CHROMA_SPECTRUMCYCLING");
    case TB_KB_WAVE: return body_wave(p ? *(const int *)p : 1);
    case TB_KB_REACTIVE: return body_simple("CHROMA_NONE"); /* lights keys as they are pressed: dark until then */
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

static char *mouse_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_MOUSE_NONE: return body_simple("CHROMA_NONE");
    case TB_MOUSE_STATIC:
    case TB_MOUSE_BLINKING: {
        NEED(p);
        const tb_mouse_led_color *s = p;
        return mouse_led_body(s->led, s->color);
    }
    case TB_MOUSE_BREATHING: {
        NEED(p);
        const tb_mouse_breathing *s = p;
        return body_breathing(s->type == 3 ? 2 : 1, s->color1, s->type == 1 ? 0 : s->color2);
    }
    case TB_MOUSE_CUSTOM: NEED(p); return body_custom_list(((const tb_mouse_custom *)p)->color, TB_MOUSE_LEDS, 0);
    case TB_MOUSE_CUSTOM2: NEED(p); return body_custom_grid("CHROMA_CUSTOM2", &((const tb_mouse_custom2 *)p)->color[0][0], TB_MOUSE_ROWS, TB_MOUSE_COLS);
    case TB_MOUSE_SPECTRUM: return body_simple("CHROMA_SPECTRUMCYCLING");
    case TB_MOUSE_WAVE: return body_wave(p && *(const int *)p == 1 ? 2 : 1); /* 0 = front→back, 1 = back→front */
    case TB_MOUSE_REACTIVE: return body_simple("CHROMA_NONE");
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

static char *mousepad_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_PAD_NONE: return body_simple("CHROMA_NONE");
    case TB_PAD_STATIC: NEED(p); return body_static(((const tb_one_color *)p)->color);
    case TB_PAD_CUSTOM: NEED(p); return body_custom_list((const tb_color *)p, TB_PAD_LEDS, 0);
    case TB_PAD_CUSTOM2: NEED(p); return body_custom_list((const tb_color *)p, TB_PAD2_LEDS, 0);
    case TB_PAD_BREATHING: {
        NEED(p);
        const tb_two_colors *t = p;
        return body_breathing(t->type, t->color1, t->color2);
    }
    case TB_PAD_SPECTRUM: return body_simple("CHROMA_SPECTRUMCYCLING");
    case TB_PAD_WAVE: return body_wave(p ? *(const int *)p : 1);
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

static char *headset_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_HS_NONE: return body_simple("CHROMA_NONE");
    case TB_HS_STATIC: NEED(p); return body_static(((const tb_one_color *)p)->color);
    case TB_HS_BREATHING: NEED(p); return body_breathing(1, ((const tb_one_color *)p)->color, 0);
    case TB_HS_SPECTRUM: return body_simple("CHROMA_SPECTRUMCYCLING");
    case TB_HS_CUSTOM: NEED(p); return body_custom_list((const tb_color *)p, TB_HEADSET_LEDS, 0);
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

static char *keypad_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_KP_NONE: return body_simple("CHROMA_NONE");
    case TB_KP_STATIC: NEED(p); return body_static(((const tb_one_color *)p)->color);
    case TB_KP_CUSTOM: NEED(p); return body_custom_grid("CHROMA_CUSTOM", (const tb_color *)p, TB_KP_ROWS, TB_KP_COLS);
    case TB_KP_BREATHING: {
        NEED(p);
        const tb_two_colors *t = p;
        return body_breathing(t->type, t->color1, t->color2);
    }
    case TB_KP_SPECTRUM: return body_simple("CHROMA_SPECTRUMCYCLING");
    case TB_KP_WAVE: return body_wave(p ? *(const int *)p : 1);
    case TB_KP_REACTIVE: return body_simple("CHROMA_NONE");
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

static char *link_body(int type, const void *p, tb_result *rc) {
    switch (type) {
    case TB_LINK_NONE: return body_simple("CHROMA_NONE");
    case TB_LINK_STATIC: NEED(p); return body_static(((const tb_one_color *)p)->color);
    case TB_LINK_CUSTOM: NEED(p); return body_custom_list((const tb_color *)p, TB_LINK_LEDS, 0);
    default: *rc = TB_NOT_SUPPORTED; return NULL;
    }
}

/* ---- effect registry + mailbox -------------------------------------------------------------- */

static void wake_worker(void);
static void ensure_worker(void);

/* Queue `body` (owned) as the next frame for `dev`; replaces an unsent older frame. */
static void post_frame(int dev, char *body) {
    char *old = g_pending[dev];
    g_pending[dev] = body;
    free(old);
    wake_worker();
}

static GUID new_id(void) {
    GUID g;
    if (CoCreateGuid(&g) != S_OK) {
        static LONG n;
        memset(&g, 0, sizeof g);
        g.Data1 = (DWORD)InterlockedIncrement(&n);
        g.Data2 = (WORD)GetCurrentProcessId();
        *(DWORD *)g.Data4 = GetTickCount();
    }
    return g;
}

/* Common tail of all Create*Effect calls: apply now (no id wanted) or store under a new id. */
static tb_result finish_create(int dev, char *body, tb_result rc, GUID *out_id) {
    if (!body && dev != DEV_NONE) {
        return rc != TB_OK ? rc : TB_INVALID; /* out of memory or bad parameters */
    }
    EnterCriticalSection(&g_lock);
    if (!out_id) {
        if (dev != DEV_NONE && g_wanted) {
            post_frame(dev, body);
            body = NULL;
        }
        LeaveCriticalSection(&g_lock);
        free(body);
        return TB_OK;
    }
    if (g_effect_count >= EFFECT_LIMIT) {
        /* Keep going with the newest effects: forget one bucket's worth of the oldest-looking ones. */
        unsigned b = bucket(out_id) ^ 1u;
        effect *e = g_effects[b % EFFECT_BUCKETS];
        g_effects[b % EFFECT_BUCKETS] = NULL;
        while (e) {
            effect *n = e->next;
            free(e->body);
            free(e);
            g_effect_count--;
            e = n;
        }
    }
    effect *e = calloc(1, sizeof *e);
    if (!e) {
        LeaveCriticalSection(&g_lock);
        free(body);
        return TB_INVALID;
    }
    e->id = new_id();
    e->dev = dev;
    e->body = body;
    unsigned h = bucket(&e->id);
    e->next = g_effects[h];
    g_effects[h] = e;
    g_effect_count++;
    *out_id = e->id;
    LeaveCriticalSection(&g_lock);
    return TB_OK;
}

static tb_result create_for(int dev, int type, const void *param, GUID *out_id) {
    tb_result rc = TB_OK;
    char *body = NULL;
    switch (dev) {
    case DEV_KEYBOARD: body = keyboard_body(type, param, &rc); break;
    case DEV_MOUSE: body = mouse_body(type, param, &rc); break;
    case DEV_MOUSEPAD: body = mousepad_body(type, param, &rc); break;
    case DEV_HEADSET: body = headset_body(type, param, &rc); break;
    case DEV_KEYPAD: body = keypad_body(type, param, &rc); break;
    case DEV_LINK: body = link_body(type, param, &rc); break;
    default: break;
    }
    if (!body && dev != DEV_NONE) {
        logf_("create %s type %d: rejected (%ld)", DEV_PATH[dev], type, rc);
        return rc != TB_OK ? rc : TB_INVALID;
    }
    return finish_create(dev, body, rc, out_id);
}

/* ---- the exported interface ----------------------------------------------------------------- */

static void utf16_field(char *out, size_t cap, const wchar_t *w, size_t wmax, int narrow) {
    out[0] = 0;
    if (!w) {
        return;
    }
    if (narrow) {
        const char *s = (const char *)w;
        size_t i = 0;
        for (; i + 1 < cap && i < wmax && s[i]; i++) {
            out[i] = s[i];
        }
        out[i] = 0;
        return;
    }
    size_t n = 0;
    while (n < wmax && w[n]) {
        n++;
    }
    int len = WideCharToMultiByte(CP_UTF8, 0, w, (int)n, out, (int)cap - 1, NULL, NULL);
    out[len > 0 ? len : 0] = 0;
}

/* Build the REST init body from the app info (or from the program's file name). */
static char *app_body(const tb_appinfo *info) {
    char title[256 * 3], desc[1024 * 3], name[256 * 3], contact[256 * 3];
    DWORD devices = 0x3F, category = 2;
    if (info) {
        /* Very old builds used 8-bit strings with the same field counts: then the first UTF-16 unit
         * would be two printable bytes. */
        const unsigned char *raw = (const unsigned char *)info->title;
        int narrow = raw[0] >= 0x20 && raw[0] < 0x7f && raw[1] >= 0x20 && raw[1] < 0x7f;
        if (narrow) {
            const char *base = (const char *)info;
            utf16_field(title, 64, (const wchar_t *)base, 256, 1);
            utf16_field(desc, 256, (const wchar_t *)(base + 256), 1024, 1);
            utf16_field(name, 64, (const wchar_t *)(base + 1280), 256, 1);
            utf16_field(contact, 64, (const wchar_t *)(base + 1536), 256, 1);
            devices = *(const DWORD *)(base + 1792);
            category = *(const DWORD *)(base + 1796);
        } else {
            utf16_field(title, 64 * 3, info->title, 64, 0);
            utf16_field(desc, 256 * 3, info->description, 256, 0);
            utf16_field(name, 64 * 3, info->author_name, 64, 0);
            utf16_field(contact, 64 * 3, info->author_contact, 64, 0);
            devices = info->supported_devices;
            category = info->category;
        }
    } else {
        title[0] = desc[0] = name[0] = contact[0] = 0;
    }
    if (!title[0]) {
        /* Init() without app info: name the session after the program. */
        wchar_t path[MAX_PATH];
        DWORD n = GetModuleFileNameW(NULL, path, MAX_PATH);
        const wchar_t *base = path;
        for (DWORD i = 0; i < n; i++) {
            if (path[i] == L'\\' || path[i] == L'/') {
                base = path + i + 1;
            }
        }
        size_t len = wcslen(base);
        if (len > 4 && _wcsicmp(base + len - 4, L".exe") == 0) {
            len -= 4;
        }
        utf16_field(title, sizeof title, base, len < 64 ? len : 64, 0);
        if (!title[0]) {
            strcpy(title, "Game");
        }
    }
    if (!desc[0]) {
        strcpy(desc, "Game lighting via the Tsuba bridge");
    }
    if (!name[0]) {
        strcpy(name, "Unknown");
    }
    if (!contact[0]) {
        strcpy(contact, "-");
    }
    if (!(devices & 0x3F)) {
        devices = 0x3F;
    }
    static const struct { DWORD bit; const char *name; } BITS[] = {
        { TB_SUPPORT_KEYBOARD, "keyboard" }, { TB_SUPPORT_MOUSE, "mouse" }, { TB_SUPPORT_HEADSET, "headset" },
        { TB_SUPPORT_MOUSEPAD, "mousepad" }, { TB_SUPPORT_KEYPAD, "keypad" }, { TB_SUPPORT_CHROMALINK, "chromalink" },
    };
    jbuf b = { 0 };
    jb_s(&b, "{\"title\":");
    jb_str(&b, title);
    jb_s(&b, ",\"description\":");
    jb_str(&b, desc);
    jb_s(&b, ",\"author\":{\"name\":");
    jb_str(&b, name);
    jb_s(&b, ",\"contact\":");
    jb_str(&b, contact);
    jb_s(&b, "},\"device_supported\":[");
    int first = 1;
    for (size_t i = 0; i < sizeof BITS / sizeof BITS[0]; i++) {
        if (devices & BITS[i].bit) {
            jb_s(&b, first ? "\"" : ",\"");
            jb_s(&b, BITS[i].name);
            jb_s(&b, "\"");
            first = 0;
        }
    }
    jb_s(&b, "],\"category\":");
    jb_str(&b, category == 1 ? "application" : "game");
    jb_s(&b, "}");
    return jb_done(&b);
}

static tb_result start(const tb_appinfo *info) {
    char *body = app_body(info);
    EnterCriticalSection(&g_lock);
    free(g_appinfo);
    g_appinfo = body;
    g_wanted = 1;
    ensure_worker();
    LeaveCriticalSection(&g_lock);
    wake_worker();
    logf_("init: %s", body ? body : "(no memory)");
    return TB_OK; /* never fail: a missing server must not break the game */
}

API Init(void) { return start(NULL); }

API InitSDK(tb_appinfo *info) { return start(info); }

API UnInit(void) {
    EnterCriticalSection(&g_lock);
    g_wanted = 0;
    for (int d = 0; d < DEV_COUNT; d++) {
        free(g_pending[d]);
        g_pending[d] = NULL;
    }
    free_all_effects();
    LeaveCriticalSection(&g_lock);
    wake_worker();
    logf_("uninit");
    return TB_OK;
}

API CreateEffect(GUID device, int type, void *param, GUID *out_id) {
    int dev = dev_of_kind(kind_of_id(&device));
    if (dev == DEV_NONE) {
        return finish_create(DEV_NONE, NULL, TB_OK, out_id); /* unknown device: accept, show nothing */
    }
    /* The generic effect numbers, translated to each device's own; parameter blocks are either the
     * generic ones (size-prefixed) or the device's own struct. */
    const DWORD gen_size = (DWORD)sizeof(tb_gen_static);
    switch (type) {
    case TB_GEN_NONE: return create_for(dev, 0, NULL, out_id);
    case TB_GEN_STATIC:
    case TB_GEN_BLINKING: {
        if (!param) {
            return TB_INVALID_PARAMETER;
        }
        tb_color c = *(const DWORD *)param == gen_size ? ((const tb_gen_static *)param)->color : *(const tb_color *)param;
        if (dev == DEV_MOUSE) {
            tb_mouse_led_color m = { TB_MOUSE_LED_ALL, c };
            return create_for(dev, TB_MOUSE_STATIC, &m, out_id);
        }
        static const int STATIC_OF[DEV_COUNT] = { TB_KB_STATIC, TB_MOUSE_STATIC, TB_PAD_STATIC, TB_HS_STATIC, TB_KP_STATIC, TB_LINK_STATIC };
        tb_one_color one = { c };
        return create_for(dev, STATIC_OF[dev], &one, out_id);
    }
    case TB_GEN_BREATHING: {
        if (!param) {
            return TB_INVALID_PARAMETER;
        }
        tb_color c1 = 0xFFFFFF, c2 = 0;
        int t = 1;
        if (*(const DWORD *)param == (DWORD)sizeof(tb_gen_breathing)) {
            const tb_gen_breathing *g = param;
            t = g->type == 3 ? 2 : 1;
            c1 = g->color1;
            c2 = g->type == 1 ? 0 : g->color2;
        }
        return finish_create(dev, body_breathing(t, c1, c2), TB_OK, out_id);
    }
    case TB_GEN_WAVE: return finish_create(dev, body_wave(1), TB_OK, out_id);
    case TB_GEN_SPECTRUM: return finish_create(dev, body_simple("CHROMA_SPECTRUMCYCLING"), TB_OK, out_id);
    case TB_GEN_REACTIVE: return finish_create(dev, body_simple("CHROMA_NONE"), TB_OK, out_id);
    case TB_GEN_CUSTOM: {
        static const int CUSTOM_OF[DEV_COUNT] = { TB_KB_CUSTOM, TB_MOUSE_CUSTOM, TB_PAD_CUSTOM, TB_HS_CUSTOM, TB_KP_CUSTOM, TB_LINK_CUSTOM };
        return create_for(dev, CUSTOM_OF[dev], param, out_id);
    }
    default: return TB_NOT_SUPPORTED;
    }
}

API CreateKeyboardEffect(int type, void *param, GUID *out_id) { return create_for(DEV_KEYBOARD, type, param, out_id); }
API CreateMouseEffect(int type, void *param, GUID *out_id) { return create_for(DEV_MOUSE, type, param, out_id); }
API CreateMousepadEffect(int type, void *param, GUID *out_id) { return create_for(DEV_MOUSEPAD, type, param, out_id); }
API CreateHeadsetEffect(int type, void *param, GUID *out_id) { return create_for(DEV_HEADSET, type, param, out_id); }
API CreateKeypadEffect(int type, void *param, GUID *out_id) { return create_for(DEV_KEYPAD, type, param, out_id); }
API CreateChromaLinkEffect(int type, void *param, GUID *out_id) { return create_for(DEV_LINK, type, param, out_id); }

API SetEffect(GUID id) {
    tb_result rc = TB_OK;
    EnterCriticalSection(&g_lock);
    effect *e = find_effect(&id);
    if (!e) {
        rc = TB_NOT_FOUND;
    } else if (e->dev != DEV_NONE && g_wanted) {
        char *copy = _strdup(e->body);
        if (copy) {
            post_frame(e->dev, copy);
        }
    }
    LeaveCriticalSection(&g_lock);
    return rc;
}

API DeleteEffect(GUID id) {
    tb_result rc = TB_NOT_FOUND;
    EnterCriticalSection(&g_lock);
    effect **pp = &g_effects[bucket(&id)];
    for (; *pp; pp = &(*pp)->next) {
        if (memcmp(&(*pp)->id, &id, sizeof(GUID)) == 0) {
            effect *e = *pp;
            *pp = e->next;
            free(e->body);
            free(e);
            g_effect_count--;
            rc = TB_OK;
            break;
        }
    }
    LeaveCriticalSection(&g_lock);
    return rc;
}

static HWND g_notify_window;

API RegisterEventNotification(HWND window) {
    g_notify_window = window; /* no events are sent yet; kept for future use */
    return TB_OK;
}

API UnregisterEventNotification(void) {
    g_notify_window = NULL;
    return TB_OK;
}

static DWORD connected_count(int kind) {
    int dev = dev_of_kind(kind);
    if (dev == DEV_NONE) {
        return 0;
    }
    return (g_connected_mask >> dev) & 1 ? 1 : 0;
}

API QueryDevice(GUID device, tb_devinfo *info) {
    if (!info) {
        return TB_INVALID_PARAMETER;
    }
    int kind = kind_of_id(&device);
    if (kind == TB_KIND_INVALID) {
        info->kind = TB_KIND_INVALID;
        info->connected = 0;
        return TB_DEVICE_NOT_AVAILABLE;
    }
    info->kind = kind;
    info->connected = connected_count(kind);
    return TB_OK;
}

API IsConnected(tb_devinfo *info) {
    if (!info) {
        return TB_INVALID_PARAMETER;
    }
    if (info->kind == TB_KIND_ALL) {
        DWORD n = 0;
        for (int d = 0; d < DEV_COUNT; d++) {
            n += (g_connected_mask >> d) & 1;
        }
        info->connected = n;
    } else {
        info->connected = connected_count(info->kind);
    }
    return TB_OK;
}

API IsActive(BOOL *active) {
    if (!active) {
        return TB_INVALID_PARAMETER;
    }
    *active = TRUE;
    return TB_OK;
}

API SetEventName(const wchar_t *name) {
    (void)name; /* named in-game events: nothing to do */
    return TB_OK;
}

API UseIdleAnimation(BOOL use) {
    (void)use;
    return TB_OK;
}

API UseIdleAnimations(BOOL use) {
    (void)use;
    return TB_OK;
}

API UseForwardChromaEvents(BOOL use) {
    (void)use;
    return TB_OK;
}

/* ---- HTTP client (127.0.0.1 only) ----------------------------------------------------------- */

/* The worker's connection. Each worker owns its own (a worker that is exiting can never close
 * the socket of the next one). */
typedef struct {
    SOCKET s;
    int port;
} conn_t;

static void sock_close(conn_t *c) {
    if (c->s != INVALID_SOCKET) {
        closesocket(c->s);
        c->s = INVALID_SOCKET;
    }
}

static int sock_open(conn_t *c, int port) {
    sock_close(c);
    SOCKET s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (s == INVALID_SOCKET) {
        return -1;
    }
    DWORD tmo = 2000;
    setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, (const char *)&tmo, sizeof tmo);
    setsockopt(s, SOL_SOCKET, SO_SNDTIMEO, (const char *)&tmo, sizeof tmo);
    BOOL one = TRUE;
    setsockopt(s, IPPROTO_TCP, TCP_NODELAY, (const char *)&one, sizeof one);
    struct sockaddr_in a;
    memset(&a, 0, sizeof a);
    a.sin_family = AF_INET;
    a.sin_port = htons((u_short)port);
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (connect(s, (struct sockaddr *)&a, sizeof a) != 0) {
        closesocket(s);
        return -1;
    }
    c->s = s;
    c->port = port;
    return 0;
}

static int send_all(conn_t *c, const char *p, size_t n) {
    while (n) {
        int k = send(c->s, p, (int)(n > 65536 ? 65536 : n), 0);
        if (k <= 0) {
            return -1;
        }
        p += k;
        n -= (size_t)k;
    }
    return 0;
}

static const char *find_header(const char *head, const char *name) {
    size_t n = strlen(name);
    for (const char *p = strstr(head, "\r\n"); p && p[2]; p = strstr(p + 2, "\r\n")) {
        if (_strnicmp(p + 2, name, n) == 0 && p[2 + n] == ':') {
            const char *v = p + 3 + n;
            while (*v == ' ' || *v == '\t') {
                v++;
            }
            return v;
        }
    }
    return NULL;
}

/* Grow buf so that at least `room` more bytes plus a NUL fit after n; fails above `limit`. */
static int grow(char **buf, size_t *cap, size_t n, size_t room, size_t limit) {
    if (n + room + 1 <= *cap) {
        return 0;
    }
    size_t want = *cap;
    while (n + room + 1 > want) {
        if (want > limit) {
            return -1;
        }
        want *= 2;
    }
    char *nb = realloc(*buf, want);
    if (!nb) {
        return -1;
    }
    *buf = nb;
    *cap = want;
    return 0;
}

/* One request on the cached connection. Returns the HTTP status (body in *out, owned) or -1 when
 * the transport failed or the reply is not one tsubad sends (tsubad always answers with a
 * Content-Length, never chunked: a chunked reply is refused rather than decoded). The buffer
 * always keeps room for a terminating NUL. */
static int http_once(conn_t *c, int port, const char *method, const char *path, const char *body, char **out) {
    if (c->s == INVALID_SOCKET || c->port != port) {
        if (sock_open(c, port) != 0) {
            return -1;
        }
    }
    size_t blen = body ? strlen(body) : 0;
    char head[512];
    int hn = snprintf(head, sizeof head,
                      "%s %s HTTP/1.1\r\nHost: localhost:%d\r\nUser-Agent: tsuba-game-lighting\r\n"
                      "Content-Type: application/json\r\nContent-Length: %u\r\nConnection: keep-alive\r\n\r\n",
                      method, path, port, (unsigned)blen);
    if (hn <= 0 || hn >= (int)sizeof head || send_all(c, head, (size_t)hn) || (blen && send_all(c, body, blen))) {
        sock_close(c);
        return -1;
    }
    /* Read the head. */
    size_t cap = 4096, n = 0;
    char *buf = malloc(cap);
    if (!buf) {
        sock_close(c);
        return -1;
    }
    buf[0] = 0;
    char *end = NULL;
    for (;;) {
        if (grow(&buf, &cap, n, 1024, 1 << 20)) {
            goto fail;
        }
        int k = recv(c->s, buf + n, (int)(cap - n - 1), 0);
        if (k <= 0) {
            goto fail;
        }
        n += (size_t)k;
        buf[n] = 0;
        if ((end = strstr(buf, "\r\n\r\n")) != NULL) {
            break;
        }
    }
    int status = 0;
    if (sscanf(buf, "HTTP/%*d.%*d %d", &status) != 1) {
        goto fail;
    }
    *end = 0;
    const char *cl = find_header(buf, "Content-Length");
    const char *te = find_header(buf, "Transfer-Encoding");
    const char *conn = find_header(buf, "Connection");
    int close_after = conn && _strnicmp(conn, "close", 5) == 0;
    if (te) {
        /* Not something tsubad sends: treat the request as failed. */
        goto fail;
    }
    long want = -1;
    if (cl) {
        char *e = NULL;
        want = strtol(cl, &e, 10);
        if (e == cl || want < 0) {
            goto fail;
        }
    }
    size_t body_at = (size_t)(end + 4 - buf);
    char *rb = NULL;
    size_t rn = 0;
    if (want >= 0) {
        if (want > (4 << 20)) {
            goto fail;
        }
        size_t total = body_at + (size_t)want;
        if (total + 1 > cap) {
            char *nb = realloc(buf, total + 1);
            if (!nb) {
                goto fail;
            }
            buf = nb;
            cap = total + 1;
        }
        while (n < total) {
            int k = recv(c->s, buf + n, (int)(total - n), 0);
            if (k <= 0) {
                goto fail;
            }
            n += (size_t)k;
        }
        rb = malloc((size_t)want + 1);
        if (!rb) {
            goto fail;
        }
        memcpy(rb, buf + body_at, (size_t)want);
        rn = (size_t)want;
        rb[rn] = 0;
    } else {
        /* No length: the body runs until the server closes. */
        for (;;) {
            if (grow(&buf, &cap, n, 1024, 1 << 22)) {
                goto fail;
            }
            int k = recv(c->s, buf + n, (int)(cap - n - 1), 0);
            if (k <= 0) {
                break;
            }
            n += (size_t)k;
            buf[n] = 0;
        }
        buf[n] = 0;
        rb = _strdup(buf + body_at);
        close_after = 1;
    }
    free(buf);
    if (close_after) {
        sock_close(c);
    }
    if (out) {
        *out = rb;
    } else {
        free(rb);
    }
    return status;
fail:
    free(buf);
    sock_close(c);
    return -1;
}

static int http(conn_t *c, int port, const char *method, const char *path, const char *body, char **out) {
    if (out) {
        *out = NULL;
    }
    int reused = c->s != INVALID_SOCKET && c->port == port;
    int st = http_once(c, port, method, path, body, out);
    if (st < 0 && reused) {
        st = http_once(c, port, method, path, body, out); /* the server closed an idle keep-alive connection */
    }
    return st;
}

/* "key":"value" → value (NUL-terminated, at most cap-1 bytes) */
static int json_string(const char *json, const char *key, char *out, size_t cap) {
    char pat[64];
    snprintf(pat, sizeof pat, "\"%s\"", key);
    const char *p = json ? strstr(json, pat) : NULL;
    if (!p) {
        return -1;
    }
    p += strlen(pat);
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n' || *p == ':') {
        p++;
    }
    if (*p != '"') {
        return -1;
    }
    p++;
    size_t i = 0;
    while (*p && *p != '"' && i + 1 < cap) {
        out[i++] = *p++;
    }
    out[i] = 0;
    return *p == '"' ? 0 : -1;
}

/* Optional "devices": ["keyboard", ...] in a server reply: which kinds are attached. */
static void read_devices(const char *json) {
    const char *p = json ? strstr(json, "\"devices\"") : NULL;
    if (!p) {
        return;
    }
    p = strchr(p, '[');
    const char *e = p ? strchr(p, ']') : NULL;
    if (!p || !e) {
        return;
    }
    LONG mask = 0;
    for (int d = 0; d < DEV_COUNT; d++) {
        char pat[32];
        snprintf(pat, sizeof pat, "\"%s\"", DEV_PATH[d]);
        const char *f = strstr(p, pat);
        if (f && f < e) {
            mask |= 1 << d;
        }
    }
    InterlockedExchange(&g_connected_mask, mask);
}

/* ---- worker --------------------------------------------------------------------------------- */

static int base_port(void) {
    char v[16];
    DWORD n = GetEnvironmentVariableA("TSUBA_GAME_LIGHTING_PORT", v, sizeof v);
    if (n > 0 && n < sizeof v) {
        long p = strtol(v, NULL, 10);
        if (p > 0 && p < 65536) {
            return (int)p;
        }
    }
    return 54235;
}

static void wake_worker(void) {
    if (g_wake) {
        SetEvent(g_wake);
    }
}

static DWORD WINAPI worker(LPVOID arg);

/* Called with g_lock held. */
static void ensure_worker(void) {
    if (g_worker_running) {
        return;
    }
    HMODULE ref = NULL;
    /* Our own reference on the DLL: the thread releases it with FreeLibraryAndExitThread. */
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, (LPCWSTR)(void *)&worker, &ref)) {
        return;
    }
    HANDLE t = CreateThread(NULL, 0, worker, ref, 0, NULL);
    if (!t) {
        FreeLibrary(ref);
        return;
    }
    CloseHandle(t);
    g_worker_running = 1;
}

static DWORD WINAPI worker(LPVOID arg) {
    HMODULE ref = (HMODULE)arg;
    WSADATA wsa;
    int wsa_ok = WSAStartup(MAKEWORD(2, 2), &wsa) == 0;
    const int port = base_port();
    conn_t conn = { INVALID_SOCKET, 0 };
    int session = 0, sport = 0;
    char spath[256] = { 0 };
    char *last[DEV_COUNT] = { 0 }; /* last frame the server accepted, re-sent after a reconnect */
    DWORD next_try = 0, backoff = 500, last_io = 0, idle_since = GetTickCount();
    for (;;) {
        DWORD now = GetTickCount();
        char *frames[DEV_COUNT] = { 0 };
        char *app = NULL;
        EnterCriticalSection(&g_lock);
        int wanted = g_wanted;
        if (wanted) {
            for (int d = 0; d < DEV_COUNT; d++) {
                frames[d] = g_pending[d];
                g_pending[d] = NULL;
            }
            if (!session && g_appinfo) {
                app = _strdup(g_appinfo);
            }
        } else if (!session && now - idle_since > 3000) {
            /* Nothing to do for a while after UnInit: leave (Init starts a new worker). */
            g_worker_running = 0;
            LeaveCriticalSection(&g_lock);
            break;
        }
        LeaveCriticalSection(&g_lock);

        if (!wanted) {
            if (session) {
                char path[300];
                snprintf(path, sizeof path, "%s", spath);
                int st = wsa_ok ? http(&conn, sport, "DELETE", path, NULL, NULL) : -1;
                logf_("session closed (%d)", st);
                session = 0;
                idle_since = now;
                sock_close(&conn);
            }
            for (int d = 0; d < DEV_COUNT; d++) {
                free(last[d]);
                last[d] = NULL;
            }
            WaitForSingleObject(g_wake, 1000);
            continue;
        }
        idle_since = now;

        if (!session && app && wsa_ok && (LONG)(now - next_try) >= 0) {
            char *reply = NULL;
            int st = http(&conn, port, "POST", "/razer/chromasdk", app, &reply);
            char uri[256];
            if (st == 200 && json_string(reply, "uri", uri, sizeof uri) == 0) {
                /* uri = http://localhost:<port>/<path>; only the port and path are used (always 127.0.0.1). */
                const char *h = strstr(uri, "://");
                h = h ? h + 3 : uri;
                const char *colon = strchr(h, ':');
                const char *slash = strchr(h, '/');
                int p = colon && (!slash || colon < slash) ? atoi(colon + 1) : port;
                if (p > 0 && p < 65536 && slash) {
                    sport = p;
                    snprintf(spath, sizeof spath, "%s", slash);
                    size_t l = strlen(spath);
                    while (l > 1 && spath[l - 1] == '/') {
                        spath[--l] = 0;
                    }
                    session = 1;
                    backoff = 500;
                    read_devices(reply);
                    logf_("session %s on port %d", spath, sport);
                    /* Restore what the game showed before the server went away. */
                    for (int d = 0; d < DEV_COUNT; d++) {
                        if (!frames[d] && last[d]) {
                            frames[d] = last[d];
                            last[d] = NULL;
                        }
                    }
                }
            }
            if (!session) {
                logf_("server not reachable on port %d (%d); retry in %lu ms", port, st, (unsigned long)backoff);
                next_try = now + backoff;
                backoff = backoff * 2 > 15000 ? 15000 : backoff * 2;
                sock_close(&conn);
            }
            free(reply);
        }
        free(app);

        if (session) {
            char path[300];
            int lost = 0;
            for (int d = 0; d < DEV_COUNT && !lost; d++) {
                if (!frames[d]) {
                    continue;
                }
                snprintf(path, sizeof path, "%s/%s", spath, DEV_PATH[d]);
                int st = http(&conn, sport, "PUT", path, frames[d], NULL);
                if (st < 0 || st == 404) {
                    lost = 1; /* keep the frame: it goes out again on the next session */
                    break;
                }
                free(last[d]);
                last[d] = frames[d];
                frames[d] = NULL;
                last_io = GetTickCount();
            }
            if (!lost && GetTickCount() - last_io >= 1000) {
                snprintf(path, sizeof path, "%s/heartbeat", spath);
                int st = http(&conn, sport, "PUT", path, NULL, NULL);
                lost = st < 0 || st == 404;
                last_io = GetTickCount();
            }
            if (lost) {
                logf_("session lost; reconnecting");
                session = 0;
                sock_close(&conn);
                next_try = GetTickCount() + backoff;
            }
        }
        /* Frames that couldn't be sent wait for the next session unless a newer one arrived. */
        EnterCriticalSection(&g_lock);
        for (int d = 0; d < DEV_COUNT; d++) {
            if (frames[d]) {
                if (!g_pending[d] && g_wanted) {
                    g_pending[d] = frames[d];
                } else {
                    free(frames[d]);
                }
            }
        }
        int more = 0;
        for (int d = 0; d < DEV_COUNT; d++) {
            more |= g_pending[d] != NULL;
        }
        LeaveCriticalSection(&g_lock);
        DWORD wait = 1000;
        if (session) {
            DWORD since = GetTickCount() - last_io;
            wait = since >= 1000 ? 0 : 1000 - since;
            if (more) {
                wait = 0;
            }
        } else if ((LONG)(next_try - GetTickCount()) > 0) {
            wait = next_try - GetTickCount();
            if (wait > 1000) {
                wait = 1000;
            }
        } else if (more) {
            wait = 0;
        }
        if (wait) {
            WaitForSingleObject(g_wake, wait);
        }
    }
    for (int d = 0; d < DEV_COUNT; d++) {
        free(last[d]);
    }
    sock_close(&conn);
    if (wsa_ok) {
        WSACleanup();
    }
    logf_("worker exit");
    FreeLibraryAndExitThread(ref, 0);
    return 0;
}

/* ---- DLL entry ------------------------------------------------------------------------------ */

BOOL WINAPI DllMain(HINSTANCE inst, DWORD reason, LPVOID reserved) {
    switch (reason) {
    case DLL_PROCESS_ATTACH: {
        DisableThreadLibraryCalls(inst);
        InitializeCriticalSection(&g_lock);
        g_wake = CreateEventW(NULL, FALSE, FALSE, NULL);
        char path[MAX_PATH];
        DWORD n = GetEnvironmentVariableA("TSUBA_GAME_LIGHTING_LOG", path, sizeof path);
        if (n > 0 && n < sizeof path) {
            g_log = fopen(path, "a");
            logf_("loaded (%d-bit)", (int)sizeof(void *) * 8);
        }
        break;
    }
    case DLL_PROCESS_DETACH:
        if (reserved) {
            break; /* process exit: the OS reclaims everything; threads are already gone */
        }
        /* FreeLibrary: the worker (if any) holds a reference, so it has already exited. */
        free_all_effects();
        for (int d = 0; d < DEV_COUNT; d++) {
            free(g_pending[d]);
            g_pending[d] = NULL;
        }
        free(g_appinfo);
        g_appinfo = NULL;
        if (g_wake) {
            CloseHandle(g_wake);
        }
        if (g_log) {
            fclose(g_log);
        }
        DeleteCriticalSection(&g_lock);
        break;
    }
    return TRUE;
}
