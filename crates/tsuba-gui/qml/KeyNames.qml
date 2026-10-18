pragma Singleton

import QtQuick

QtObject {
    // HID keyboard-page usage -> key legend. Key-cap labels (A, F1, Esc, Ctrl...) are the same in every
    // language; descriptive names (Space, Left, Num Lock...) are translated.
    readonly property var hid: buildHid()
    // i18n-ok-begin (key caps)
    function buildHid() {
        const t = {};
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").forEach((c, i) => { t[0x04 + i] = c; });
        "1234567890".split("").forEach((c, i) => { t[0x1e + i] = c; });
        for (let i = 0; i < 12; i++) { t[0x3a + i] = "F" + (i + 1); }
        for (let i = 0; i < 12; i++) { t[0x68 + i] = "F" + (i + 13); }
        Object.assign(t, {
            0x28: "Enter", 0x29: "Esc", 0x2a: "Backspace", 0x2b: "Tab", 0x2c: i18nc("key name", "Space"), 0x2d: "-", 0x2e: "=",
            0x2f: "[", 0x30: "]", 0x31: "\\", 0x32: "#", 0x33: ";", 0x34: "'", 0x35: "`", 0x36: ",", 0x37: ".", 0x38: "/",
            0x39: "Caps Lock", 0x46: "Print Screen", 0x47: "Scroll Lock", 0x48: "Pause", 0x49: "Insert", 0x4a: "Home",
            0x4b: "Page Up", 0x4c: "Delete", 0x4d: "End", 0x4e: "Page Down",
            0x4f: i18nc("arrow key", "Right"), 0x50: i18nc("arrow key", "Left"), 0x51: i18nc("arrow key", "Down"), 0x52: i18nc("arrow key", "Up"),
            0x53: "Num Lock", 0x54: i18nc("numpad key", "Num /"), 0x55: i18nc("numpad key", "Num *"), 0x56: i18nc("numpad key", "Num -"),
            0x57: i18nc("numpad key", "Num +"), 0x58: i18nc("numpad key", "Num Enter"),
            0x63: i18nc("numpad key", "Num ."), 0x64: "\\|", 0x65: "Menu",
            0xe0: "Ctrl", 0xe1: "Shift", 0xe2: "Alt", 0xe3: "Win", 0xe4: "Ctrl", 0xe5: "Shift", 0xe6: "Alt", 0xe7: "Win"
        });
        for (let i = 0; i < 10; i++) { t[0x59 + i] = i18nc("numpad key, %1 is a digit", "Num %1", (i + 1) % 10); }
        return t;
    }
    // i18n-ok-end

    // i18n-ok: key caps
    readonly property var mods: [[0x01, "Ctrl"], [0x02, "Shift"], [0x04, "Alt"], [0x08, "Win"], [0x10, "RCtrl"], [0x20, "RShift"], [0x40, "AltGr"], [0x80, "RWin"]] // i18n-ok

    readonly property var media: ({
        0xe2: i18nc("media key", "Mute"), 0xb6: i18nc("media key", "Previous"), 0xb5: i18nc("media key", "Next"),
        0xcd: i18nc("media key", "Play / Pause"), 0xe9: i18nc("media key", "Volume up"), 0xea: i18nc("media key", "Volume down"),
        0xb7: i18nc("media key", "Stop")
    })

    function modifierNames(m) {
        return mods.filter(p => (m & p[0]) !== 0).map(p => p[1]);
    }

    // On-board macro id -> name, filled in by pages that load the macro list.
    property var macroNames: ({})
    // "pid:code" -> software action label, filled in by the Customize tab.
    property var actionLabels: ({})
    property string currentPid: ""
    // key position -> action label for the profile/layer shown in Customize.
    property var actionByKey: ({})

    // Long key name (left/right modifiers spelled out) for the macro editor.
    function hidName(h) {
        if (h >= 0xe0 && h <= 0xe7) {
            return h < 0xe4 ? i18nc("modifier key, left side", "Left %1", hid[h]) : i18nc("modifier key, right side", "Right %1", hid[h]);
        }
        return hid[h] || ("0x" + h.toString(16));
    }

    // HID usage of the physical key in a QML key event (modifier keys -> 0xe0..0xe7), or -1.
    function hidOfKeyEvent(ev) {
        const code = ev.nativeScanCode - 8;
        if (evdevMod[code] !== undefined) {
            return 0xe0 + Math.round(Math.log2(evdevMod[code]));
        }
        const h = evdevHid[code];
        return h === undefined ? -1 : h;
    }

    function macroLabel(a) {
        const id = (a.data[0] << 8) | a.data[1];
        const name = macroNames[id] || i18n("macro 0x%1", id.toString(16));
        switch (a.function) {
        case 3:
            return a.data[2] > 1 ? i18n("%1 (×%2)", name, a.data[2]) : name;
        case 4:
            return i18n("%1 (while held)", name);
        case 5:
            return i18n("%1 (toggle)", name);
        default:
            return i18n("%1 (sequence)", name);
        }
    }

    // Short legend for one assignment (`a` from GetAssignments).
    function legend(a) {
        if (!a) {
            return "";
        }
        if (a.key !== undefined && actionByKey[a.key] !== undefined && (a.function === 2 || a.function === 17)) {
            return i18nc("short key legend", "App");
        }
        switch (a.function) {
        case 0:
            return "—";
        case 1:
            return ({ 1: "L", 2: "R", 3: "M", 4: "M4", 5: "M5", 9: "↑", 10: "↓", 104: "←", 105: "→" })[a.data[0]] || ("M" + a.data[0]);
        case 2: {
            const mods = modifierNames(a.data[0]);
            const key = hid[a.data[1]] || (a.data[1] ? "0x" + a.data[1].toString(16) : "");
            return key === "" ? mods.join("+") : mods.concat([key]).join("+");
        }
        case 3:
        case 4:
        case 5:
        case 15:
            return i18nc("short key legend", "Macro");
        case 6:
            return "DPI";
        case 12:
            return i18nc("short key legend for the Shift layer key", "Shift");
        case 17:
            return a.data[0] === 1 ? "Fn" : (a.data[0] >= 128 ? i18nc("short key legend", "App") : ""); // i18n-ok
        case 13:
        case 14:
            return i18nc("short key legend", "Turbo");
        case 10:
            return media[(a.data[0] << 8) | a.data[1]] || i18nc("short key legend", "Media");
        default:
            return "";
        }
    }

    // Mouse output codes used by mouse functions (firmware numbering).
    readonly property var mouseOut: ({
        1: i18nc("mouse output", "Left click"), 2: i18nc("mouse output", "Right click"), 3: i18nc("mouse output", "Scroll click"),
        4: i18nc("mouse output", "Mouse button 4"), 5: i18nc("mouse output", "Mouse button 5"), 9: i18nc("mouse output", "Scroll up"),
        10: i18nc("mouse output", "Scroll down"), 104: i18nc("mouse output", "Scroll left"), 105: i18nc("mouse output", "Scroll right")
    })
    readonly property var powerKeys: ({ 129: i18nc("system key", "Power"), 130: i18nc("system key", "Sleep"), 131: i18nc("system key", "Wake") })

    function rate(d) {
        const ms = d >= 1 ? d : 143;
        return i18nc("presses per second", "%1/s", Math.round(1000 / ms));
    }

    // Long description for the assignment panel.
    function describe(a) {
        if (!a) {
            return i18n("Not assignable");
        }
        if (a.key !== undefined && actionByKey[a.key] !== undefined && (a.function === 2 || a.function === 17)) {
            return actionByKey[a.key];
        }
        const w = i => ((a.data[i] || 0) << 8) | (a.data[i + 1] || 0);
        switch (a.function) {
        case 0:
            return i18n("Disabled");
        case 1:
            return mouseOut[a.data[0]] || i18n("Mouse button %1", a.data[0]);
        case 2:
            return i18n("Keyboard: %1", legend(a));
        case 3:
        case 4:
        case 5:
        case 15:
            return i18n("Macro: %1", macroLabel(a));
        case 6:
            switch (a.data[0]) {
            case 1:
                return i18n("Sensitivity: stage up");
            case 2:
                return i18n("Sensitivity: stage down");
            case 3:
                return i18n("Sensitivity: stage %1", a.data[1]);
            case 5:
                return w(1) === 0 ? i18n("Sensitivity clutch") : i18n("Sensitivity: clutch %1 x %2", w(1), w(3));
            case 6:
                return i18n("Sensitivity: cycle up");
            case 7:
                return i18n("Sensitivity: cycle down");
            default:
                return a.label;
            }
        case 7:
            return ({ 1: i18n("Profile up"), 2: i18n("Profile down"), 3: i18n("Switch to profile %1", a.data[1]), 4: i18n("Profile cycle up"), 5: i18n("Profile cycle down") })[a.data[0]] || i18n("Switch profile");
        case 9:
            return i18n("System: %1", powerKeys[a.data[0]] || a.data[0]);
        case 10:
            return i18n("Multimedia: %1", media[w(0)] || a.label);
        case 11:
            return i18n("Double click");
        case 12:
            return i18n("Shift layer");
        case 13:
            return i18n("Turbo: %1 (%2)", legend({ function: 2, data: a.data }), rate(w(2)));
        case 14:
            // The wheel tilts' factory setting is a repeating scroll (0x68 left / 0x69 right).
            if (a.data[0] === 0x68 || a.data[0] === 0x69)
                return a.data[0] === 0x68 ? i18n("Repeat scroll left") : i18n("Repeat scroll right");
            return i18n("Turbo: %1 (%2)", mouseOut[a.data[0]] || i18n("button %1", a.data[0]), rate(w(1)));
        case 17:
            if (a.data[0] >= 128 && a.data[0] <= 207) {
                return actionLabels[currentPid + ":" + a.data[0]] || i18n("Software action");
            }
            return a.data[0] === 1 ? i18n("Fn (shift-layer key)") : i18n("Device key %1", a.data[0]);
        case 18:
            return ({ 1: i18n("Scroll mode toggle"), 2: i18n("Scroll stage up"), 3: i18n("Scroll stage down"), 4: i18n("Scroll stage cycle up"), 5: i18n("Scroll stage cycle down") })[a.data[0]] || i18n("Scroll mode");
        default:
            return a.label;
        }
    }

    // Text -> key events (US layout), for the Text function (stored as an on-board macro).
    readonly property var usText: ({
        " ": [0x2c, 0], "\n": [0x28, 0], "\t": [0x2b, 0], "-": [0x2d, 0], "_": [0x2d, 1], "=": [0x2e, 0], "+": [0x2e, 1],
        "[": [0x2f, 0], "{": [0x2f, 1], "]": [0x30, 0], "}": [0x30, 1], "\\": [0x31, 0], "|": [0x31, 1], ";": [0x33, 0], ":": [0x33, 1],
        "'": [0x34, 0], "\"": [0x34, 1], "`": [0x35, 0], "~": [0x35, 1], ",": [0x36, 0], "<": [0x36, 1], ".": [0x37, 0], ">": [0x37, 1],
        "/": [0x38, 0], "?": [0x38, 1], "!": [0x1e, 1], "@": [0x1f, 1], "#": [0x20, 1], "$": [0x21, 1], "%": [0x22, 1], "^": [0x23, 1],
        "&": [0x24, 1], "*": [0x25, 1], "(": [0x26, 1], ")": [0x27, 1]
    })

    function charKey(c) {
        if (usText[c] !== undefined) {
            return usText[c];
        }
        const code = c.charCodeAt(0);
        if (code >= 97 && code <= 122) {
            return [0x04 + code - 97, 0];
        }
        if (code >= 65 && code <= 90) {
            return [0x04 + code - 65, 1];
        }
        if (code >= 49 && code <= 57) {
            return [0x1e + code - 49, 0];
        }
        if (c === "0") {
            return [0x27, 0];
        }
        return null;
    }

    // Returns {events, skipped} — characters the US layout can't type are skipped.
    function textToEvents(text, delayMs) {
        const ev = [];
        let skipped = "";
        for (const c of text) {
            const k = charKey(c);
            if (!k) {
                skipped += c;
                continue;
            }
            if (k[1]) {
                ev.push({ type: "key", hid: 0xe1, down: true });
            }
            ev.push({ type: "key", hid: k[0], down: true }, { type: "key", hid: k[0], down: false });
            if (k[1]) {
                ev.push({ type: "key", hid: 0xe1, down: false });
            }
            if (delayMs > 0) {
                ev.push({ type: "delay", ms: delayMs });
            }
        }
        return { events: ev, skipped: skipped };
    }

    // Common shortcuts (Linux / KDE): [label, modifier bits, HID usage].
    readonly property var shortcuts: [
        [i18nc("keyboard shortcut", "Copy"), 0x01, 0x06], [i18nc("keyboard shortcut", "Cut"), 0x01, 0x1b], [i18nc("keyboard shortcut", "Paste"), 0x01, 0x19], [i18nc("keyboard shortcut", "Undo"), 0x01, 0x1d], [i18nc("keyboard shortcut", "Redo"), 0x03, 0x1d],
        [i18nc("keyboard shortcut", "Select all"), 0x01, 0x04], [i18nc("keyboard shortcut", "Save"), 0x01, 0x16], [i18nc("keyboard shortcut", "Find"), 0x01, 0x09], [i18nc("keyboard shortcut", "New tab"), 0x01, 0x17], [i18nc("keyboard shortcut", "Close tab"), 0x01, 0x1a],
        [i18nc("keyboard shortcut", "Reopen closed tab"), 0x03, 0x17], [i18nc("keyboard shortcut", "Next tab"), 0x01, 0x2b], [i18nc("keyboard shortcut", "Previous tab"), 0x03, 0x2b], [i18nc("keyboard shortcut", "Switch window (Alt+Tab)"), 0x04, 0x2b],
        [i18nc("keyboard shortcut", "Close window (Alt+F4)"), 0x04, 0x3d], [i18nc("keyboard shortcut", "Show desktop (Meta+D)"), 0x08, 0x07], [i18nc("keyboard shortcut", "Application launcher (Meta)"), 0x08, 0],
        [i18nc("keyboard shortcut", "Lock screen (Meta+L)"), 0x08, 0x0f], [i18nc("keyboard shortcut", "Screenshot (Print)"), 0x00, 0x46], [i18nc("keyboard shortcut", "Task manager (Ctrl+Esc)"), 0x01, 0x29],
        [i18nc("keyboard shortcut", "Zoom in"), 0x01, 0x2e], [i18nc("keyboard shortcut", "Zoom out"), 0x01, 0x2d], [i18nc("keyboard shortcut", "Refresh (F5)"), 0x00, 0x3e], [i18nc("keyboard shortcut", "Fullscreen (F11)"), 0x00, 0x44]
    ]

    // ---- Key groups for the key picker: [title, [HID usages]].
    function range(a, b) {
        const r = [];
        for (let i = a; i <= b; i++) {
            r.push(i);
        }
        return r;
    }
    readonly property var keyGroups: [
        [i18nc("key group", "Letters"), range(0x04, 0x1d)],
        [i18nc("key group", "Numbers"), range(0x1e, 0x27)],
        [i18nc("key group", "Function keys"), range(0x3a, 0x45).concat(range(0x68, 0x73))],
        [i18nc("key group", "Navigation"), [0x52, 0x51, 0x50, 0x4f, 0x4a, 0x4d, 0x4b, 0x4e, 0x49, 0x4c]],
        [i18nc("key group", "Editing and spacing"), [0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x39, 0x65, 0x46, 0x47, 0x48]],
        [i18nc("key group", "Punctuation"), [0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x32, 0x64]],
        [i18nc("key group", "Numpad"), range(0x53, 0x63)],
        [i18nc("key group", "Modifiers"), range(0xe0, 0xe7)]
    ]
    // Keyboard-function data [modifier bits, usage] for a picked key (modifier keys are a bit, no usage).
    function pickedData(h, mods) {
        return h >= 0xe0 && h <= 0xe7 ? [1 << (h - 0xe0), 0] : [mods, h];
    }
    // Which group a HID usage belongs to (index into keyGroups), or -1.
    function groupOf(h) {
        return keyGroups.findIndex(g => g[1].indexOf(h) >= 0);
    }

    // ---- Shortcut catalogue: [category id, title, [[label, modifier bits, HID usage], ...]].
    // Modifier bits: 1 Ctrl, 2 Shift, 4 Alt, 8 Meta (Windows key). Linux / KDE defaults.
    readonly property var shortcutCatalogue: [
        ["system", i18nc("shortcut category", "System"), [
            [i18nc("keyboard shortcut", "Switch window (Alt+Tab)"), 0x04, 0x2b], [i18nc("keyboard shortcut", "Close window (Alt+F4)"), 0x04, 0x3d], [i18nc("keyboard shortcut", "Show desktop (Meta+D)"), 0x08, 0x07],
            [i18nc("keyboard shortcut", "Application launcher (Meta)"), 0x08, 0], [i18nc("keyboard shortcut", "Overview (Meta+W)"), 0x08, 0x1a], [i18nc("keyboard shortcut", "Lock screen (Meta+L)"), 0x08, 0x0f],
            [i18nc("keyboard shortcut", "Run command (Alt+Space)"), 0x04, 0x2c], [i18nc("keyboard shortcut", "Screenshot (Print)"), 0x00, 0x46], [i18nc("keyboard shortcut", "Task manager (Ctrl+Esc)"), 0x01, 0x29],
            [i18nc("keyboard shortcut", "Maximise window (Meta+Up)"), 0x08, 0x52], [i18nc("keyboard shortcut", "Minimise window (Meta+Down)"), 0x08, 0x51], [i18nc("keyboard shortcut", "Emoji picker (Meta+.)"), 0x08, 0x37],
            [i18nc("keyboard shortcut", "Clipboard history (Meta+V)"), 0x08, 0x19], [i18nc("keyboard shortcut", "Next desktop (Ctrl+Meta+Right)"), 0x09, 0x4f], [i18nc("keyboard shortcut", "Previous desktop (Ctrl+Meta+Left)"), 0x09, 0x50]
        ]],
        ["editing", i18nc("shortcut category", "Editing"), [
            [i18nc("keyboard shortcut", "Copy"), 0x01, 0x06], [i18nc("keyboard shortcut", "Cut"), 0x01, 0x1b], [i18nc("keyboard shortcut", "Paste"), 0x01, 0x19], [i18nc("keyboard shortcut", "Paste without formatting"), 0x03, 0x19],
            [i18nc("keyboard shortcut", "Undo"), 0x01, 0x1d], [i18nc("keyboard shortcut", "Redo"), 0x03, 0x1d], [i18nc("keyboard shortcut", "Select all"), 0x01, 0x04], [i18nc("keyboard shortcut", "Find"), 0x01, 0x09], [i18nc("keyboard shortcut", "Replace"), 0x01, 0x0b],
            [i18nc("keyboard shortcut", "Save"), 0x01, 0x16], [i18nc("keyboard shortcut", "Save as"), 0x03, 0x16], [i18nc("keyboard shortcut", "Open"), 0x01, 0x12], [i18nc("keyboard shortcut", "New"), 0x01, 0x11], [i18nc("keyboard shortcut", "Print"), 0x01, 0x13]
        ]],
        ["browser", i18nc("shortcut category", "Browser"), [
            [i18nc("keyboard shortcut", "New tab"), 0x01, 0x17], [i18nc("keyboard shortcut", "Close tab"), 0x01, 0x1a], [i18nc("keyboard shortcut", "Reopen closed tab"), 0x03, 0x17], [i18nc("keyboard shortcut", "Next tab"), 0x01, 0x2b],
            [i18nc("keyboard shortcut", "Previous tab"), 0x03, 0x2b], [i18nc("keyboard shortcut", "Address bar"), 0x01, 0x0f], [i18nc("keyboard shortcut", "Refresh"), 0x00, 0x3e], [i18nc("keyboard shortcut", "Refresh without cache"), 0x03, 0x15],
            [i18nc("keyboard shortcut", "Back"), 0x04, 0x50], [i18nc("keyboard shortcut", "Forward"), 0x04, 0x4f], [i18nc("keyboard shortcut", "Home page"), 0x04, 0x4a], [i18nc("keyboard shortcut", "Bookmark page"), 0x01, 0x07],
            [i18nc("keyboard shortcut", "Developer tools"), 0x00, 0x45], [i18nc("keyboard shortcut", "Zoom in"), 0x01, 0x2e], [i18nc("keyboard shortcut", "Zoom out"), 0x01, 0x2d], [i18nc("keyboard shortcut", "Reset zoom"), 0x01, 0x27], [i18nc("keyboard shortcut", "Full screen"), 0x00, 0x44]
        ]],
        ["office", i18nc("shortcut category", "Office"), [
            [i18nc("keyboard shortcut", "Bold"), 0x01, 0x05], [i18nc("keyboard shortcut", "Italic"), 0x01, 0x0c], [i18nc("keyboard shortcut", "Underline"), 0x01, 0x18], [i18nc("keyboard shortcut", "Align left"), 0x01, 0x0f], [i18nc("keyboard shortcut", "Align centre"), 0x01, 0x08],
            [i18nc("keyboard shortcut", "Align right"), 0x01, 0x15], [i18nc("keyboard shortcut", "Insert link"), 0x01, 0x0e], [i18nc("keyboard shortcut", "Find and replace"), 0x01, 0x0b], [i18nc("keyboard shortcut", "Go to line / page"), 0x01, 0x0a],
            [i18nc("keyboard shortcut", "Spell check"), 0x00, 0x40], [i18nc("keyboard shortcut", "Select all"), 0x01, 0x04], [i18nc("keyboard shortcut", "Save"), 0x01, 0x16], [i18nc("keyboard shortcut", "Print"), 0x01, 0x13]
        ]],
        ["creative", i18nc("shortcut category", "Creative apps"), [
            [i18nc("keyboard shortcut", "Undo"), 0x01, 0x1d], [i18nc("keyboard shortcut", "Redo"), 0x03, 0x1d], [i18nc("keyboard shortcut", "Brush smaller ([)"), 0x00, 0x2f], [i18nc("keyboard shortcut", "Brush bigger (])"), 0x00, 0x30],
            [i18nc("keyboard shortcut", "Zoom in"), 0x01, 0x2e], [i18nc("keyboard shortcut", "Zoom out"), 0x01, 0x2d], [i18nc("keyboard shortcut", "Fit on screen"), 0x01, 0x27], [i18nc("keyboard shortcut", "Brush (B)"), 0x00, 0x05], [i18nc("keyboard shortcut", "Eraser (E)"), 0x00, 0x08],
            [i18nc("keyboard shortcut", "Move (V)"), 0x00, 0x19], [i18nc("keyboard shortcut", "Eyedropper (I)"), 0x00, 0x0c], [i18nc("keyboard shortcut", "Deselect"), 0x01, 0x07], [i18nc("keyboard shortcut", "Invert selection"), 0x03, 0x0c],
            [i18nc("keyboard shortcut", "Free transform"), 0x01, 0x17], [i18nc("keyboard shortcut", "New layer"), 0x03, 0x11], [i18nc("keyboard shortcut", "Merge layer down"), 0x01, 0x08]
        ]],
        ["video", i18nc("shortcut category", "Video"), [
            [i18nc("keyboard shortcut", "Play / pause (Space)"), 0x00, 0x2c], [i18nc("keyboard shortcut", "Back 5 seconds"), 0x00, 0x50], [i18nc("keyboard shortcut", "Forward 5 seconds"), 0x00, 0x4f], [i18nc("keyboard shortcut", "Back 10 seconds (J)"), 0x00, 0x0d],
            [i18nc("keyboard shortcut", "Pause (K)"), 0x00, 0x0e], [i18nc("keyboard shortcut", "Forward 10 seconds (L)"), 0x00, 0x0f], [i18nc("keyboard shortcut", "Previous frame (,)"), 0x00, 0x36], [i18nc("keyboard shortcut", "Next frame (.)"), 0x00, 0x37],
            [i18nc("keyboard shortcut", "Mark in (I)"), 0x00, 0x0c], [i18nc("keyboard shortcut", "Mark out (O)"), 0x00, 0x12], [i18nc("keyboard shortcut", "Split clip (S)"), 0x00, 0x16], [i18nc("keyboard shortcut", "Full screen (F)"), 0x00, 0x09], [i18nc("keyboard shortcut", "Mute (M)"), 0x00, 0x10]
        ]],
        ["meeting", i18nc("shortcut category", "Meetings"), [
            [i18nc("keyboard shortcut", "Mute / unmute (Zoom)"), 0x04, 0x04], [i18nc("keyboard shortcut", "Camera on / off (Zoom)"), 0x04, 0x19], [i18nc("keyboard shortcut", "Raise hand (Zoom)"), 0x04, 0x1c], [i18nc("keyboard shortcut", "Share screen (Zoom)"), 0x04, 0x16],
            [i18nc("keyboard shortcut", "Mute / unmute (Meet)"), 0x01, 0x07], [i18nc("keyboard shortcut", "Camera on / off (Meet)"), 0x01, 0x08], [i18nc("keyboard shortcut", "Mute / unmute (Teams)"), 0x03, 0x10],
            [i18nc("keyboard shortcut", "Camera on / off (Teams)"), 0x03, 0x12], [i18nc("keyboard shortcut", "Raise hand (Teams)"), 0x03, 0x0e], [i18nc("keyboard shortcut", "Leave meeting (Teams)"), 0x03, 0x0b],
            [i18nc("keyboard shortcut", "Mute microphone (Discord)"), 0x03, 0x10], [i18nc("keyboard shortcut", "Deafen (Discord)"), 0x03, 0x07]
        ]]
    ]
    // The flat list the first version had (kept for callers that still read it).
    readonly property var allShortcuts: shortcutCatalogue.reduce((a, c) => a.concat(c[2]), [])

    // Which catalogue category suits a linked app (by its name or the text it is matched by), or "".
    readonly property var appCategories: [
        ["browser", ["firefox", "chrome", "chromium", "brave", "vivaldi", "opera", "edge", "zen", "librewolf", "browser"]],
        ["office", ["libreoffice", "writer", "calc", "impress", "word", "excel", "powerpoint", "onlyoffice", "soffice", "obsidian", "notion", "thunderbird"]],
        // i18n-ok: app name matching
        ["creative", ["photoshop", "gimp", "krita", "blender", "inkscape", "aseprite", "darktable", "figma", "illustrator", "affinity", "clip studio", "paint"]], // i18n-ok
        ["video", ["vlc", "mpv", "kdenlive", "davinci", "resolve", "premiere", "shotcut", "olive", "handbrake", "obs", "youtube", "netflix", "plex", "jellyfin"]],
        ["meeting", ["zoom", "teams", "discord", "slack", "skype", "meet", "jitsi", "webex", "telegram"]]
    ]
    function categoryForApp(text) {
        const t = String(text || "").toLowerCase();
        for (const c of appCategories) {
            if (c[1].some(k => t.indexOf(k) >= 0)) {
                return c[0];
            }
        }
        return "";
    }
    // Text of a shortcut chip: "Ctrl+Shift+K".
    // i18n-ok-begin (key caps)
    function chord(mods, usage) {
        const parts = [mods & 1 ? "Ctrl" : "", mods & 2 ? "Shift" : "", mods & 4 ? "Alt" : "", mods & 8 ? "Meta" : ""].filter(x => x !== "");
        if (usage) {
            parts.push(hid[usage] || ("0x" + usage.toString(16)));
        }
        return parts.join("+");
    }
    // i18n-ok-end

    // Linux evdev keycode -> HID keyboard usage; modifier keys -> modifier bit.
    readonly property var evdevHid: ({ 1: 0x29, 2: 0x1e, 3: 0x1f, 4: 0x20, 5: 0x21, 6: 0x22, 7: 0x23, 8: 0x24, 9: 0x25, 10: 0x26, 11: 0x27, 12: 0x2d, 13: 0x2e, 14: 0x2a, 15: 0x2b, 16: 0x14, 17: 0x1a, 18: 0x08, 19: 0x15, 20: 0x17, 21: 0x1c, 22: 0x18, 23: 0x0c, 24: 0x12, 25: 0x13, 26: 0x2f, 27: 0x30, 28: 0x28, 30: 0x04, 31: 0x16, 32: 0x07, 33: 0x09, 34: 0x0a, 35: 0x0b, 36: 0x0d, 37: 0x0e, 38: 0x0f, 39: 0x33, 40: 0x34, 41: 0x35, 43: 0x31, 44: 0x1d, 45: 0x1b, 46: 0x06, 47: 0x19, 48: 0x05, 49: 0x11, 50: 0x10, 51: 0x36, 52: 0x37, 53: 0x38, 55: 0x55, 57: 0x2c, 58: 0x39, 59: 0x3a, 60: 0x3b, 61: 0x3c, 62: 0x3d, 63: 0x3e, 64: 0x3f, 65: 0x40, 66: 0x41, 67: 0x42, 68: 0x43, 69: 0x53, 70: 0x47, 71: 0x5f, 72: 0x60, 73: 0x61, 74: 0x56, 75: 0x5c, 76: 0x5d, 77: 0x5e, 78: 0x57, 79: 0x59, 80: 0x5a, 81: 0x5b, 82: 0x62, 83: 0x63, 87: 0x44, 88: 0x45, 96: 0x58, 98: 0x54, 99: 0x46, 102: 0x4a, 103: 0x52, 104: 0x4b, 105: 0x50, 106: 0x4f, 107: 0x4d, 108: 0x51, 109: 0x4e, 110: 0x49, 111: 0x4c, 119: 0x48, 127: 0x65, 183: 0x68, 184: 0x69, 185: 0x6a, 186: 0x6b, 187: 0x6c, 188: 0x6d, 189: 0x6e, 190: 0x6f, 191: 0x70, 192: 0x71, 193: 0x72, 194: 0x73 })
    readonly property var evdevMod: ({ 29: 0x01, 42: 0x02, 54: 0x20, 56: 0x04, 97: 0x10, 100: 0x40, 125: 0x08, 126: 0x80 })

    // Factory defaults per key position and layer: [function, data]. Read from freshly created
    // (factory-default) on-board profiles on the BlackWidow V4 Pro and Basilisk V3 Pro.
    readonly property var kbDefaults: ({
        1: [2, [0, 0x35]], 2: [2, [0, 0x1e]], 3: [2, [0, 0x1f]], 4: [2, [0, 0x20]], 5: [2, [0, 0x21]],
        6: [2, [0, 0x22]], 7: [2, [0, 0x23]], 8: [2, [0, 0x24]], 9: [2, [0, 0x25]], 10: [2, [0, 0x26]],
        11: [2, [0, 0x27]], 12: [2, [0, 0x2d]], 13: [2, [0, 0x2e]], 14: [2, [0, 0x89]], 15: [2, [0, 0x2a]],
        16: [2, [0, 0x2b]], 17: [2, [0, 0x14]], 18: [2, [0, 0x1a]], 19: [2, [0, 8]], 20: [2, [0, 0x15]],
        21: [2, [0, 0x17]], 22: [2, [0, 0x1c]], 23: [2, [0, 0x18]], 24: [2, [0, 0xc]], 25: [2, [0, 0x12]],
        26: [2, [0, 0x13]], 27: [2, [0, 0x2f]], 28: [2, [0, 0x30]], 29: [2, [0, 0x31]], 30: [2, [0, 0x39]],
        31: [2, [0, 4]], 32: [2, [0, 0x16]], 33: [2, [0, 7]], 34: [2, [0, 9]], 35: [2, [0, 0xa]],
        36: [2, [0, 0xb]], 37: [2, [0, 0xd]], 38: [2, [0, 0xe]], 39: [2, [0, 0xf]], 40: [2, [0, 0x33]],
        41: [2, [0, 0x34]], 42: [2, [0, 0x32]], 43: [2, [0, 0x28]], 44: [2, [2, 0]], 45: [2, [0, 0x64]],
        46: [2, [0, 0x1d]], 47: [2, [0, 0x1b]], 48: [2, [0, 6]], 49: [2, [0, 0x19]], 50: [2, [0, 5]],
        51: [2, [0, 0x11]], 52: [2, [0, 0x10]], 53: [2, [0, 0x36]], 54: [2, [0, 0x37]], 55: [2, [0, 0x38]],
        56: [2, [0, 0x87]], 57: [2, [0x20, 0]], 58: [2, [1, 0]], 59: [17, [1]], 60: [2, [4, 0]],
        61: [2, [0, 0x2c]], 62: [2, [0x40, 0]], 64: [2, [0x10, 0]], 75: [2, [0, 0x49]], 76: [2, [0, 0x4c]],
        79: [2, [0, 0x50]], 80: [2, [0, 0x4a]], 81: [2, [0, 0x4d]], 83: [2, [0, 0x52]], 84: [2, [0, 0x51]],
        85: [2, [0, 0x4b]], 86: [2, [0, 0x4e]], 89: [2, [0, 0x4f]], 90: [2, [0, 0x53]], 91: [2, [0, 0x5f]],
        92: [2, [0, 0x5c]], 93: [2, [0, 0x59]], 95: [2, [0, 0x54]], 96: [2, [0, 0x60]], 97: [2, [0, 0x5d]],
        98: [2, [0, 0x5a]], 99: [2, [0, 0x62]], 100: [2, [0, 0x55]], 101: [2, [0, 0x61]], 102: [2, [0, 0x5e]],
        103: [2, [0, 0x5b]], 104: [2, [0, 0x63]], 105: [2, [0, 0x56]], 106: [2, [0, 0x57]],
        107: [2, [0, 0x85]], 108: [2, [0, 0x58]], 110: [2, [0, 0x29]], 112: [2, [0, 0x3a]],
        113: [2, [0, 0x3b]], 114: [2, [0, 0x3c]], 115: [2, [0, 0x3d]], 116: [2, [0, 0x3e]],
        117: [2, [0, 0x3f]], 118: [2, [0, 0x40]], 119: [2, [0, 0x41]], 120: [2, [0, 0x42]],
        121: [2, [0, 0x43]], 122: [2, [0, 0x44]], 123: [2, [0, 0x45]], 124: [2, [0, 0x46]],
        125: [2, [0, 0x47]], 126: [2, [0, 0x48]], 127: [2, [8, 0]], 129: [2, [0, 0x65]], 150: [2, [0, 0x8b]],
        151: [2, [0, 0x88]], 152: [2, [0, 0x91]], 153: [2, [0, 0x90]], 154: [2, [0, 0x8a]], 160: [17, [0x20]],
        161: [17, [0x21]], 162: [17, [0x22]], 163: [17, [0x23]], 164: [17, [0x24]], 169: [17, [0x60]],
        170: [17, [0x26]], 171: [17, [0x27]], 172: [17, [0x28]], 173: [17, [0x29]], 174: [17, [0x2a]],
        175: [17, [0x2b]], 200: [10, [0, 0xe2]], 201: [10, [0, 0xb6]], 202: [10, [0, 0xb5]],
        204: [10, [0, 0xe9]], 205: [10, [0, 0xcd]], 206: [10, [0, 0xea]], 207: [18, [0x63]], 208: [18, [0x64]],
        209: [18, [0x65]]
    })
    readonly property var kbDefaultsShift: ({
        1: [2, [0, 0x35]], 2: [2, [0, 0x1e]], 3: [2, [0, 0x1f]], 4: [2, [0, 0x20]], 5: [2, [0, 0x21]],
        6: [2, [0, 0x22]], 7: [2, [0, 0x23]], 8: [2, [0, 0x24]], 9: [2, [0, 0x25]], 10: [2, [0, 0x26]],
        11: [2, [0, 0x27]], 12: [2, [0, 0x2d]], 13: [2, [0, 0x2e]], 14: [2, [0, 0x89]], 15: [2, [0, 0x2a]],
        16: [2, [0, 0x2b]], 17: [2, [0, 0x14]], 18: [2, [0, 0x1a]], 19: [2, [0, 8]], 20: [2, [0, 0x15]],
        21: [2, [0, 0x17]], 22: [2, [0, 0x1c]], 23: [2, [0, 0x18]], 24: [2, [0, 0xc]], 25: [2, [0, 0x12]],
        26: [2, [0, 0x13]], 27: [2, [0, 0x2f]], 28: [2, [0, 0x30]], 29: [2, [0, 0x31]], 30: [2, [0, 0x39]],
        31: [2, [0, 4]], 32: [2, [0, 0x16]], 33: [2, [0, 7]], 34: [2, [0, 9]], 35: [2, [0, 0xa]],
        36: [2, [0, 0xb]], 37: [2, [0, 0xd]], 38: [2, [0, 0xe]], 39: [2, [0, 0xf]], 40: [2, [0, 0x33]],
        41: [2, [0, 0x34]], 42: [2, [0, 0x32]], 43: [2, [0, 0x28]], 44: [2, [2, 0]], 45: [2, [0, 0x64]],
        46: [2, [0, 0x1d]], 47: [2, [0, 0x1b]], 48: [2, [0, 6]], 49: [2, [0, 0x19]], 50: [2, [0, 5]],
        51: [2, [0, 0x11]], 52: [2, [0, 0x10]], 53: [2, [0, 0x36]], 54: [2, [0, 0x37]], 55: [2, [0, 0x38]],
        56: [2, [0, 0x87]], 57: [2, [0x20, 0]], 58: [2, [1, 0]], 59: [17, [1]], 60: [2, [4, 0]],
        61: [2, [0, 0x2c]], 62: [2, [0x40, 0]], 64: [2, [0x10, 0]], 75: [2, [0, 0x49]], 76: [2, [0, 0x4c]],
        79: [2, [0, 0x50]], 80: [2, [0, 0x4a]], 81: [2, [0, 0x4d]], 83: [2, [0, 0x52]], 84: [2, [0, 0x51]],
        85: [2, [0, 0x4b]], 86: [2, [0, 0x4e]], 89: [2, [0, 0x4f]], 90: [2, [0, 0x53]], 91: [2, [0, 0x5f]],
        92: [2, [0, 0x5c]], 93: [2, [0, 0x59]], 95: [2, [0, 0x54]], 96: [2, [0, 0x60]], 97: [2, [0, 0x5d]],
        98: [2, [0, 0x5a]], 99: [2, [0, 0x62]], 100: [2, [0, 0x55]], 101: [2, [0, 0x61]], 102: [2, [0, 0x5e]],
        103: [2, [0, 0x5b]], 104: [2, [0, 0x63]], 105: [2, [0, 0x56]], 106: [2, [0, 0x57]],
        107: [2, [0, 0x85]], 108: [2, [0, 0x58]], 110: [2, [0, 0x29]], 112: [2, [0, 0x3a]],
        113: [2, [0, 0x3b]], 114: [2, [0, 0x3c]], 115: [2, [0, 0x3d]], 116: [2, [0, 0x3e]],
        117: [2, [0, 0x3f]], 118: [2, [0, 0x40]], 119: [2, [0, 0x41]], 120: [17, [4]], 121: [17, [3]],
        122: [17, [9]], 123: [17, [8]], 124: [2, [0, 0x46]], 125: [2, [0, 0x47]], 126: [9, [0x82]],
        127: [2, [8, 0]], 129: [7, [4]], 150: [2, [0, 0x8b]], 151: [2, [0, 0x88]], 152: [2, [0, 0x91]],
        153: [2, [0, 0x90]], 154: [2, [0, 0x8a]], 160: [17, [0x20]], 161: [17, [0x21]], 162: [17, [0x22]],
        163: [17, [0x23]], 164: [17, [0x24]], 169: [17, [0x60]], 170: [17, [0x26]], 171: [17, [0x27]],
        172: [17, [0x28]], 173: [17, [0x29]], 174: [17, [0x2a]], 175: [17, [0x2b]], 200: [10, [0, 0xe2]],
        201: [10, [0, 0xb6]], 202: [10, [0, 0xb5]], 204: [10, [0, 0xe9]], 205: [10, [0, 0xcd]],
        206: [10, [0, 0xea]], 207: [18, [0x63]], 208: [18, [0x64]], 209: [18, [0x65]]
    })
    readonly property var mouseDefaults: ({
        1: [1, [1]], 2: [1, [2]], 3: [1, [3]], 4: [1, [4]], 5: [1, [5]], 9: [1, [9]], 10: [1, [0xa]],
        14: [7, [4]], 15: [6, [5]], 52: [14, [0x68]], 53: [14, [0x69]], 96: [6, [6]], 106: [18, [1]]
    })
    readonly property var mouseDefaultsShift: ({
        1: [1, [1]], 2: [1, [2]], 3: [1, [3]], 4: [1, [4]], 5: [1, [5]], 9: [1, [9]], 10: [1, [0xa]],
        14: [7, [4]], 15: [6, [5]], 52: [14, [0x68]], 53: [14, [0x69]], 96: [6, [6]], 106: [18, [1]]
    })

    // Factory default {function, data} for a key on a layer, or null when not known.
    // Keyboards have separate shift tables; mice fall back to the standard table on layer 1.
    function defaultFor(isMouse, pos, layer) {
        if (isMouse) {
            const d = (layer === 1 ? mouseDefaultsShift[pos] : undefined) || mouseDefaults[pos];
            return d ? { function: d[0], data: d[1] } : null;
        }
        const d = (layer === 1 ? kbDefaultsShift : kbDefaults)[pos];
        return d ? { function: d[0], data: d[1] } : null;
    }
    readonly property string defaultUnknownTip: i18n("Factory default not known for this key yet")

    // Keyboard-function data [modifier, usage] from a QML key event (uses the X/Wayland
    // scancode = evdev code + 8), or null if the key isn't mappable.
    function fromKeyEvent(ev) {
        const code = ev.nativeScanCode - 8;
        if (evdevMod[code] !== undefined) {
            return [evdevMod[code], 0];
        }
        const hid = evdevHid[code];
        if (hid === undefined) {
            return null;
        }
        let m = 0;
        if (ev.modifiers & Qt.ControlModifier) m |= 0x01;
        if (ev.modifiers & Qt.ShiftModifier) m |= 0x02;
        if (ev.modifiers & Qt.AltModifier) m |= 0x04;
        if (ev.modifiers & Qt.MetaModifier) m |= 0x08;
        return [m, hid];
    }
}
