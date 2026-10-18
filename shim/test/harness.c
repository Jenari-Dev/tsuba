/* SPDX-License-Identifier: GPL-2.0-or-later
 *
 * Test harness for the Tsuba game lighting bridge: behaves like a game. Loads the DLL by path,
 * calls the SDK entry points and checks that none of them blocks.
 *
 *   harness.exe <dll> full     Init(SDK) → effects → frames → heartbeat wait → UnInit → FreeLibrary
 *   harness.exe <dll> offline  same calls against a closed port: every call must still succeed fast
 *   harness.exe <dll> full 15  as full, but keeps the session for 15 s (screenshots)
 */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "abi.h"

typedef tb_result(__cdecl *fn_void)(void);
typedef tb_result(__cdecl *fn_initsdk)(tb_appinfo *);
typedef tb_result(__cdecl *fn_create)(int, void *, GUID *);
typedef tb_result(__cdecl *fn_create_generic)(GUID, int, void *, GUID *);
typedef tb_result(__cdecl *fn_guid)(GUID);
typedef tb_result(__cdecl *fn_query)(GUID, tb_devinfo *);

static int failures;
#define CHECK(cond, ...) \
    do { \
        if (!(cond)) { \
            printf("FAIL: " __VA_ARGS__); \
            printf("\n"); \
            failures++; \
        } \
    } while (0)

static double ms_since(LARGE_INTEGER t0) {
    LARGE_INTEGER f, t;
    QueryPerformanceFrequency(&f);
    QueryPerformanceCounter(&t);
    return (double)(t.QuadPart - t0.QuadPart) * 1000.0 / (double)f.QuadPart;
}

static FARPROC need(HMODULE m, const char *name) {
    FARPROC p = GetProcAddress(m, name);
    if (!p) {
        printf("FAIL: export %s missing\n", name);
        failures++;
    }
    return p;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        printf("usage: harness <dll> full|offline\n");
        return 2;
    }
    int offline = strcmp(argv[2], "offline") == 0;
    HMODULE m = LoadLibraryA(argv[1]);
    if (!m) {
        printf("FAIL: LoadLibrary(%s) = %lu\n", argv[1], GetLastError());
        return 1;
    }
    fn_initsdk init_sdk = (fn_initsdk)need(m, "InitSDK");
    fn_void uninit = (fn_void)need(m, "UnInit");
    fn_create kb = (fn_create)need(m, "CreateKeyboardEffect");
    fn_create mouse = (fn_create)need(m, "CreateMouseEffect");
    fn_create pad = (fn_create)need(m, "CreateMousepadEffect");
    fn_create headset = (fn_create)need(m, "CreateHeadsetEffect");
    fn_create keypad = (fn_create)need(m, "CreateKeypadEffect");
    fn_create link = (fn_create)need(m, "CreateChromaLinkEffect");
    fn_create_generic generic = (fn_create_generic)need(m, "CreateEffect");
    fn_guid set = (fn_guid)need(m, "SetEffect");
    fn_guid del = (fn_guid)need(m, "DeleteEffect");
    fn_query query = (fn_query)need(m, "QueryDevice");
    const char *others[] = { "Init", "RegisterEventNotification", "UnregisterEventNotification", "IsActive", "IsConnected", "SetEventName" };
    for (size_t i = 0; i < sizeof others / sizeof others[0]; i++) {
        need(m, others[i]);
    }
    if (failures) {
        return 1;
    }

    LARGE_INTEGER t0;
    double worst = 0, t;
    static const char *worst_what = "-";
