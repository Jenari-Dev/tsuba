pragma Singleton

import QtQuick

QtObject {
    // HID keyboard-page usage → short key legend.
    readonly property var hid: ({
        0x04: "A", 0x05: "B", 0x06: "C", 0x07: "D", 0x08: "E", 0x09: "F", 0x0a: "G", 0x0b: "H", 0x0c: "I",
        0x0d: "J", 0x0e: "K", 0x0f: "L", 0x10: "M", 0x11: "N", 0x12: "O", 0x13: "P", 0x14: "Q", 0x15: "R",
        0x16: "S", 0x17: "T", 0x18: "U", 0x19: "V", 0x1a: "W", 0x1b: "X", 0x1c: "Y", 0x1d: "Z",
        0x1e: "1", 0x1f: "2", 0x20: "3", 0x21: "4", 0x22: "5", 0x23: "6", 0x24: "7", 0x25: "8", 0x26: "9", 0x27: "0",
        0x28: "Enter", 0x29: "Esc", 0x2a: "Backspace", 0x2b: "Tab", 0x2c: "Space", 0x2d: "-", 0x2e: "=",
        0x2f: "[", 0x30: "]", 0x31: "\\", 0x32: "#", 0x33: ";", 0x34: "'", 0x35: "`", 0x36: ",", 0x37: ".",
        0x38: "/", 0x39: "Caps Lock",
        0x3a: "F1", 0x3b: "F2", 0x3c: "F3", 0x3d: "F4", 0x3e: "F5", 0x3f: "F6", 0x40: "F7", 0x41: "F8",
        0x42: "F9", 0x43: "F10", 0x44: "F11", 0x45: "F12",
        0x46: "PrtSc", 0x47: "ScrLk", 0x48: "Pause", 0x49: "Ins", 0x4a: "Home", 0x4b: "PgUp", 0x4c: "Del",
        0x4d: "End", 0x4e: "PgDn", 0x4f: "→", 0x50: "←", 0x51: "↓", 0x52: "↑",
        0x53: "Num", 0x54: "/", 0x55: "*", 0x56: "-", 0x57: "+", 0x58: "Enter",
        0x59: "1", 0x5a: "2", 0x5b: "3", 0x5c: "4", 0x5d: "5", 0x5e: "6", 0x5f: "7", 0x60: "8", 0x61: "9",
        0x62: "0", 0x63: ".", 0x64: "\\", 0x65: "Menu",
        0x68: "F13", 0x69: "F14", 0x6a: "F15", 0x6b: "F16", 0x6c: "F17", 0x6d: "F18", 0x6e: "F19",
        0x6f: "F20", 0x70: "F21", 0x71: "F22", 0x72: "F23", 0x73: "F24",
        0x46: "Print Screen", 0x47: "Scroll Lock", 0x48: "Pause", 0x49: "Insert", 0x4a: "Home", 0x4b: "Page Up",
        0x4c: "Delete", 0x4d: "End", 0x4e: "Page Down", 0x4f: "Right", 0x50: "Left", 0x51: "Down", 0x52: "Up",
        0x53: "Num Lock", 0x54: "Num /", 0x55: "Num *", 0x56: "Num -", 0x57: "Num +", 0x58: "Num Enter",
        0x59: "Num 1", 0x5a: "Num 2", 0x5b: "Num 3", 0x5c: "Num 4", 0x5d: "Num 5", 0x5e: "Num 6", 0x5f: "Num 7",
        0x60: "Num 8", 0x61: "Num 9", 0x62: "Num 0", 0x63: "Num .", 0x64: "\\|", 0x65: "Menu",
        0xe0: "Ctrl", 0xe1: "Shift", 0xe2: "Alt", 0xe3: "Win", 0xe4: "Ctrl", 0xe5: "Shift", 0xe6: "Alt", 0xe7: "Win"
    })

    readonly property var mods: [[0x01, "Ctrl"], [0x02, "Shift"], [0x04, "Alt"], [0x08, "Win"], [0x10, "RCtrl"], [0x20, "RShift"], [0x40, "AltGr"], [0x80, "RWin"]]

    readonly property var media: ({ 0xe2: "Mute", 0xb6: "Previous", 0xb5: "Next", 0xcd: "Play / Pause", 0xe9: "Volume up", 0xea: "Volume down", 0xb7: "Stop" })

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
            return (h < 0xe4 ? "Left " : "Right ") + hid[h];
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
        const name = macroNames[id] || ("macro 0x" + id.toString(16));
        switch (a.function) {
        case 3:
            return a.data[2] > 1 ? `${name} (×${a.data[2]})` : name;
        case 4:
            return name + " (while held)";
        case 5:
            return name + " (toggle)";
        default:
            return name + " (sequence)";
        }
    }

    // Short legend for one assignment (`a` from GetAssignments).
    function legend(a) {
        if (!a) {
            return "";
        }
        if (a.key !== undefined && actionByKey[a.key] !== undefined && (a.function === 2 || a.function === 17)) {
            return "App";
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
            return "Macro";
        case 6:
            return "DPI";
        case 12:
            return "HS";
        case 17:
            return a.data[0] === 1 ? "Fn" : (a.data[0] >= 128 ? "App" : "");
        case 13:
        case 14:
            return "Turbo";
        case 10:
            return media[(a.data[0] << 8) | a.data[1]] || "Media";
        default:
            return "";
        }
    }

    // Mouse output codes used by mouse functions (firmware numbering).
    readonly property var mouseOut: ({ 1: "Left click", 2: "Right click", 3: "Scroll click", 4: "Mouse button 4", 5: "Mouse button 5", 9: "Scroll up", 10: "Scroll down", 104: "Scroll left", 105: "Scroll right" })
    readonly property var powerKeys: ({ 129: "Power", 130: "Sleep", 131: "Wake" })

    function rate(d) {
        const ms = d >= 1 ? d : 143;
        return Math.round(1000 / ms) + "/s";
    }

    // Long description for the assignment panel.
    function describe(a) {
        if (!a) {
            return "Not assignable";
        }
        if (a.key !== undefined && actionByKey[a.key] !== undefined && (a.function === 2 || a.function === 17)) {
            return actionByKey[a.key];
        }
        const w = i => ((a.data[i] || 0) << 8) | (a.data[i + 1] || 0);
        switch (a.function) {
        case 0:
            return "Disabled";
        case 1:
            return mouseOut[a.data[0]] || ("Mouse button " + a.data[0]);
        case 2:
            return "Keyboard: " + legend(a);
        case 3:
        case 4:
        case 5:
        case 15:
            return "Macro: " + macroLabel(a);
        case 6:
            switch (a.data[0]) {
            case 1:
                return "Sensitivity: stage up";
            case 2:
                return "Sensitivity: stage down";
            case 3:
                return "Sensitivity: stage " + a.data[1];
            case 5:
                return w(1) === 0 ? "Sensitivity clutch" : `Sensitivity: clutch ${w(1)} x ${w(3)}`;
            case 6:
                return "Sensitivity: cycle up";
            case 7:
                return "Sensitivity: cycle down";
            default:
                return a.label;
            }
        case 7:
            return ({ 1: "Profile up", 2: "Profile down", 3: "Switch to profile " + a.data[1], 4: "Profile cycle up", 5: "Profile cycle down" })[a.data[0]] || "Switch profile";
        case 9:
            return "System: " + (powerKeys[a.data[0]] || a.data[0]);
        case 10:
            return "Multimedia: " + (media[w(0)] || a.label);
        case 11:
            return "Double click";
        case 12:
            return "Shift layer";
        case 13:
            return `Turbo: ${legend({ function: 2, data: a.data })} (${rate(w(2))})`;
        case 14:
            // The wheel tilts' factory setting is a repeating scroll (0x68 left / 0x69 right).
            if (a.data[0] === 0x68 || a.data[0] === 0x69)
                return a.data[0] === 0x68 ? "Repeat scroll left" : "Repeat scroll right";
            return `Turbo: ${mouseOut[a.data[0]] || "button " + a.data[0]} (${rate(w(1))})`;
        case 17:
            if (a.data[0] >= 128 && a.data[0] <= 207) {
                return actionLabels[currentPid + ":" + a.data[0]] || "Software action";
            }
            return a.data[0] === 1 ? "Fn (shift-layer key)" : "Device key " + a.data[0];
        case 18:
            return ({ 1: "Scroll mode toggle", 2: "Scroll stage up", 3: "Scroll stage down", 4: "Scroll stage cycle up", 5: "Scroll stage cycle down" })[a.data[0]] || "Scroll mode";
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
        ["Copy", 0x01, 0x06], ["Cut", 0x01, 0x1b], ["Paste", 0x01, 0x19], ["Undo", 0x01, 0x1d], ["Redo", 0x03, 0x1d],
        ["Select all", 0x01, 0x04], ["Save", 0x01, 0x16], ["Find", 0x01, 0x09], ["New tab", 0x01, 0x17], ["Close tab", 0x01, 0x1a],
        ["Reopen closed tab", 0x03, 0x17], ["Next tab", 0x01, 0x2b], ["Previous tab", 0x03, 0x2b], ["Switch window (Alt+Tab)", 0x04, 0x2b],
        ["Close window (Alt+F4)", 0x04, 0x3d], ["Show desktop (Meta+D)", 0x08, 0x07], ["Application launcher (Meta)", 0x08, 0],
        ["Lock screen (Meta+L)", 0x08, 0x0f], ["Screenshot (Print)", 0x00, 0x46], ["Task manager (Ctrl+Esc)", 0x01, 0x29],
        ["Zoom in", 0x01, 0x2e], ["Zoom out", 0x01, 0x2d], ["Refresh (F5)", 0x00, 0x3e], ["Fullscreen (F11)", 0x00, 0x44]
    ]

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
    readonly property string defaultUnknownTip: "Factory default not known for this key yet"

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
