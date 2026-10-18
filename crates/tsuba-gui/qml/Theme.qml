pragma Singleton

import QtQuick
import QtCore

// Dark palette and metrics. The accent is Tsuba's own jade by default and each user can pick
// any colour in Appearance (stored in ~/.config/tsuba/app.ini).
QtObject {
    readonly property color defaultAccent: "#1fc9a7"
    property Settings prefs: Settings {
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/tsuba/app.ini"
        category: "appearance"
        property string accent: "#1fc9a7"
        // "builtin" = always Tsuba's line-art device drawings; "auto" = a personal picture pack if present.
        property string art: "auto"
    }
    // Where a pack view's picture sits in the drawing's coordinate space: [x, y, w, h].
    // `imageRect` is set when the user placed their own picture over Tsuba's drawing.
    function imageRect(view) {
        if (!view) {
            return [0, 0, 1, 1];
        }
        const r = view.imageRect;
        if (r && r.length === 4 && r[2] > 0 && r[3] > 0) {
            return r;
        }
        return [0, 0, view.width, view.height];
    }
    // Bumped after the user's device pictures change, so views reload their packs.
    property int picRev: 0
    // Bumped whenever on-board or Lighting Studio profiles are created, renamed or deleted,
    // so pages that list them (Linked games) reload.
    property int profilesRev: 0
    // Bumped by the refresh button: device pages re-read everything from the daemon.
    property int dataRev: 0
    function setAccent(c) {
        prefs.accent = String(c);
    }

    readonly property color bg: "#212121"
    readonly property color sidebar: "#000000"
    readonly property color bar: "#222222"
    readonly property color panel: "#1a1a1a"
    readonly property color card: "#1a1a1a"
    readonly property color sliderRest: Qt.rgba(accent.r * 0.38, accent.g * 0.38, accent.b * 0.38, 1)
    readonly property color accentText: "#101010"
    readonly property color chrome: "#000000"
    readonly property color panelRaised: "#2d2d2d"
    readonly property color line: "#000000"
    readonly property color border: "#5d5d5d"
    readonly property color borderSoft: "#383838"
    readonly property color text: "#d3d3d3"
    readonly property color textBright: "#ffffff"
    readonly property color textDim: "#707070"
    readonly property color textMuted: "#5d5d5d"
    readonly property color accent: prefs.accent
    readonly property color accentHover: Qt.darker(accent, 1.12)
    // Translucent accent for selected rows / chips.
    readonly property color accentTint: Qt.rgba(accent.r, accent.g, accent.b, 0.10)
    readonly property color accentTintMid: Qt.rgba(accent.r, accent.g, accent.b, 0.13)
    readonly property color accentTintStrong: Qt.rgba(accent.r, accent.g, accent.b, 0.20)
    readonly property color warn: "#fd8611"
    readonly property color danger: "#c8323c"
    readonly property color keyFace: "#1d1d1d"
    readonly property color keyEdge: "#3a3a3a"

    readonly property string font: "Roboto"
    // Razer's title face is proprietary and not shipped; Qt falls back to Roboto / the system sans.
    readonly property string titleFont: "Roboto"
    readonly property int fontSize: 14
    readonly property int small: 12
    readonly property int heading: 20
    readonly property int barHeight: 48
    readonly property int sidebarWidth: 220
}
