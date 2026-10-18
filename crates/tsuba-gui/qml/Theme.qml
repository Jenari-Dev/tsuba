pragma Singleton

import QtQuick
import QtCore

// Palette and metrics. Dark, light or automatic (follows the desktop's colour scheme); the accent is
// Tsuba's own jade by default and each user can pick any colour in Settings (all of it is stored in
// ~/.config/tsuba/app.ini).
QtObject {
    readonly property color defaultAccent: "#1fc9a7"
    property Settings prefs: Settings {
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/tsuba/app.ini"
        category: "appearance"
        property string accent: "#1fc9a7"
        // "builtin" = always Tsuba's line-art device drawings; "auto" = a personal picture pack if present.
        property string art: "auto"
        // "auto" follows the desktop colour scheme; "dark" / "light" force one.
        property string themeMode: "auto"
    }
    // App behaviour (Settings page): [app] group of the same file.
    property Settings app: Settings {
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/tsuba/app.ini"
        category: "app"
        // "system" or a language code such as "ja"; read by the app at start (restart to change).
        property string language: "system"
        // Close button hides the window to the tray (only used while a tray exists).
        property bool closeToTray: true
        // Which page the app opens on: "home" or "last" (the page open when it was closed).
        property string startPage: "home"
        property string lastPage: ""
        property bool showTips: true
        // Welcome screen shown once; reset from Settings.
        property bool onboarded: false
        // Conflict banners the user dismissed (JSON array of ids).
        property string dismissedConflicts: "[]"
    }
    // The scheme actually in use. Unknown (headless / no portal) counts as dark, Tsuba's original look.
    readonly property bool dark: prefs.themeMode === "dark" || (prefs.themeMode !== "light" && Qt.styleHints.colorScheme !== Qt.Light)
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

    // ---- colours: [dark, light]
    readonly property color bg: dark ? "#212121" : "#f1f3f4"
    readonly property color sidebar: dark ? "#000000" : "#e2e6e8"
    readonly property color bar: dark ? "#222222" : "#e6e9eb"
    readonly property color panel: dark ? "#1a1a1a" : "#ffffff"
    readonly property color card: dark ? "#1a1a1a" : "#ffffff"
    readonly property color sliderRest: dark ? Qt.rgba(accent.r * 0.38, accent.g * 0.38, accent.b * 0.38, 1) : Qt.rgba(accent.r * 0.25 + 0.65, accent.g * 0.25 + 0.65, accent.b * 0.25 + 0.65, 1)
    // Text on a filled accent surface.
    readonly property color accentText: dark ? "#101010" : "#ffffff"
    readonly property color chrome: dark ? "#000000" : "#dde1e3"
    readonly property color panelRaised: dark ? "#2d2d2d" : "#e9edef"
    readonly property color line: dark ? "#000000" : "#cfd5d8"
    readonly property color border: dark ? "#5d5d5d" : "#8d979c"
    readonly property color borderSoft: dark ? "#383838" : "#d3d9dc"
    readonly property color text: dark ? "#d3d3d3" : "#2b3135"
    readonly property color textBright: dark ? "#ffffff" : "#000000"
    readonly property color textDim: dark ? "#8c8c8c" : "#5b666c"
    readonly property color textMuted: dark ? "#5d5d5d" : "#78838a"
    // Greyed switch / checkbox and hover surfaces shared by the Syn* controls.
    readonly property color inactive: dark ? "#4a4a4a" : "#b4bcc1"
    readonly property color knob: dark ? "#d3d3d3" : "#ffffff"
    readonly property color hoverFill: dark ? "#2d2d2d" : "#e2e7e9"
    readonly property color hoverTint: dark ? "#1affffff" : "#14000000"
    readonly property color navText: dark ? "#a0a0a0" : "#4a5358"
    // In the light scheme a pale accent gets darker so it stays readable as text on white.
    readonly property color userAccent: prefs.accent
    readonly property color accent: dark ? userAccent : Qt.hsla(userAccent.hslHue, userAccent.hslSaturation, Math.min(userAccent.hslLightness, 0.30), 1)
    readonly property color accentHover: Qt.darker(accent, 1.12)
    // Translucent accent for selected rows / chips.
    readonly property color accentTint: Qt.rgba(accent.r, accent.g, accent.b, 0.10)
    readonly property color accentTintMid: Qt.rgba(accent.r, accent.g, accent.b, 0.13)
    readonly property color accentTintStrong: Qt.rgba(accent.r, accent.g, accent.b, 0.20)
    readonly property color warn: dark ? "#fd8611" : "#b85a00"
    readonly property color danger: dark ? "#c8323c" : "#b3202b"
    // Backdrops behind dialogs / overlays, and subtle surfaces (added by the light-theme sweep).
    readonly property color scrim: dark ? "#b3000000" : "#80000000"
    readonly property color scrimSoft: dark ? "#a0000000" : "#40000000"
    readonly property color overlayPanel: dark ? "#e61a1a1a" : "#f2ffffff"
    readonly property color pressedFill: dark ? "#3a3a3a" : "#d3d9dc"
    readonly property color controlFill: dark ? "#2d2d2d" : "#e9edef"
    readonly property color controlBorder: dark ? "#707070" : "#8d979c"
    readonly property color controlBorderHover: dark ? "#8a8a8a" : "#5b666c"
    readonly property color stripe: dark ? "#0affffff" : "#08000000"
    readonly property color warnTint: dark ? "#22fd8611" : "#22b85a00"
    readonly property color dangerTint: dark ? "#22c8323c" : "#22b3202b"
    readonly property color okTint: dark ? "#1444d62c" : "#1a1f8a1c"
    readonly property color keyFace: dark ? "#1d1d1d" : "#eceff1"
    readonly property color keyEdge: dark ? "#3a3a3a" : "#b9c1c5"

    // Where the project lives (the one place this address is written in QML).
    readonly property string projectUrl: "https://github.com/JenAriDev/tsuba" // i18n-ok
    function docsUrl(name) {
        return projectUrl + "/blob/main/docs/" + name;
    }

    readonly property string font: "Roboto" // i18n-ok
    // Razer's title face is proprietary and not shipped; Qt falls back to Roboto / the system sans.
    readonly property string titleFont: "Roboto" // i18n-ok
    readonly property int fontSize: 14
    readonly property int small: 12
    readonly property int heading: 20
    readonly property int barHeight: 48
    readonly property int sidebarWidth: 220
}