#define TIMED(expr) (QueryPerformanceCounter(&t0), (expr), t = ms_since(t0), (t > worst ? (worst_what = #expr) : 0), worst = t > worst ? t : worst)

    static tb_appinfo info;
    wcscpy(info.title, L"Harness Game — 剣");
    wcscpy(info.description, L"Tsuba bridge test");
    wcscpy(info.author_name, L"JenAriDev");
    wcscpy(info.author_contact, L"https://example.invalid");
    info.supported_devices = TB_SUPPORT_KEYBOARD | TB_SUPPORT_MOUSE;
    info.category = 2;
    tb_result rc;
    TIMED(rc = init_sdk(&info));
    CHECK(rc == TB_OK, "InitSDK = %ld", rc);

    /* Keyboard custom frame stored under an id, applied with SetEffect. */
    static tb_kb_custom frame;
    for (int r = 0; r < TB_KB_ROWS; r++) {
        for (int c = 0; c < TB_KB_COLS; c++) {
            frame.color[r][c] = (tb_color)(r * 0x10 + c); /* distinctive values */
        }
    }
    GUID id;
    TIMED(rc = kb(TB_KB_CUSTOM, &frame, &id));
    CHECK(rc == TB_OK, "CreateKeyboardEffect(CUSTOM) = %ld", rc);
    TIMED(rc = set(id));
    CHECK(rc == TB_OK, "SetEffect = %ld", rc);

    /* Mouse static, applied at once (no id). */
    tb_mouse_led_color ms = { TB_MOUSE_LED_ALL, 0x0000FF00 };
    TIMED(rc = mouse(TB_MOUSE_STATIC, &ms, NULL));
    CHECK(rc == TB_OK, "CreateMouseEffect(STATIC) = %ld", rc);

    /* The other devices, each once. */
    tb_color five[TB_PAD2_LEDS] = { 0x0000FF, 0x00FF00, 0xFF0000, 0xFFFFFF, 0x123456 };
    TIMED(rc = pad(TB_PAD_CUSTOM, five, NULL));
    CHECK(rc == TB_OK, "CreateMousepadEffect = %ld", rc);
    TIMED(rc = headset(TB_HS_CUSTOM, five, NULL));
    CHECK(rc == TB_OK, "CreateHeadsetEffect = %ld", rc);
    static tb_color kp[TB_KP_ROWS][TB_KP_COLS];
    kp[3][4] = 0xABCDEF;
    TIMED(rc = keypad(TB_KP_CUSTOM, kp, NULL));
    CHECK(rc == TB_OK, "CreateKeypadEffect = %ld", rc);
    TIMED(rc = link(TB_LINK_CUSTOM, five, NULL));
    CHECK(rc == TB_OK, "CreateChromaLinkEffect = %ld", rc);

    /* Generic effect on a published keyboard id → keyboard static. */
    const GUID kb_id = { 0x2ea1bb63, 0xca28, 0x428d, { 0x9f, 0x06, 0x19, 0x6b, 0x88, 0x33, 0x0b, 0xbb } };
    tb_gen_static gs = { sizeof(tb_gen_static), 0, 0x00FF00FF };
    GUID gid;
    TIMED(rc = generic(kb_id, TB_GEN_STATIC, &gs, &gid));
    CHECK(rc == TB_OK, "CreateEffect(generic static) = %ld", rc);
    tb_devinfo di = { 0, 0 };
    TIMED(rc = query(kb_id, &di));
    CHECK(rc == TB_OK && di.kind == TB_KIND_KEYBOARD && di.connected == 1, "QueryDevice = %ld kind %d connected %lu", rc, di.kind, di.connected);
    CHECK(del(gid) == TB_OK, "DeleteEffect(generic)");
    CHECK(set(gid) == TB_NOT_FOUND, "SetEffect after delete should be NOT_FOUND");

    Sleep(400); /* let the first frames go out */

    /* An animation burst: 200 frames in a tight loop. Only the newest should matter. */
    static tb_kb_custom_key ck;
    GUID last = { 0 };
    for (int i = 0; i < 200; i++) {
        ck.color[0][0] = (tb_color)i;
        ck.key[2][3] = 0x01000000u | 0xFF;
        GUID f;
        TIMED(rc = kb(TB_KB_CUSTOM_KEY, &ck, &f));
        if (rc != TB_OK) {
            CHECK(0, "burst create %d = %ld", i, rc);
            break;
        }
        TIMED(set(f));
        if (i) {
            TIMED(del(last));
        }
        last = f;
    }
    /* Keyboard CUSTOM2 (extended 8×24 grid) applied directly as the very last keyboard frame. */
    static tb_kb_custom2 ext;
    ext.color[7][23] = 0x00654321;
    TIMED(rc = kb(TB_KB_CUSTOM2, &ext, NULL));
    CHECK(rc == TB_OK, "CreateKeyboardEffect(CUSTOM2) = %ld", rc);

    /* Heartbeats (≥ 2 expected); an optional third argument holds the session longer (seconds). */
    Sleep(argc > 3 ? (DWORD)atoi(argv[3]) * 1000 : (offline ? 500 : 2600));
    TIMED(rc = uninit());
    CHECK(rc == TB_OK, "UnInit = %ld", rc);
    printf("slowest call: %.3f ms (%s)\n", worst, worst_what);
    /* A call that waited on the network would take hundreds of ms (connect/read timeouts); Wine and
     * the OS scheduler occasionally pause a thread for 10-30 ms, so allow that much jitter. */
    CHECK(worst < 100.0, "a call blocked for %.3f ms (%s)", worst, worst_what);
    Sleep(offline ? 200 : 800); /* the worker deletes the session */
    FreeLibrary(m);
    Sleep(offline ? 200 : 3600); /* the worker leaves after 3 s idle and drops its reference */
    printf(failures ? "harness: %d failure(s)\n" : "harness: ok\n", failures);
    return failures ? 1 : 0;
}
