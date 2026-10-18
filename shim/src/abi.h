/* SPDX-License-Identifier: GPL-2.0-or-later
 *
 * Tsuba game lighting bridge: binary interface of the native lighting SDK that games call.
 *
 * Written for Tsuba from the publicly documented interface (function names, argument order, effect
 * numbers, struct layouts and device ids). Only what is needed for binary compatibility is
 * described here; the names are Tsuba's own.
 */
#ifndef TSUBA_BRIDGE_ABI_H
#define TSUBA_BRIDGE_ABI_H

#include <windows.h>
#include <stddef.h>

typedef LONG tb_result;
typedef DWORD tb_color; /* 0x00BBGGRR; bit 24 marks a per-key colour in "key" grids */

/* Result codes (Windows error numbers, as the interface uses them). */
#define TB_OK 0L
#define TB_INVALID -1L
#define TB_NOT_SUPPORTED 50L
#define TB_INVALID_PARAMETER 87L
#define TB_NOT_FOUND 1168L
#define TB_DEVICE_NOT_AVAILABLE 4319L

/* Application info passed to InitSDK (UTF-16 strings). */
typedef struct tb_appinfo {
    wchar_t title[256];
    wchar_t description[1024];
    wchar_t author_name[256];
    wchar_t author_contact[256];
    DWORD supported_devices; /* bit mask, see TB_SUPPORT_* */
    DWORD category;          /* 1 = application, 2 = game */
} tb_appinfo;

#define TB_SUPPORT_KEYBOARD 0x01
#define TB_SUPPORT_MOUSE 0x02
#define TB_SUPPORT_HEADSET 0x04
#define TB_SUPPORT_MOUSEPAD 0x08
#define TB_SUPPORT_KEYPAD 0x10
#define TB_SUPPORT_CHROMALINK 0x20

/* Device kind numbers used by QueryDevice / IsConnected. */
enum {
    TB_KIND_KEYBOARD = 1,
    TB_KIND_MOUSE = 2,
    TB_KIND_HEADSET = 3,
    TB_KIND_MOUSEPAD = 4,
    TB_KIND_KEYPAD = 5,
    TB_KIND_SYSTEM = 6,
    TB_KIND_SPEAKERS = 7,
    TB_KIND_CHROMALINK = 8,
    TB_KIND_ALL = 255,
    TB_KIND_INVALID = 256
};

typedef struct tb_devinfo {
    int kind;        /* enum above (a C++ enum: int sized) */
    DWORD connected; /* number of connected devices of that kind */
} tb_devinfo;

/* ---- Generic (device id based) effects: CreateEffect ------------------------------------------ */
enum {
    TB_GEN_NONE = 0,
    TB_GEN_WAVE,
    TB_GEN_SPECTRUM,
    TB_GEN_BREATHING,
    TB_GEN_BLINKING,
    TB_GEN_REACTIVE,
    TB_GEN_STATIC,
    TB_GEN_CUSTOM,
    TB_GEN_RESERVED,
    TB_GEN_INVALID
};
/* Generic parameter blocks start with a size and a reserved word. */
typedef struct tb_gen_header {
    size_t size;
    DWORD param;
} tb_gen_header;
typedef struct tb_gen_static {
    size_t size;
    DWORD param;
    tb_color color;
} tb_gen_static;
typedef struct tb_gen_breathing {
    size_t size;
    DWORD param;
    int type;
    tb_color color1;
    tb_color color2;
} tb_gen_breathing;

/* ---- Keyboard ------------------------------------------------------------------------------- */
#define TB_KB_ROWS 6
#define TB_KB_COLS 22
#define TB_KB2_ROWS 8
#define TB_KB2_COLS 24
enum {
    TB_KB_NONE = 0,
    TB_KB_BREATHING,
    TB_KB_CUSTOM,
    TB_KB_REACTIVE,
    TB_KB_STATIC,
    TB_KB_SPECTRUM,
    TB_KB_WAVE,
    TB_KB_RESERVED,
    TB_KB_CUSTOM_KEY,
    TB_KB_CUSTOM2,
    TB_KB_INVALID
};
typedef struct tb_kb_custom { tb_color color[TB_KB_ROWS][TB_KB_COLS]; } tb_kb_custom;
typedef struct tb_kb_custom_key {
    tb_color color[TB_KB_ROWS][TB_KB_COLS];
    tb_color key[TB_KB_ROWS][TB_KB_COLS];
} tb_kb_custom_key;
typedef struct tb_kb_custom2 {
    tb_color color[TB_KB2_ROWS][TB_KB2_COLS];
    tb_color key[TB_KB_ROWS][TB_KB_COLS];
} tb_kb_custom2;
typedef struct tb_one_color { tb_color color; } tb_one_color;
typedef struct tb_two_colors { int type; tb_color color1; tb_color color2; } tb_two_colors;
typedef struct tb_reactive { int duration; tb_color color; } tb_reactive;

/* ---- Mouse ---------------------------------------------------------------------------------- */
#define TB_MOUSE_LEDS 30
#define TB_MOUSE_ROWS 9
#define TB_MOUSE_COLS 7
#define TB_MOUSE_LED_ALL 0xFFFF
enum {
    TB_MOUSE_NONE = 0,
    TB_MOUSE_BLINKING,
    TB_MOUSE_BREATHING,
    TB_MOUSE_CUSTOM,
    TB_MOUSE_REACTIVE,
    TB_MOUSE_SPECTRUM,
    TB_MOUSE_STATIC,
    TB_MOUSE_WAVE,
    TB_MOUSE_CUSTOM2,
    TB_MOUSE_INVALID
};
typedef struct tb_mouse_custom { tb_color color[TB_MOUSE_LEDS]; } tb_mouse_custom;
typedef struct tb_mouse_custom2 { tb_color color[TB_MOUSE_ROWS][TB_MOUSE_COLS]; } tb_mouse_custom2;
typedef struct tb_mouse_led_color { int led; tb_color color; } tb_mouse_led_color;          /* static, blinking */
typedef struct tb_mouse_breathing { int led; int type; tb_color color1; tb_color color2; } tb_mouse_breathing;

/* ---- Mousepad ------------------------------------------------------------------------------- */
#define TB_PAD_LEDS 15
#define TB_PAD2_LEDS 20
enum {
    TB_PAD_NONE = 0,
    TB_PAD_BREATHING,
    TB_PAD_CUSTOM,
    TB_PAD_SPECTRUM,
    TB_PAD_STATIC,
    TB_PAD_WAVE,
    TB_PAD_CUSTOM2,
    TB_PAD_INVALID
};

/* ---- Headset -------------------------------------------------------------------------------- */
#define TB_HEADSET_LEDS 5
enum { TB_HS_NONE = 0, TB_HS_STATIC, TB_HS_BREATHING, TB_HS_SPECTRUM, TB_HS_CUSTOM, TB_HS_INVALID };

/* ---- Keypad --------------------------------------------------------------------------------- */
#define TB_KP_ROWS 4
#define TB_KP_COLS 5
enum {
    TB_KP_NONE = 0,
    TB_KP_BREATHING,
    TB_KP_CUSTOM,
    TB_KP_REACTIVE,
    TB_KP_SPECTRUM,
    TB_KP_STATIC,
    TB_KP_WAVE,
    TB_KP_INVALID
};

/* ---- Chroma Link (addressable accessories) -------------------------------------------------- */
#define TB_LINK_LEDS 5
enum { TB_LINK_NONE = 0, TB_LINK_CUSTOM, TB_LINK_STATIC, TB_LINK_INVALID };

#endif
