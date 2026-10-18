import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

// Lighting Studio: layers of effects on selected LEDs, rendered live by tsubad.
// Each device has its own layers; the "All devices" canvas holds layers that span devices
// (one effect laid out over keyboard and mouse placed together).
Rectangle {
    id: studio

    required property var backend
    required property string pid           // the keyboard the page opened with
    // Device being edited (each Chroma profile holds layers per device).
    property string editPid: pid
    readonly property bool editMouse: editPid === "00aa" && !combined
    readonly property var devicesHere: {
        const d = [{ pid: pid, name: i18n("Keyboard") }];
        if (JSON.parse(backend.devices || "[]").some(x => x.pid === "00aa")) {
            d.push({ pid: "00aa", name: i18n("Mouse") });
        }
        return d;
    }
    property var pack: packOf(editPid)
    function packOf(p) {
        const j = backend.device_pack(p);
        return j === "" ? null : JSON.parse(j);
    }
    function deviceName(p) {
        const d = devicesHere.find(x => x.pid === p);
        return d ? d.name : p;
    }
    // "All devices": the shared canvas, keyboard and mouse placed together; layers there span them.
    property bool combined: false
    readonly property var kbPack: packOf(pid)
    readonly property var mousePack: packOf("00aa")
    readonly property bool hasMouse: devicesHere.length > 1
    readonly property var kbShown: combined ? kbPack : (editPid === "00aa" ? null : pack)
    readonly property var mouseShown: combined ? mousePack : (editPid === "00aa" ? pack : null)
    property var liveByPid: ({})
    readonly property var tabNames: devicesHere.map(d => d.name).concat(hasMouse ? [i18n("All devices")] : [])
    readonly property var mouseLeds: [{ led: [0, 0], name: i18n("Logo") }, { led: [0, 1], name: i18n("Scroll wheel") }].concat(Array.from({ length: 11 }, (_, i) => ({ led: [0, 2 + i], name: i18n("Underglow") })))
    readonly property var kbLeds: kbPack ? kbPack.views.top.hotspots.filter(h => h.led !== null) : []
    readonly property var leds: ledsOf(editPid)
    function ledsOf(p) {
        if (p === "00aa") {
            return mouseLeds;
        }
        if (p === pid) {
            return kbLeds;
        }
        return pack && editPid === p ? pack.views.top.hotspots.filter(h => h.led !== null) : [];
    }
    // Underglow runs 2 (front-left) → 7 (back) → 12 (front-right).
    readonly property var mouseGroups: [
        { key: "all", label: i18n("All"), test: h => true },
        { key: "logo", label: i18n("Logo"), test: h => h.led[1] === 0 },
        { key: "wheel", label: i18n("Scroll wheel"), test: h => h.led[1] === 1 },
        { key: "underglow", label: i18n("All underglow"), test: h => h.led[1] >= 2 },
        { key: "left", label: i18n("Left side"), test: h => h.led[1] >= 2 && h.led[1] <= 6 },
        { key: "back", label: i18n("Back"), test: h => h.led[1] >= 6 && h.led[1] <= 8 },
        { key: "right", label: i18n("Right side"), test: h => h.led[1] >= 8 },
        { key: "logowheel", label: i18n("Logo + wheel"), test: h => h.led[1] <= 1 }
    ]

    // Named Chroma profiles.
    property var profile: ({ name: "My Lighting", devices: {} })
    property string loadedName: ""
    property var profileNames: []
    property string activeName: ""
    property string status: ""
    property bool statusBad: false
    function say(text, bad) {
        status = text;
        statusBad = !!bad;
    }
    property var liveColours: ({})

    // Canvas: where the devices sit (stage units: the stage is 100 wide, 50 tall) and the layers
    // that span them. While the canvas is being edited, `layers` is the canvas layer list.
    readonly property real stageW: 100
    readonly property real stageH: 50
    property var placements: ({})
    property var canvasLayers: []
    property bool arranging: false
    property bool arranged: false      // the user moved something: save the placements

    // Model: list of layers, index 0 = bottom. The UI lists them top-first.
    property var layers: []
    property int current: -1
    property bool dirty: false
    property int editingColour: 0
    property bool cleared: false        // selection shown empty until the user picks LEDs

    // Undo / redo: snapshots (JSON) of `layers`.
    property var history: []
    property int hpos: -1
    property double lastSnap: 0
    readonly property bool canUndo: hpos > 0
    readonly property bool canRedo: hpos >= 0 && hpos < history.length - 1
    function resetHistory() {
        history = [JSON.stringify(layers)];
        hpos = 0;
    }
    function snapshot() {
        const now = Date.now();
        const h = history.slice(0, hpos + 1);
        const js = JSON.stringify(layers);
        // Drags (sliders, colour area) fire many edits: merge those within 400 ms.
        if (h.length > 1 && now - lastSnap < 400) {
            h[h.length - 1] = js;
        } else if (h[h.length - 1] !== js) {
            h.push(js);
        }
        lastSnap = now;
        history = h.slice(-100);
        hpos = history.length - 1;
    }
    function restore(i) {
        hpos = i;
        layers = JSON.parse(history[i]);
        current = Math.min(current, layers.length - 1);
        cleared = false;
        dirty = true;
        push.restart();
        restartPreview();
    }
    function undo() {
        if (canUndo) {
            restore(hpos - 1);
        }
    }
    function redo() {
        if (canRedo) {
            restore(hpos + 1);
        }
    }
    onCurrentChanged: {
        cleared = false;
        editingColour = 0;
    }

    // Effects, in the order of the ADD EFFECT grid. `group` = its heading there.
    //   colours: how many colour slots it starts with (0 = none); stops = any number of colours
    //   (a gradient); random = may pick colours at random / rainbow when it has none.
    readonly property var effects: [
        { type: "static", label: i18n("Static"), group: "colour", colours: 1 },
        { type: "gradient", label: i18n("Gradient"), group: "colour", colours: 2, stops: true },
        { type: "breathing", label: i18n("Breathing"), group: "colour", colours: 2, random: true, stops: true },
        { type: "spectrum", label: i18n("Spectrum Cycling"), group: "colour", colours: 0 },
        { type: "wave", label: i18n("Wave"), group: "motion", colours: 2, random: true, stops: true, directions: [i18n("Left to right"), i18n("Right to left"), i18n("Top to bottom"), i18n("Bottom to top")] },
        { type: "wheel", label: i18n("Wheel"), group: "motion", colours: 0, random: true, stops: true, directions: [i18n("Clockwise"), i18n("Counter-clockwise")] },
        { type: "tidal", label: i18n("Tidal"), group: "motion", colours: 0, random: true, stops: true, directions: [i18n("Outward"), i18n("Inward")] },
        { type: "starlight", label: i18n("Starlight"), group: "motion", colours: 2, random: true, stops: true, density: true },
        { type: "fire", label: i18n("Fire"), group: "motion", colours: 0 },
        { type: "audio_meter", label: i18n("Audio Meter"), group: "live", colours: 0 },
        { type: "ambient", label: i18n("Ambient (screen)"), group: "live", colours: 0 },
        { type: "battery", label: i18n("Battery level"), group: "live", colours: 0 },
        { type: "image", label: i18n("Image"), group: "live", colours: 0 }
    ]
    readonly property var effectGroups: [{ key: "colour", title: i18n("Colour") }, { key: "motion", title: i18n("Motion") }, { key: "live", title: i18n("Live") }]
    readonly property var dirKeys: ({
        wave: ["left_to_right", "right_to_left", "top_to_bottom", "bottom_to_top"],
        wheel: ["clockwise", "counter_clockwise"]
    })
    readonly property var groups: [
        { key: "all", label: i18n("All"), test: h => true },
        { key: "wasd", label: i18n("WASD"), test: h => (h.group || "").indexOf("wasd") >= 0 },
        { key: "arrows", label: i18n("Arrows"), test: h => (h.group || "").indexOf("arrows") >= 0 },
        { key: "letters", label: i18n("Letters"), test: h => (h.group || "").indexOf("alphabets") >= 0 },
        { key: "numbers", label: i18n("Number row"), test: h => (h.group || "").indexOf("numberRows") >= 0 },
        { key: "fkeys", label: i18n("F-keys"), test: h => (h.group || "").indexOf("functions") >= 0 },
        { key: "numpad", label: i18n("Numpad"), test: h => (h.group || "").indexOf("numpad") >= 0 },
        { key: "macro", label: i18n("Macro"), test: h => h.group === "macro" },
        { key: "media", label: i18n("Media"), test: h => (h.group || "").indexOf("multimedia") >= 0 },
        { key: "underglow", label: i18n("Underglow"), test: h => h.group === "underglow" }
    ]
    function groupsFor(p) {
        return p === "00aa" ? mouseGroups : groups;
    }
    // Quick-select entries: the edited device's groups, or (canvas) whole devices and their groups.
    // `pid` "*" = every device; `key` = the group (see `groups`).
    readonly property var quickEntries: {
        if (!combined) {
            return groupsFor(editPid).map(g => ({ text: g.label, pid: editPid, key: g.key }));
        }
        const out = [{ text: i18n("All devices"), pid: "*", key: "all" }];
        devicesHere.forEach(d => {
            out.push({ text: i18n("%1: all", d.name), pid: d.pid, key: "all" });
            groupsFor(d.pid).filter(g => g.key !== "all").forEach(g => out.push({ text: i18n("%1: %2", d.name, g.label), pid: d.pid, key: g.key }));
        });
        return out;
    }

    color: Theme.bg

    // Colour stops drawn as a left-to-right gradient bar.
    component GradBar: Canvas {
        id: gb
        property var stops: []
        onStopsChanged: requestPaint()
        onWidthChanged: requestPaint()
        onPaint: {
            const c = getContext("2d");
            c.reset();
            if (stops.length < 2) {
                return;
            }
            const g = c.createLinearGradient(0, 0, width, 0);
            stops.forEach((s, i) => g.addColorStop(i / (stops.length - 1), "#" + s));
            c.fillStyle = g;
            c.fillRect(0, 0, width, height);
        }
    }

    // A tiny picture (w×h, RRGGBB hex pixels) drawn pixel by pixel, keeping its proportions.
    component PixThumb: Canvas {
        id: pt
        property int w: 0
        property int h: 0
        property string pixels: ""
        onPixelsChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const c = getContext("2d");
            c.reset();
            c.fillStyle = "#202020";
            c.fillRect(0, 0, width, height);
            if (w <= 0 || h <= 0 || pixels.length !== w * h * 6) {
                return;
            }
            const k = Math.min(width / w, height / h);
            const ox = (width - w * k) / 2, oy = (height - h * k) / 2;
            for (let y = 0; y < h; y++) {
                for (let x = 0; x < w; x++) {
                    const i = (y * w + x) * 6;
                    c.fillStyle = "#" + pixels.substr(i, 6);
                    c.fillRect(ox + x * k, oy + y * k, Math.ceil(k), Math.ceil(k));
                }
            }
        }
    }

    // Small square toolbar button with a hand-drawn glyph.
    component ToolBtn: Rectangle {
        id: tb
        property string kind: "all"
        property string tip: ""
        signal clicked
        implicitWidth: 28
        implicitHeight: 24
        width: 28
        height: 24
        radius: 3
        color: th.hovered && enabled ? "#26ffffff" : "transparent"
        border.color: th.hovered && enabled ? Theme.accent : Theme.borderSoft
        opacity: enabled ? 1 : 0.35
        readonly property color ink: th.hovered && enabled ? Theme.accent : Theme.text
        onInkChanged: cv.requestPaint()
        Canvas {
            id: cv
            anchors.centerIn: parent
            width: 16
            height: 16
            onPaint: {
                const c = getContext("2d");
                c.reset();
                c.strokeStyle = tb.ink;
                c.lineWidth = 1.5;
                c.lineCap = "round";
                c.lineJoin = "round";
                const ln = (a, b, d, e) => {
                    c.beginPath();
                    c.moveTo(a, b);
                    c.lineTo(d, e);
                    c.stroke();
                };
                switch (tb.kind) {
                case "all":      // dashed selection box with a tick
                    c.setLineDash([2, 2]);
                    c.strokeRect(2, 2, 12, 12);
                    c.setLineDash([]);
                    ln(5, 8, 7, 10);
                    ln(7, 10, 11, 5.5);
                    break;
                case "clear":    // dashed box with a cross
                    c.setLineDash([2, 2]);
                    c.strokeRect(2, 2, 12, 12);
                    c.setLineDash([]);
                    ln(5.5, 5.5, 10.5, 10.5);
                    ln(10.5, 5.5, 5.5, 10.5);
                    break;
                case "undo":
                case "redo": {
                    const f = tb.kind === "redo" ? -1 : 1;
                    const X = x => f === 1 ? x : 16 - x;
                    c.beginPath();
                    c.moveTo(X(4), 6);
                    c.lineTo(X(10), 6);
                    c.bezierCurveTo(X(14), 6, X(14), 13, X(10), 13);
                    c.lineTo(X(6), 13);
                    c.stroke();
                    ln(X(4), 6, X(7), 3);
                    ln(X(4), 6, X(7), 9);
                    break;
                }
                case "minus":
                    ln(4, 8, 12, 8);
                    break;
                case "plus":
                    ln(4, 8, 12, 8);
                    ln(8, 4, 8, 12);
                    break;
                }
            }
        }
        HoverHandler {
            id: th
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            enabled: tb.enabled
            onTapped: tb.clicked()
        }
        QQC2.ToolTip.visible: th.hovered && tip !== ""
        QQC2.ToolTip.text: tip
        QQC2.ToolTip.delay: 500
    }

    function effectInfo(type) {
        return effects.find(e => e.type === type) || effects[0];
    }
    function newEffect(type) {
        switch (type) {
        case "static":
            return { type: type, colour: "44d62c" };
        case "gradient":
            return { type: type, colours: ["44d62c", "0080ff"], angle: 0, scroll: false, duration: "medium" };
        case "breathing":
            return { type: type, colours: ["44d62c"], duration: "medium" };
        case "spectrum":
            return { type: type, duration: "medium" };
        case "wave":
            return { type: type, direction: "left_to_right", duration: "medium", colours: [] };
        case "wheel":
            return { type: type, direction: "clockwise", duration: "medium", centre: [0.5, 0.5], spread: 1, colours: [] };
        case "starlight":
            return { type: type, colours: [], duration: "medium", density: 0.15 };
        case "audio_meter":
            return { type: type, mode: "spectrum", peak_hold: false };
        case "ambient":
            return { type: type, boost: 1.3 };
        case "tidal":
            return { type: type, inward: false, duration: "medium", colours: [] };
        case "image":
            return { type: type, w: 0, h: 0, data: "", fit: "cover", scroll: "none" };
        case "battery":
            return { type: type };
        default:
            return { type: type, duration: "medium" };
        }
    }
    function newLayer(type) {
        const l = { name: effectInfo(type).label, enabled: true, leds: [], effect: newEffect(type), brightness: 1, opacity: 1, speed: 1 };
        if (combined) {
            // A canvas layer starts on every device.
            l.targets = {};
            devicesHere.forEach(d => l.targets[d.pid] = []);
        }
        return l;
    }
    function addLayer(type) {
        const l = layers.slice();
        l.push(newLayer(type));
        layers = l;
        current = l.length - 1;
        changed();
    }
    function duplicateLayer(i) {
        if (i < 0 || i >= layers.length) {
            return;
        }
        const l = layers.slice();
        const c = JSON.parse(JSON.stringify(layers[i]));
        c.name = c.name + " copy";
        l.splice(i + 1, 0, c);
        layers = l;
        current = i + 1;
        changed();
    }
    // Back to the effect's defaults (keeps the layer's name, LEDs, brightness, opacity, speed).
    function resetEffect() {
        updateCurrent(l => l.effect = newEffect(l.effect.type));
    }
    function updateCurrent(fn) {
        if (current < 0) {
            return;
        }
        const l = JSON.parse(JSON.stringify(layers));
        fn(l[current]);
        layers = l;
        changed();
    }
    function moveLayer(i, d) {
        const j = i + d;
        if (j < 0 || j >= layers.length) {
            return;
        }
        const l = layers.slice();
        [l[i], l[j]] = [l[j], l[i]];
        layers = l;
        current = j;
        changed();
    }
    function removeLayer(i) {
        const l = layers.slice();
        l.splice(i, 1);
        layers = l;
        current = Math.min(current, l.length - 1);
        changed();
    }
    function changed() {
        snapshot();
        dirty = true;
        push.restart();
        restartPreview();
    }

    // ---- The profile as the daemon sees it
    // The whole profile with the layers being edited folded in.
    function currentProfile() {
        const p = JSON.parse(JSON.stringify(profile));
        p.devices = p.devices || {};
        if (combined) {
            p.canvas = { placements: placements, layers: JSON.parse(JSON.stringify(layers)) };
        } else {
            p.devices[editPid] = { layers: JSON.parse(JSON.stringify(layers)) };
            if (canvasLayers.length || arranged) {
                p.canvas = { placements: placements, layers: JSON.parse(JSON.stringify(canvasLayers)) };
            } else {
                delete p.canvas;
            }
        }
        return p;
    }
    // What each device runs for the profile being edited: { pid: { layers } }.
    function expanded() {
        if (typeof backend.studio_call !== "function") {
            const m = {};
            m[editPid] = { layers: layers };
            return m;
        }
        return JSON.parse(backend.studio_call("expand", JSON.stringify(currentProfile())) || "{}");
    }
    // Live preview of every device (same renderer code as the device output), then the edited
    // one last so preview_frame() keeps meaning "the edited device".
    function restartPreview() {
        const ex = expanded();
        const order = devicesHere.filter(d => d.pid !== editPid).concat(devicesHere.filter(d => d.pid === editPid));
        order.forEach(d => backend.preview_start(d.pid, JSON.stringify(ex[d.pid] || { layers: [] })));
    }
    function startOtherPreviews() {
        restartPreview();
    }
    // Send the profile being edited to the devices (only those whose layers changed).
    property var pushed: ({})
    function pushLive() {
        const ex = expanded();
        devicesHere.forEach(d => {
            const st = ex[d.pid];
            if (!st || !st.layers.length) {
                return;
            }
            const js = JSON.stringify({ type: "studio", studio: st });
            if (pushed[d.pid] !== js) {
                pushed[d.pid] = js;
                backend.set_effect(d.pid, js, 255);
            }
        });
    }

    // "all LEDs" / "12 LEDs" / "Keyboard + Mouse" for a layer row.
    function coverText(l) {
        if (l.targets) {
            const names = Object.keys(l.targets).map(deviceName);
            return names.length ? names.join(" + ") : i18n("no device");
        }
        return l.leds.length ? i18np("%1 LED", "%1 LEDs", l.leds.length) : i18n("all LEDs");
    }

    // ---- LED selection
    function selectionFor(p) {
        const s = {};
        if (cleared || current < 0 || !layers[current]) {
            return s;
        }
        const l = layers[current];
        let list;
        if (combined) {
            const t = l.targets ? l.targets[p] : undefined;
            if (t === undefined) {
                return s;
            }
            list = t.length ? t : ledsOf(p).map(h => h.led);
        } else {
            if (p !== editPid) {
                return s;
            }
            list = l.leds.length ? l.leds : leds.map(h => h.led);
        }
        list.forEach(rc => s[rc[0] + "," + rc[1]] = true);
        return s;
    }
    readonly property var selection: selectionFor(editPid)
    // LEDs picked on one device's view: the layer's LEDs there (canvas: its targets).
    function pickLeds(devPid, sel) {
        cleared = false;
        const list = Object.keys(sel).map(k => k.split(",").map(Number));
        if (combined) {
            updateCurrent(l => {
                l.targets = l.targets || {};
                if (list.length) {
                    l.targets[devPid] = list;
                } else {
                    delete l.targets[devPid];
                }
            });
            return;
        }
        if (devPid !== editPid) {
            switchDevice(devPid);
        }
        updateCurrent(l => l.leds = list);
    }
    // Choose LEDs by group: `key` of `groups` on the edited device, or (canvas) on `devPid`
    // ("*" = every device, whole).
    function applyGroup(key, devPid) {
        cleared = false;
        if (combined) {
            const t = {};
            if (devPid === "*") {
                devicesHere.forEach(d => t[d.pid] = []);
            } else {
                if (!devicesHere.some(d => d.pid === devPid)) {
                    return;
                }
                if (key === "all") {
                    t[devPid] = [];
                } else {
                    const grp = groupsFor(devPid).find(x => x.key === key);
                    if (!grp) {
                        return;
                    }
                    t[devPid] = ledsOf(devPid).filter(grp.test).map(h => h.led);
                }
            }
            updateCurrent(l => l.targets = t);
            return;
        }
        const grp = groupsFor(editPid).find(x => x.key === key);
        if (!grp) {
            return;
        }
        updateCurrent(l => l.leds = key === "all" ? [] : leds.filter(grp.test).map(h => h.led));
    }
    function applyQuick(e) {
        applyGroup(e.key, e.pid);
    }
    // Devices a canvas layer covers (toggle one on or off; at least one stays).
    function toggleTarget(devPid) {
        updateCurrent(l => {
            l.targets = l.targets || {};
            if (l.targets[devPid] !== undefined) {
                if (Object.keys(l.targets).length > 1) {
                    delete l.targets[devPid];
                }
            } else {
                l.targets[devPid] = [];
            }
        });
    }
    // Make the current device layer span devices (it moves onto the canvas, on top).
    function spanCurrent() {
        if (combined || current < 0 || !hasMouse) {
            return;
        }
        const l = JSON.parse(JSON.stringify(layers[current]));
        const t = {};
        t[editPid] = l.leds || [];
        devicesHere.forEach(d => {
            if (d.pid !== editPid) {
                t[d.pid] = [];
            }
        });
        l.leds = [];
        l.targets = t;
        const rest = layers.slice();
        rest.splice(current, 1);
        layers = rest;
        current = Math.min(current, rest.length - 1);
        canvasLayers = canvasLayers.concat([l]);
        changed();
        setMode(true, "");
        current = layers.length - 1;
    }
    // Move a canvas layer that covers one device back onto that device's own layers.
    function unspanCurrent() {
        if (!combined || current < 0) {
            return;
        }
        const l = JSON.parse(JSON.stringify(layers[current]));
        const ps = Object.keys(l.targets || {});
        if (ps.length !== 1) {
            return;
        }
        const target = ps[0];
        l.leds = l.targets[target];
        delete l.targets;
        const rest = layers.slice();
        rest.splice(current, 1);
        layers = rest;
        current = Math.min(current, rest.length - 1);
        canvasLayers = rest;
        const d = Object.assign({}, profile.devices);
        d[target] = { layers: ((d[target] && d[target].layers) || []).concat([l]) };
        profile = Object.assign({}, profile, { devices: d });
        changed();
    }

    // ---- Devices on the canvas
    // Aspect (width / height) of the box round a device's LED centres: what a placement keeps.
    function ledAspect(p) {
        const v = p === "00aa" ? mouseView : kbView;
        if (v && v.view) {
            return Math.max(0.2, v.ledsPic.width / v.ledsPic.height);
        }
        return p === "00aa" ? 0.8 : 3.3;
    }
    readonly property real kbAspect: ledAspect(pid)
    readonly property real mouseAspect: ledAspect("00aa")
    // The pictures load after the page: lay the devices out again once their shapes are known.
    onKbAspectChanged: if (!arranged) placements = defaultPlacements()
    onMouseAspectChanged: if (!arranged) placements = defaultPlacements()
    function defaultPlacements() {
        // Keyboard on the left, mouse to its right, bottoms level; sized from the LED boxes.
        const ka = ledAspect(pid), ma = ledAspect("00aa");
        const gap = 6;
        let mh = 36, mw = mh * ma;
        let kw = 92 - mw - gap, kh = kw / ka;
        if (kh > 30) {
            kh = 30;
            kw = kh * ka;
        }
        const x0 = (stageW - (kw + gap + mw)) / 2, bottom = (stageH + Math.max(kh, mh)) / 2;
        const m = {};
        m[pid] = { x: x0, y: bottom - kh, w: kw, h: kh };
        m["00aa"] = { x: x0 + kw + gap, y: bottom - mh, w: mw, h: mh };
        return m;
    }
    function placementOf(p) {
        return placements[p] || { x: 0, y: 0, w: 40, h: 20 };
    }
    function movePlacement(p, x, y) {
        const pl = Object.assign({}, placementOf(p));
        pl.x = Math.max(0, Math.min(stageW - pl.w, x));
        pl.y = Math.max(0, Math.min(stageH - pl.h, y));
        const m = Object.assign({}, placements);
        m[p] = pl;
        placements = m;
        arranged = true;
        dirty = true;
        push.restart();
        restartPreview();
    }
    // Resize keeping the centre; factor relative to the current size.
    function scalePlacement(p, f) {
        const pl = Object.assign({}, placementOf(p));
        const cx = pl.x + pl.w / 2, cy = pl.y + pl.h / 2;
        // One factor for both sides: a placement keeps the shape of the device's LED box.
        f = Math.max(Math.max(6 / pl.w, 3 / pl.h), Math.min(f, stageW / pl.w, stageH / pl.h));
        pl.w *= f;
        pl.h *= f;
        pl.x = Math.max(0, Math.min(stageW - pl.w, cx - pl.w / 2));
        pl.y = Math.max(0, Math.min(stageH - pl.h, cy - pl.h / 2));
        const m = Object.assign({}, placements);
        m[p] = pl;
        placements = m;
        arranged = true;
        dirty = true;
        push.restart();
        restartPreview();
    }
    function resetPlacements() {
        placements = defaultPlacements();
        arranged = false;
        dirty = true;
        push.restart();
        restartPreview();
    }

    // ---- Editing a device or the canvas
    // Fold the layers being edited back into the profile.
    function stash() {
        if (combined) {
            canvasLayers = JSON.parse(JSON.stringify(layers));
        } else {
            const d = Object.assign({}, profile.devices);
            d[editPid] = { layers: layers };
            profile = Object.assign({}, profile, { devices: d });
        }
    }
    // Edit one device (canvas off) or the shared canvas (on).
    function setMode(canvasOn, devPid) {
        stash();
        combined = canvasOn && hasMouse;
        if (combined) {
            layers = JSON.parse(JSON.stringify(canvasLayers));
        } else {
            if (devPid) {
                editPid = devPid;
            }
            const st = profile.devices[editPid];
            layers = st ? JSON.parse(JSON.stringify(st.layers)) : [];
        }
        current = layers.length - 1;
        resetHistory();
        restartPreview();
    }
    function switchDevice(p) {
        if (p === editPid && !combined) {
            return;
        }
        setMode(false, p);
    }
    function showCombined(on) {
        if (on === combined) {
            return;
        }
        setMode(on, "");
    }
    function openProfile(p) {
        profile = JSON.parse(JSON.stringify(p));
        loadedName = p.name;
        const cv = profile.canvas || {};
        canvasLayers = cv.layers || [];
        const dp = defaultPlacements();
        const pl = cv.placements || {};
        placements = Object.keys(pl).length ? Object.assign(dp, pl) : dp;
        arranged = Object.keys(pl).length > 0;
        if (combined) {
            layers = JSON.parse(JSON.stringify(canvasLayers));
        } else {
            const st = profile.devices[editPid];
            layers = st ? JSON.parse(JSON.stringify(st.layers)) : [];
        }
        current = layers.length - 1;
        dirty = false;
        pushed = {};
        resetHistory();
        restartPreview();
    }
    function loadProfiles() {
        const j = JSON.parse(backend.chroma_profiles() || "{\"profiles\": []}");
        profileNames = j.profiles.map(p => p.name);
        activeName = j.active || "";
        return j;
    }
    function saveProfile(apply) {
        const toSave = currentProfile();
        if (loadedName !== "" && loadedName !== profile.name && profileNames.indexOf(loadedName) >= 0) {
            if (!backend.rename_chroma_profile(loadedName, profile.name)) {
                say(i18n("Failed: %1", backend.status), true);
                return;
            }
        }
        stash();
        if (backend.save_chroma_profile(JSON.stringify(toSave), apply)) {
            loadedName = profile.name;
            dirty = false;
            say(apply ? i18n("Saved and applied") : i18n("Saved"), false);
        } else {
            say(i18n("Failed: %1", backend.status), true);
        }
        loadProfiles();
        Theme.profilesRev++;
    }
    function uniqueName(base) {
        let n = base, i = 2;
        while (profileNames.indexOf(n) >= 0) {
            n = base + " " + i++;
        }
        return n;
    }
    function studioJson() {
        return JSON.stringify({ type: "studio", studio: { layers: layers } });
    }
    // Profile files (Tsuba's own JSON format).
    function exportProfile(url) {
        const r = JSON.parse(backend.studio_call("export", JSON.stringify({ path: url.toString(), profile: currentProfile() })));
        if (r.ok) {
            say(i18n("Exported to %1", r.path), false);
        } else {
            say(i18n("Failed: %1", r.error), true);
        }
    }
    function importProfile(url) {
        const r = JSON.parse(backend.studio_call("import", JSON.stringify({ path: url.toString(), taken: profileNames.concat([profile.name]) })));
        if (!r.ok) {
            say(i18n("Failed: %1", r.error), true);
            return;
        }
        openProfile(r.profile);
        loadedName = "";
        dirty = true;
        say(i18n("Imported as %1 (not saved yet)", r.profile.name), false);
    }
    function editedColour() {
        const l = current >= 0 ? layers[current] : null;
        if (!l) {
            return "";
        }
        if (l.effect.type === "static") {
            return l.effect.colour || "44d62c";
        }
        const c = l.effect.colours || [];
        return c[Math.min(editingColour, c.length - 1)] || "";
    }
    function setColour(hex) {
        updateCurrent(l => {
            if (l.effect.type === "static") {
                l.effect.colour = hex;
            } else {
                const c = (l.effect.colours || []).slice();
                if (c.length === 0) {
                    return;
                }
                c[Math.min(editingColour, c.length - 1)] = hex;
                l.effect.colours = c;
            }
        });
    }
    // Gradient stops: add one after the selected stop (a blend of it and the next), or remove it.
    function addStop() {
        updateCurrent(l => {
            const c = (l.effect.colours || []).slice();
            if (c.length >= 8) {
                return;
            }
            const i = Math.min(editingColour, c.length - 1);
            const a = c.length ? c[i] : "44d62c", b = c.length ? c[(i + 1) % c.length] : "0080ff";
            const mix = k => Math.round((parseInt(a.substr(k, 2), 16) + parseInt(b.substr(k, 2), 16)) / 2).toString(16).padStart(2, "0");
            c.splice(i + 1, 0, c.length < 2 ? "0080ff" : mix(0) + mix(2) + mix(4));
            l.effect.colours = c;
            editingColour = i + 1;
        });
    }
    function removeStop() {
        updateCurrent(l => {
            const c = (l.effect.colours || []).slice();
            const min = l.effect.type === "gradient" ? 2 : 1;
            if (c.length <= min) {
                return;
            }
            c.splice(Math.min(editingColour, c.length - 1), 1);
            l.effect.colours = c;
            editingColour = Math.max(0, Math.min(editingColour, c.length - 1));
        });
    }
    function swatch(e) {
        if (e.colour) {
            return "#" + e.colour;
        }
        if (e.colours && e.colours.length) {
            return "#" + e.colours[0];
        }
        return e.type === "fire" ? "#ff7a1a" : "transparent";
    }
    // Hex pixels (RRGGBB each) for a w×h picture sampled from the loaded image item.
    function pixelsHex(imageData, w, h) {
        let out = "";
        for (let i = 0; i < w * h; i++) {
            const a = imageData.data[i * 4 + 3] / 255;
            for (let k = 0; k < 3; k++) {
                out += Math.round(imageData.data[i * 4 + k] * a).toString(16).padStart(2, "0");
            }
        }
        return out;
    }

    function load() {
        const j = loadProfiles();
        const act = j.profiles.find(p => p.name === j.active) || j.profiles[0];
        if (act) {
            openProfile(act);
            return;
        }
        const cfg = JSON.parse(backend.config(pid) || "{}");
        const e = cfg.lighting ? cfg.lighting.effect : null;
        if (e && e.type === "studio") {
            layers = e.studio.layers;
            current = layers.length - 1;
        } else if (backend.env_var("TSUBA_DEMO") === "1") {
            const sel = g => leds.filter(g.test).map(h => h.led);
            layers = [
                { name: i18n("Wave"), enabled: true, leds: [], effect: newEffect("wave"), brightness: 1, opacity: 1, speed: 1 },
                { name: "Underglow stars", enabled: true, leds: sel(groups[groups.length - 1]), effect: { type: "starlight", colours: ["ffffff"], duration: "medium", density: 0.2 }, brightness: 1, opacity: 1, speed: 1 },
                { name: "WASD", enabled: true, leds: sel(groups[1]), effect: { type: "static", colour: "ff2020" }, brightness: 1, opacity: 1, speed: 1 }
            ];
            current = 2;
        }
        // First run: the device's current studio becomes the first profile.
        const devs = {};
        devs[pid] = { layers: layers };
        const mcfg = JSON.parse(backend.config("00aa") || "{}");
        if (mcfg.lighting && mcfg.lighting.effect && mcfg.lighting.effect.type === "studio") {
            devs["00aa"] = { layers: mcfg.lighting.effect.studio.layers };
        }
        profile = { name: "My Lighting", devices: devs };
        loadedName = "";
        dirty = layers.length > 0;
        placements = defaultPlacements();
        resetHistory();
        restartPreview();
    }

    Component.onCompleted: {
        placements = defaultPlacements();
        load();
        if (backend.env_var("TSUBA_SYNAPSE") !== "") {   // headless UI check
            synapseDlg.open("lighting");
        }
        if (backend.env_var("TSUBA_STUDIO_DEVICE") !== "") {   // headless UI check
            if (backend.env_var("TSUBA_STUDIO_DEVICE") === "all") {
                showCombined(true);
            } else {
                switchDevice(backend.env_var("TSUBA_STUDIO_DEVICE"));
            }
        }
        if (backend.env_var("TSUBA_DEMO_EFFECT") !== "" && current >= 0) {   // headless UI check
            const fx = newEffect(backend.env_var("TSUBA_DEMO_EFFECT"));
            updateCurrent(l => l.effect = fx);
        }
        if (backend.env_var("TSUBA_DEMO_CANVAS") !== "" && hasMouse) {   // headless UI check
            showCombined(true);
            addLayer("wave");
            updateCurrent(l => l.effect.colours = ["ff2020", "ffd200", "44d62c", "0080ff"]);
            arranging = backend.env_var("TSUBA_DEMO_CANVAS") === "arrange";
        }
    }

    Timer {
        id: push
        interval: 150
        onTriggered: if (!studio.backend.readonly && (studio.layers.length || studio.canvasLayers.length)) studio.pushLive()
    }

    // Live animated preview from Tsuba's own renderer (same code as the device output).
    Timer {
        interval: 40
        repeat: true
        running: studio.visible
        onTriggered: {
            const m = {};
            studio.devicesHere.forEach(d => m[d.pid] = JSON.parse(studio.backend.preview_frame_for(d.pid) || "{}"));
            studio.liveByPid = m;
            studio.liveColours = m[studio.editPid] || {};
        }
    }

    Shortcut {
        sequence: "Ctrl+Z"
        enabled: studio.visible
        onActivated: studio.undo()
    }
    Shortcut {
        sequences: ["Ctrl+Shift+Z", "Ctrl+Y"]
        enabled: studio.visible
        onActivated: studio.redo()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            color: Theme.bg
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 28
                anchors.rightMargin: 24
                spacing: 14
                Text {
                    text: i18n("LIGHTING STUDIO")
                    color: Theme.accent
                    font.family: Theme.titleFont
                    font.pixelSize: 16
                }
                // Profile: pick / name / new / duplicate / delete / save & apply
                SynDropdown {
                    Layout.preferredWidth: 200
                    model: studio.profileNames.length ? studio.profileNames : [studio.profile.name]
                    currentIndex: Math.max(0, studio.profileNames.indexOf(studio.loadedName))
                    onActivated: i => {
                        const j = studio.loadProfiles();
                        const p = j.profiles.find(x => x.name === studio.profileNames[i]);
                        if (p) {
                            studio.openProfile(p);
                            // Picking a profile switches the devices to it (like Synapse).
                            if (!studio.backend.readonly) {
                                if (studio.backend.apply_chroma_profile(p.name)) {
                                    studio.say(i18n("Applied %1", p.name), false);
                                } else {
                                    studio.say(i18n("Failed: %1", studio.backend.status), true);
                                }
                                studio.loadProfiles();
                            }
                        }
                    }
                }
                QQC2.TextField {
                    Layout.preferredWidth: 180
                    text: studio.profile.name
                    placeholderText: i18n("Profile name")
                    onTextEdited: {
                        studio.profile = Object.assign({}, studio.profile, { name: text });
                        studio.dirty = true;
                    }
                }
                LightingOwnerChip {
                    objectName: "ownerChip"
                    backend: studio.backend
                    pid: studio.editPid
                    visible: !studio.backend.readonly
                }
                Text {
                    visible: studio.loadedName !== "" && studio.loadedName === studio.activeName
                    text: i18n("ACTIVE")
                    color: Theme.accent
                    font.family: Theme.font
                    font.pixelSize: 10
                    font.letterSpacing: 1
                }
                SynButton {
                    objectName: "profileMenuButton"
                    text: "•••"
                    implicitWidth: 56
                    onClicked: profileMenu.open()
                    QQC2.Menu {
                        id: profileMenu
                        y: parent.height
                        QQC2.MenuItem {
                            objectName: "menuNew"
                            text: i18n("New profile")
                            onTriggered: {
                                studio.profile = { name: studio.uniqueName("Profile"), devices: {} };
                                studio.loadedName = "";
                                studio.layers = [];
                                studio.canvasLayers = [];
                                studio.arranged = false;
                                studio.current = -1;
                                studio.dirty = true;
                                studio.pushed = {};
                                studio.resetHistory();
                                studio.restartPreview();
                            }
                        }
                        QQC2.MenuItem {
                            objectName: "menuDuplicate"
                            text: i18n("Duplicate")
                            onTriggered: {
                                const p = studio.currentProfile();
                                studio.profile = Object.assign({}, p, { name: studio.uniqueName(studio.profile.name + " copy") });
                                studio.loadedName = "";
                                studio.dirty = true;
                            }
                        }
                        QQC2.MenuItem {
                            objectName: "menuExport"
                            text: i18n("Export to file…")
                            onTriggered: exportDlg.open()
                        }
                        QQC2.MenuItem {
                            objectName: "menuImport"
                            text: i18n("Import from file…")
                            onTriggered: importDlg.open()
                        }
                        QQC2.MenuItem {
                            objectName: "studioSynapseImport"
                            text: i18n("Import from Synapse…")
                            onTriggered: synapseDlg.open("lighting")
                        }
                        QQC2.MenuSeparator {}
                        QQC2.MenuItem {
                            objectName: "menuDelete"
                            text: i18n("Delete profile")
                            enabled: studio.loadedName !== "" && !studio.backend.readonly
                            onTriggered: {
                                studio.backend.delete_chroma_profile(studio.loadedName);
                                Theme.profilesRev++;
                                const j = studio.loadProfiles();
                                if (j.profiles.length) {
                                    studio.openProfile(j.profiles[0]);
                                } else {
                                    studio.profile = { name: "My Lighting", devices: {} };
                                    studio.loadedName = "";
                                    studio.layers = [];
                                    studio.canvasLayers = [];
                                    studio.resetHistory();
                                }
                            }
                        }
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
                Text {
                    text: studio.status !== "" ? studio.status : (studio.backend.readonly ? i18n("read-only daemon") : (studio.layers.length ? i18n("LIVE ON DEVICE") : ""))
                    color: studio.statusBad || studio.backend.readonly ? Theme.warn : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    font.letterSpacing: 1
                }
                SynButton {
                    text: i18n("Save & apply")
                    primary: studio.dirty
                    enabled: !studio.backend.readonly && studio.profile.name.trim() !== ""
                    onClicked: studio.saveProfile(true)
                }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 2
            color: Theme.line
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // ---- Layers
            Rectangle {
                Layout.preferredWidth: 270
                Layout.fillHeight: true
                color: Theme.panel
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 10
                    RowLayout {
                        Text {
                            text: i18n("EFFECT LAYERS")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            font.letterSpacing: 0.5
                        }
                        HelpButton {
                            Layout.leftMargin: 8
                            size: 16
                            text: i18n("A Lighting Studio profile is a stack of effect layers drawn on your devices' LEDs, top layer first. Add layers with +, choose which LEDs each one covers, and use Blend to decide how it mixes with the layers below. Profiles play on all devices while Tsuba runs.")
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        SynButton {
                            text: "+"
                            implicitWidth: 36
                            onClicked: addMenu.open()
                            QQC2.Menu {
                                id: addMenu
                                y: parent.height
                                Repeater {
                                    model: studio.effects
                                    delegate: QQC2.MenuItem {
                                        required property var modelData
                                        text: modelData.label
                                        onTriggered: studio.addLayer(modelData.type)
                                    }
                                }
                            }
                        }
                    }
                    ListView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 6
                        clip: true
                        // Show the top layer first.
                        model: studio.layers.length
                        delegate: Rectangle {
                            required property int index
                            readonly property int li: studio.layers.length - 1 - index
                            readonly property var lyr: studio.layers[li]
                            width: ListView.view.width
                            height: 52
                            color: studio.current === li ? Theme.panelRaised : (lh.hovered ? "#262626" : "transparent")
                            border.color: studio.current === li ? Theme.accent : "transparent"
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 6
                                spacing: 8
                                Rectangle {
                                    // visibility toggle
                                    Layout.preferredWidth: 18
                                    Layout.preferredHeight: 18
                                    radius: 3
                                    color: lyr && lyr.enabled ? Theme.accent : "transparent"
                                    border.color: Theme.border
                                    TapHandler {
                                        onTapped: {
                                            studio.current = li;
                                            studio.updateCurrent(l => l.enabled = !l.enabled);
                                        }
                                    }
                                }
                                Item {
                                    Layout.preferredWidth: 24
                                    Layout.preferredHeight: 24
                                    EffectIcon {
                                        anchors.centerIn: parent
                                        size: 22
                                        type: lyr ? lyr.effect.type : "static"
                                        active: studio.current === li
                                    }
                                    Rectangle {
                                        // colour dot
                                        visible: lyr && studio.swatch(lyr.effect) !== "transparent"
                                        x: 14
                                        y: 14
                                        width: 9
                                        height: 9
                                        radius: 5
                                        color: lyr ? studio.swatch(lyr.effect) : "transparent"
                                        border.color: Theme.panel
                                        border.width: 1
                                    }
                                }
                                Column {
                                    Layout.fillWidth: true
                                    Text {
                                        text: lyr ? lyr.name : ""
                                        color: studio.current === li ? Theme.accent : Theme.text
                                        font.family: Theme.font
                                        font.pixelSize: Theme.fontSize
                                        elide: Text.ElideRight
                                        width: parent.width
                                    }
                                    Text {
                                        text: lyr ? studio.effectInfo(lyr.effect.type).label + " · " + studio.coverText(lyr) : ""
                                        color: Theme.textDim
                                        font.family: Theme.font
                                        font.pixelSize: 11
                                    }
                                }
                                Column {
                                    Text {
                                        text: "▲"
                                        color: Theme.textDim
                                        font.pixelSize: 10
                                        TapHandler {
                                            onTapped: studio.moveLayer(li, 1)
                                        }
                                    }
                                    Text {
                                        text: "▼"
                                        color: Theme.textDim
                                        font.pixelSize: 10
                                        TapHandler {
                                            onTapped: studio.moveLayer(li, -1)
                                        }
                                    }
                                }
                                Text {
                                    text: "✕"
                                    color: Theme.textDim
                                    font.pixelSize: 13
                                    TapHandler {
                                        onTapped: studio.removeLayer(li)
                                    }
                                }
                            }
                            HoverHandler {
                                id: lh
                            }
                            TapHandler {
                                onTapped: studio.current = li
                            }
                        }
                    }
                    Text {
                        visible: studio.combined
                        Layout.fillWidth: true
                        text: i18n("Canvas layers span devices: one effect laid out over the keyboard and mouse placed together. They draw above each device's own layers.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                    Text {
                        visible: !studio.combined && studio.canvasLayers.length > 0
                        objectName: "canvasNote"
                        Layout.fillWidth: true
                        text: i18np("%1 canvas layer also lights this device (see All devices).", "%1 canvas layers also light this device (see All devices).", studio.canvasLayers.length)
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                    Text {
                        visible: studio.layers.length === 0
                        Layout.fillWidth: true
                        text: i18n("Add a layer to start. Upper layers draw over lower ones.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.topMargin: 6
                        text: i18n("ADD EFFECT")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.letterSpacing: 0.5
                    }
                    Flickable {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(contentHeight, 330)
                        contentWidth: width
                        contentHeight: addCol.implicitHeight
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                        Column {
                            id: addCol
                            width: parent.width
                            spacing: 4
                            Repeater {
                                model: studio.effectGroups
                                delegate: Column {
                                    required property var modelData
                                    readonly property string groupKey: modelData.key
                                    width: addCol.width
                                    spacing: 6
                                    Text {
                                        text: modelData.title.toUpperCase()
                                        color: Theme.textMuted
                                        font.family: Theme.font
                                        font.pixelSize: 10
                                        font.letterSpacing: 1
                                    }
                                    Grid {
                                        columns: 3
                                        rowSpacing: 6
                                        width: parent.width
                                        Repeater {
                                            model: studio.effects.filter(e => e.group === groupKey)
                                            delegate: Item {
                                                required property var modelData
                                                width: addCol.width / 3
                                                height: 62
                                                Rectangle {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    y: 0
                                                    width: 36
                                                    height: 36
                                                    radius: 18
                                                    color: "transparent"
                                                    border.color: eh.hovered ? Theme.accent : Theme.border
                                                    border.width: 1
                                                    EffectIcon {
                                                        anchors.centerIn: parent
                                                        size: 22
                                                        type: modelData.type
                                                        active: eh.hovered
                                                    }
                                                }
                                                Text {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    y: 40
                                                    width: parent.width
                                                    horizontalAlignment: Text.AlignHCenter
                                                    text: modelData.label
                                                    color: eh.hovered ? Theme.accent : Theme.text
                                                    font.family: Theme.font
                                                    font.pixelSize: 10
                                                    elide: Text.ElideRight
                                                }
                                                HoverHandler {
                                                    id: eh
                                                    cursorShape: Qt.PointingHandCursor
                                                }
                                                TapHandler {
                                                    onTapped: studio.addLayer(modelData.type)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Device + LED selection
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: "#3a3a3a"
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 20
                spacing: 12
                ModeTabs {
                    visible: studio.devicesHere.length > 1
                    minTabWidth: 120
                    model: studio.tabNames
                    currentIndex: studio.combined ? studio.devicesHere.length : Math.max(0, studio.devicesHere.findIndex(d => d.pid === studio.editPid))
                    onActivated: i => {
                        if (i >= studio.devicesHere.length) {
                            studio.showCombined(true);
                        } else {
                            studio.setMode(false, studio.devicesHere[i].pid);
                        }
                    }
                }
                // Toolbar: quick select, select all / clear, arrange, undo / redo.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    enabled: studio.current >= 0
                    Text {
                        text: i18n("Quick select")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    SynDropdown {
                        objectName: "quickSelect"
                        implicitWidth: 190
                        implicitHeight: 30
                        model: studio.quickEntries.map(e => e.text)
                        currentIndex: -1
                        displayText: i18n("Choose group…")
                        onActivated: i => {
                            studio.applyQuick(studio.quickEntries[i]);
                            currentIndex = -1;
                        }
                    }
                    ToolBtn {
                        objectName: "selectAll"
                        kind: "all"
                        tip: i18n("Select all LEDs")
                        onClicked: studio.applyQuick(studio.quickEntries[0])
                    }
                    ToolBtn {
                        objectName: "clearSelection"
                        kind: "clear"
                        tip: i18n("Clear selection")
                        onClicked: studio.cleared = true
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    SynButton {
                        objectName: "arrangeButton"
                        visible: studio.combined
                        enabled: true
                        text: studio.arranging ? i18n("Done") : i18n("Arrange devices")
                        primary: studio.arranging
                        implicitWidth: 150
                        implicitHeight: 28
                        onClicked: studio.arranging = !studio.arranging
                    }
                    ToolBtn {
                        objectName: "undoBtn"
                        kind: "undo"
                        tip: i18n("Undo (Ctrl+Z)")
                        enabled: studio.canUndo
                        onClicked: studio.undo()
                    }
                    ToolBtn {
                        objectName: "redoBtn"
                        kind: "redo"
                        tip: i18n("Redo (Ctrl+Shift+Z)")
                        enabled: studio.canRedo
                        onClicked: studio.redo()
                    }
                }
                // Device size controls while arranging the canvas.
                RowLayout {
                    visible: studio.combined && studio.arranging
                    Layout.fillWidth: true
                    spacing: 8
                    Text {
                        text: i18n("Drag a device to move it.")
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    Repeater {
                        model: studio.devicesHere
                        delegate: RowLayout {
                            required property var modelData
                            spacing: 4
                            Text {
                                text: modelData.name
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            ToolBtn {
                                objectName: "shrink" + modelData.pid
                                kind: "minus"
                                tip: i18n("Smaller")
                                onClicked: studio.scalePlacement(modelData.pid, 1 / 1.1)
                            }
                            ToolBtn {
                                objectName: "grow" + modelData.pid
                                kind: "plus"
                                tip: i18n("Larger")
                                onClicked: studio.scalePlacement(modelData.pid, 1.1)
                            }
                        }
                    }
                    SynButton {
                        objectName: "resetLayout"
                        text: i18n("Reset layout")
                        implicitWidth: 120
                        implicitHeight: 26
                        onClicked: studio.resetPlacements()
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                }
                Item {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                ZoomCanvas {
                    id: canvasHolder
                    objectName: "studioCanvas"
                    anchors.fill: parent
                    selecting: !(studio.combined && studio.arranging)
                    // Where the shared stage sits in the canvas (the stage keeps its proportions).
                    readonly property real sk: Math.min(width / studio.stageW, height / studio.stageH)
                    readonly property real sox: (width - studio.stageW * sk) / 2
                    readonly property real soy: (height - studio.stageH * sk) / 2
                    // Pixel rectangle of a device on the canvas (the whole area when one device is shown).
                    function rectOf(p) {
                        if (!studio.combined) {
                            return Qt.rect(0, 0, width, height);
                        }
                        const pl = studio.placementOf(p);
                        return Qt.rect(sox + pl.x * sk, soy + pl.y * sk, pl.w * sk, pl.h * sk);
                    }
                    // Where a device's drawing goes so that the box round its LEDs fills its placement
                    // (what you see is what the canvas layers are laid out over).
                    function viewRect(p) {
                        const r = rectOf(p);
                        if (!studio.combined) {
                            return r;
                        }
                        const v = p === "00aa" ? mouseView : kbView;
                        if (!v || !v.view) {
                            return r;
                        }
                        // One scale for both axes (the LED box keeps its shape), centred in the placement.
                        const lp = v.ledsPic, s = Math.min(r.width / lp.width, r.height / lp.height);
                        const bx = r.x + (r.width - lp.width * s) / 2, by = r.y + (r.height - lp.height * s) / 2;
                        if (p === "00aa") {
                            const pad = v.framePad;
                            return Qt.rect(bx - lp.x * s - pad, by - lp.y * s - pad, v.view.width * s + 2 * pad, v.view.height * s + 2 * pad);
                        }
                        const c = v.contentPic;
                        return Qt.rect(bx - (lp.x - c.x) * s, by - (lp.y - c.y) * s, c.width * s, c.height * s);
                    }
                    // Rubber band: forwarded to the device under the band's centre (canvas view)
                    // or to the one device shown.
                    function target(r) {
                        if (!studio.combined) {
                            return studio.editMouse ? mouseView : kbView;
                        }
                        const c = Qt.point(r.x + r.width / 2, r.y + r.height / 2);
                        const m = rectOf("00aa"), pad = m.width * 0.15;
                        return (c.x >= m.x - pad && c.x <= m.x + m.width + pad && c.y >= m.y - pad && c.y <= m.y + m.height + pad) ? mouseView : kbView;
                    }
                    onBoxSelected: (r, add) => {
                        const t = target(r);
                        const f = t.mapFromItem(canvasHolder.contentLayer, r.x, r.y);
                        t.selectBox(Qt.rect(f.x, f.y, r.width, r.height), add);
                    }
                    onTapped: (p, add) => {
                        const t = target(Qt.rect(p.x, p.y, 0, 0));
                        t.selectAt(t.mapFromItem(canvasHolder.contentLayer, p.x, p.y), add);
                    }
                    Rectangle {
                        // The stage: the area the canvas layers are laid out over.
                        visible: studio.combined
                        x: canvasHolder.sox
                        y: canvasHolder.soy
                        width: studio.stageW * canvasHolder.sk
                        height: studio.stageH * canvasHolder.sk
                        color: "#2e2e2e"
                        border.color: studio.arranging ? Theme.accent : "#444444"
                    }
                    LedKeyboard {
                        id: kbView
                        objectName: "kbView"
                        visible: studio.combined || !studio.editMouse
                        x: canvasHolder.viewRect(studio.pid).x
                        y: canvasHolder.viewRect(studio.pid).y
                        width: canvasHolder.viewRect(studio.pid).width
                        height: canvasHolder.viewRect(studio.pid).height
                        interactive: false
                        view: studio.kbShown ? studio.kbShown.views.top : null
                        dir: studio.kbShown ? studio.kbShown.dir : ""
                        colours: studio.liveByPid[studio.pid] || {}
                        selection: studio.selectionFor(studio.pid)
                        onSelectionEdited: sel => studio.pickLeds(studio.pid, sel)
                    }
                    Item {
                        id: mouseHolder
                        visible: studio.combined || studio.editMouse
                        x: canvasHolder.viewRect("00aa").x
                        y: canvasHolder.viewRect("00aa").y
                        width: canvasHolder.viewRect("00aa").width
                        height: canvasHolder.viewRect("00aa").height
                        MouseLeds {
                            id: mouseView
                            objectName: "mouseView"
                            anchors.fill: parent
                            interactive: false
                            view: studio.mouseShown ? studio.mouseShown.views.top : null
                            dir: studio.mouseShown ? studio.mouseShown.dir : ""
                            colours: studio.liveByPid["00aa"] || {}
                            selection: studio.selectionFor("00aa")
                            onSelectionEdited: sel => studio.pickLeds("00aa", sel)
                        }
                    }
                }
                // Drag handles while arranging (above the canvas, following its zoom and pan).
                Repeater {
                    model: studio.combined && studio.arranging ? studio.devicesHere : []
                    delegate: Rectangle {
                        id: handle
                        required property var modelData
                        objectName: "handle" + modelData.pid
                        readonly property rect r: canvasHolder.rectOf(modelData.pid)
                        x: canvasHolder.panX + canvasHolder.zoom * r.x
                        y: canvasHolder.panY + canvasHolder.zoom * r.y
                        width: canvasHolder.zoom * r.width
                        height: canvasHolder.zoom * r.height
                        color: "#1444d62c"
                        border.color: Theme.accent
                        border.width: 2
                        z: 10
                        Text {
                            anchors.left: parent.left
                            anchors.top: parent.top
                            anchors.margins: 4
                            text: modelData.name
                            color: Theme.accent
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                            property point grab
                            property rect origin
                            onPressed: m => {
                                grab = mapToItem(canvasHolder, m.x, m.y);
                                origin = Qt.rect(studio.placementOf(modelData.pid).x, studio.placementOf(modelData.pid).y, 0, 0);
                            }
                            onPositionChanged: m => {
                                if (!pressed) {
                                    return;
                                }
                                const p = mapToItem(canvasHolder, m.x, m.y);
                                const k = canvasHolder.zoom * canvasHolder.sk;
                                studio.movePlacement(modelData.pid, origin.x + (p.x - grab.x) / k, origin.y + (p.y - grab.y) / k);
                            }
                        }
                    }
                }
                Text {
                    visible: studio.editMouse || studio.combined
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.margins: 4
                    width: parent.width - 220
                    wrapMode: Text.Wrap
                    text: studio.current >= 0 ? (studio.combined ? i18n("Canvas layer: click LEDs on either device or drag a box around them. Shift or Ctrl adds to the selection.") : i18n("Click LEDs or drag a box around them to choose where this layer lights. Shift or Ctrl adds to the selection.")) : i18n("Add or pick a layer, then choose its LEDs here.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                }
                Flow {
                    visible: !studio.combined
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    spacing: 6
                    enabled: studio.current >= 0
                    opacity: enabled ? 1 : 0.4
                    SectionLabel {
                        caption: i18n("Layer LEDs")
                        width: 90
                        height: 28
                        verticalAlignment: Text.AlignVCenter
                    }
                    Repeater {
                        model: studio.groupsFor(studio.editPid)
                        delegate: Rectangle {
                            required property var modelData
                            width: gl.implicitWidth + 22
                            height: 28
                            color: gh.hovered ? "#1affffff" : Theme.panel
                            border.color: Theme.borderSoft
                            Text {
                                id: gl
                                anchors.centerIn: parent
                                text: modelData.label
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            HoverHandler {
                                id: gh
                            }
                            TapHandler {
                                onTapped: studio.applyGroup(modelData.key, studio.editPid)
                            }
                        }
                    }
                }
                // Canvas: the devices each layer covers.
                Flow {
                    visible: studio.combined
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    spacing: 6
                    enabled: studio.current >= 0
                    opacity: enabled ? 1 : 0.4
                    SectionLabel {
                        caption: i18n("Layer devices")
                        width: 110
                        height: 28
                        verticalAlignment: Text.AlignVCenter
                    }
                    Repeater {
                        model: studio.devicesHere
                        delegate: Rectangle {
                            required property var modelData
                            readonly property bool on: studio.current >= 0 && !!studio.layers[studio.current] && !!studio.layers[studio.current].targets && studio.layers[studio.current].targets[modelData.pid] !== undefined
                            objectName: "targetChip" + modelData.pid
                            width: tl.implicitWidth + 22
                            height: 28
                            color: on ? Theme.accentTint : Theme.panel
                            border.color: on ? Theme.accent : Theme.borderSoft
                            Text {
                                id: tl
                                anchors.centerIn: parent
                                text: modelData.name
                                color: on ? Theme.accent : Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            TapHandler {
                                onTapped: studio.toggleTarget(modelData.pid)
                            }
                        }
                    }
                }
            }
            }

            // ---- Effect settings
            Rectangle {
                id: fx
                Layout.preferredWidth: 340
                clip: true
                Layout.fillHeight: true
                color: Theme.panel
                visible: studio.current >= 0 && studio.layers[studio.current] !== undefined
                readonly property var cur: studio.layers[studio.current]
                readonly property var info: cur ? studio.effectInfo(cur.effect.type) : null
                readonly property string type: cur ? cur.effect.type : ""
                readonly property int slots: !cur ? 0 : (cur.effect.type === "static" ? 1 : (cur.effect.colours || []).length)
                readonly property bool isMeter: type === "audio_meter"
                readonly property bool isAmbient: type === "ambient"
                readonly property bool isImage: type === "image"
                readonly property bool isGradient: type === "gradient"
                readonly property bool isWheel: type === "wheel"
                readonly property bool hasColours: !!info && (info.colours > 0 || info.stops === true)
                readonly property bool timeless: ["static", "audio_meter", "ambient", "battery", "image"].indexOf(type) >= 0 || (isGradient && !cur.effect.scroll)
                readonly property bool randomIsRainbow: ["wave", "wheel", "tidal"].indexOf(type) >= 0
                property var audioSources: []
                onIsMeterChanged: if (isMeter) loadSources()
                function loadSources() {
                    audioSources = typeof studio.backend.studio_call === "function" ? JSON.parse(studio.backend.studio_call("audio_sources", "") || "[]") : [];
                }
                Flickable {
                    id: fxFlick
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: saveBar.top
                    clip: true
                    contentWidth: width
                    contentHeight: fxCol.implicitHeight + 36
                    boundsBehavior: Flickable.StopAtBounds
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                ColumnLayout {
                    id: fxCol
                    x: 18
                    y: 18
                    width: fxFlick.width - 36
                    spacing: 12

                    // Name, duplicate, reset
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        spacing: 8
                        QQC2.TextField {
                            objectName: "layerName"
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            Layout.minimumWidth: 60
                            text: fx.cur ? fx.cur.name : ""
                            placeholderText: i18n("Layer name")
                            maximumLength: 48
                            onEditingFinished: {
                                const v = text;
                                if (fx.cur && fx.cur.name !== v) {
                                    studio.updateCurrent(l => l.name = v);
                                }
                            }
                        }
                        SynButton {
                            objectName: "duplicateLayer"
                            text: i18n("Copy")
                            implicitWidth: 64
                            implicitHeight: 30
                            onClicked: studio.duplicateLayer(studio.current)
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: i18n("Duplicate this layer")
                        }
                        SynButton {
                            objectName: "resetEffect"
                            text: i18n("Reset")
                            implicitWidth: 64
                            implicitHeight: 30
                            onClicked: studio.resetEffect()
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: i18n("Put this effect's options back to their defaults")
                        }
                    }
                    // Device target
                    SectionLabel {
                        caption: i18n("Devices")
                        visible: studio.hasMouse
                    }
                    Text {
                        visible: studio.hasMouse
                        Layout.fillWidth: true
                        objectName: "deviceTargetText"
                        text: studio.combined ? i18n("This layer spans: %1.", studio.coverText(fx.cur || { leds: [] })) : i18n("This layer lights the %1 only.", studio.deviceName(studio.editPid).toLowerCase())
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.Wrap
                    }
                    SynButton {
                        objectName: "spanLayer"
                        visible: studio.hasMouse && !studio.combined
                        text: i18n("Span across devices")
                        implicitWidth: 200
                        implicitHeight: 30
                        onClicked: studio.spanCurrent()
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: i18n("Move this layer onto the shared canvas so one effect runs across the keyboard and the mouse")
                    }
                    SynButton {
                        objectName: "unspanLayer"
                        visible: studio.combined && !!fx.cur && !!fx.cur.targets && Object.keys(fx.cur.targets).length === 1
                        text: i18n("Move to the device's layers")
                        implicitWidth: 240
                        implicitHeight: 30
                        onClicked: studio.unspanCurrent()
                    }

                    SectionLabel {
                        caption: i18n("Layer brightness")
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        SynSlider {
                            objectName: "layerBrightness"
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            stepSize: 1
                            value: fx.cur && fx.cur.brightness !== undefined ? Math.round(fx.cur.brightness * 100) : 100
                            onMoved: {
                                const v = value / 100;
                                studio.updateCurrent(l => l.brightness = v);
                            }
                        }
                        Text {
                            text: (fx.cur && fx.cur.brightness !== undefined ? Math.round(fx.cur.brightness * 100) : 100) + "%"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    SectionLabel {
                        caption: i18n("Opacity")
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        SynSlider {
                            objectName: "layerOpacity"
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            stepSize: 1
                            value: fx.cur && fx.cur.opacity !== undefined ? Math.round(fx.cur.opacity * 100) : 100
                            onMoved: {
                                const v = value / 100;
                                studio.updateCurrent(l => l.opacity = v);
                            }
                        }
                        Text {
                            text: (fx.cur && fx.cur.opacity !== undefined ? Math.round(fx.cur.opacity * 100) : 100) + "%"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    SectionLabel {
                        caption: i18n("Speed")
                        visible: !fx.timeless || fx.isImage
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        visible: !fx.timeless || fx.isImage
                        SynSlider {
                            objectName: "layerSpeed"
                            Layout.fillWidth: true
                            from: 0.25
                            to: 4
                            value: fx.cur && fx.cur.speed !== undefined ? fx.cur.speed : 1
                            onMoved: {
                                const v = Math.round(value * 20) / 20;
                                studio.updateCurrent(l => l.speed = v);
                            }
                        }
                        Text {
                            text: (fx.cur && fx.cur.speed !== undefined ? fx.cur.speed : 1).toFixed(2) + "×"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    RowLayout {
                        spacing: 10
                        Text {
                            text: i18n("Blend")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        ModeTabs {
                            objectName: "blendTabs"
                            minTabWidth: 72
                            model: [i18n("Cover"), i18n("Add"), i18n("Lighten")]
                            currentIndex: fx.cur ? Math.max(0, ["cover", "add", "lighten"].indexOf(fx.cur.blend || "cover")) : 0
                            onActivated: i => studio.updateCurrent(l => l.blend = ["cover", "add", "lighten"][i])
                        }
                        HelpButton {
                            size: 16
                            text: i18n("Blend controls how this layer mixes with the layers below it. Cover replaces them on its LEDs, Add stacks the light, Lighten keeps whichever colour is brighter. Opacity fades the whole layer into what is below.")
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Cover hides the layers below on this layer's LEDs; Add mixes its light on top; Lighten keeps the brighter colour.")
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                    SectionLabel {
                        caption: i18n("Effect")
                    }
                    Flow {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        spacing: 6
                        Repeater {
                            model: studio.effects
                            delegate: Rectangle {
                                required property var modelData
                                readonly property bool on: !!fx.cur && fx.cur.effect.type === modelData.type
                                width: el.implicitWidth + 18
                                height: 28
                                color: on ? Theme.accentTint : Theme.bg
                                border.color: on ? Theme.accent : Theme.borderSoft
                                Text {
                                    id: el
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: on ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                TapHandler {
                                    onTapped: studio.updateCurrent(l => {
                                        l.effect = studio.newEffect(modelData.type);
                                        l.name = modelData.label;
                                    })
                                }
                            }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Reactive and Ripple run in the keyboard itself: pick them on the device's LIGHTING tab.")
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }

                    // Colours: random / rainbow, or any number of colours (a gradient)
                    SectionLabel {
                        caption: fx.isGradient || (fx.info && fx.info.stops === true) ? i18n("Colours") : i18n("Colour")
                        visible: fx.hasColours
                    }
                    ModeTabs {
                        objectName: "colourMode"
                        visible: !!fx.info && fx.info.random === true
                        minTabWidth: 110
                        model: [fx.randomIsRainbow ? i18n("Rainbow") : i18n("Random"), i18n("Colours")]
                        currentIndex: fx.cur ? ((fx.cur.effect.colours || []).length === 0 ? 0 : 1) : 0
                        onActivated: index => studio.updateCurrent(l => {
                            if (index === 0) {
                                l.effect.colours = [];
                            } else if ((l.effect.colours || []).length === 0) {
                                l.effect.colours = l.effect.type === "breathing" ? ["44d62c"] : ["44d62c", "0080ff"];
                            }
                            studio.editingColour = 0;
                        })
                    }
                    GradBar {
                        objectName: "gradientBar"
                        visible: !!fx.info && fx.info.stops === true && fx.slots > 1
                        Layout.fillWidth: true
                        Layout.preferredHeight: 14
                        stops: fx.cur && fx.cur.effect.colours ? fx.cur.effect.colours : []
                    }
                    Row {
                        spacing: 10
                        visible: fx.hasColours
                        Repeater {
                            model: {
                                const l = fx.cur;
                                if (!l) {
                                    return [];
                                }
                                return l.effect.type === "static" ? [l.effect.colour] : (l.effect.colours || []);
                            }
                            delegate: Rectangle {
                                required property string modelData
                                required property int index
                                readonly property bool sel: Math.min(studio.editingColour, fx.slots - 1) === index
                                objectName: "stop" + index
                                width: 36
                                height: 36
                                radius: 18
                                color: "#" + modelData
                                border.color: sel ? Theme.accent : Theme.border
                                border.width: sel ? 3 : 1
                                TapHandler {
                                    onTapped: studio.editingColour = index
                                }
                            }
                        }
                        Text {
                            visible: fx.slots === 0 && fx.hasColours
                            height: 36
                            verticalAlignment: Text.AlignVCenter
                            text: fx.randomIsRainbow ? i18n("Rainbow") : i18n("Random colours")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        ToolBtn {
                            objectName: "addStop"
                            visible: !!fx.info && fx.info.stops === true && fx.slots > 0 && fx.slots < 8
                            y: 6
                            kind: "plus"
                            tip: i18n("Add a colour")
                            onClicked: studio.addStop()
                        }
                        ToolBtn {
                            objectName: "removeStop"
                            visible: !!fx.info && fx.info.stops === true && fx.slots > (fx.isGradient ? 2 : 1)
                            y: 6
                            kind: "minus"
                            tip: i18n("Remove the selected colour")
                            onClicked: studio.removeStop()
                        }
                    }
                    ColourPanel {
                        objectName: "colourPanel"
                        Layout.fillWidth: true
                        visible: fx.slots > 0
                        colour: studio.editedColour() || "44d62c"
                        onEdited: hex => studio.setColour(hex)
                    }

                    // Gradient angle / scrolling
                    SectionLabel {
                        caption: i18n("Angle")
                        visible: fx.isGradient
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: fx.isGradient
                        spacing: 10
                        SynSlider {
                            objectName: "gradientAngle"
                            Layout.fillWidth: true
                            from: 0
                            to: 360
                            stepSize: 5
                            value: fx.isGradient && fx.cur.effect.angle !== undefined ? fx.cur.effect.angle : 0
                            onMoved: {
                                const v = value;
                                studio.updateCurrent(l => l.effect.angle = v);
                            }
                        }
                        Text {
                            text: Math.round(fx.isGradient && fx.cur.effect.angle !== undefined ? fx.cur.effect.angle : 0) + "°"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    SynCheckBox {
                        objectName: "gradientScroll"
                        visible: fx.isGradient
                        text: i18n("Scroll the gradient")
                        checked: fx.isGradient && !!fx.cur.effect.scroll
                        onToggled: {
                            const v = checked;
                            studio.updateCurrent(l => l.effect.scroll = v);
                        }
                    }

                    // Duration
                    SectionLabel {
                        caption: i18n("Duration")
                        visible: !!fx.cur && !fx.timeless
                    }
                    ModeTabs {
                        visible: !!fx.cur && !fx.timeless
                        minTabWidth: 96
                        model: [i18n("Short"), i18n("Medium"), i18n("Long")]
                        currentIndex: fx.cur ? ["short", "medium", "long"].indexOf(fx.cur.effect.duration || "medium") : 1
                        onActivated: index => studio.updateCurrent(l => l.effect.duration = ["short", "medium", "long"][index])
                    }

                    // Direction
                    SectionLabel {
                        caption: i18n("Direction")
                        visible: fx.info && fx.info.directions !== undefined
                    }
                    Column {
                        spacing: 6
                        visible: fx.info && fx.info.directions !== undefined
                        Repeater {
                            model: fx.info && fx.info.directions ? fx.info.directions : []
                            delegate: Rectangle {
                                required property string modelData
                                required property int index
                                readonly property var lay: fx.cur
                                readonly property bool on: !!lay && (lay.effect.type === "tidal" ? (lay.effect.inward === (index === 1)) : studio.dirKeys[lay.effect.type][index] === lay.effect.direction)
                                width: 240
                                height: 30
                                color: on ? Theme.accentTint : Theme.bg
                                border.color: on ? Theme.accent : Theme.borderSoft
                                Text {
                                    anchors.centerIn: parent
                                    text: modelData
                                    color: on ? Theme.accent : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                TapHandler {
                                    onTapped: studio.updateCurrent(l => {
                                        if (l.effect.type === "tidal") {
                                            l.effect.inward = index === 1;
                                        } else {
                                            l.effect.direction = studio.dirKeys[l.effect.type][index];
                                        }
                                    })
                                }
                            }
                        }
                    }

                    // Wheel: hub position and how many colour cycles go round
                    SectionLabel {
                        caption: i18n("Rotation centre")
                        visible: fx.isWheel
                    }
                    SynDropdown {
                        objectName: "wheelCentre"
                        visible: fx.isWheel
                        Layout.fillWidth: true
                        readonly property var spots: [[0.5, 0.5], [0.1, 0.5], [0.9, 0.5], [0.5, 0.1], [0.5, 0.9]]
                        model: [i18n("Middle of the device"), i18n("Left edge"), i18n("Right edge"), i18n("Top edge"), i18n("Bottom edge")]
                        currentIndex: {
                            if (!fx.isWheel) {
                                return 0;
                            }
                            const c = fx.cur.effect.centre || [0.5, 0.5];
                            const i = spots.findIndex(s => Math.abs(s[0] - c[0]) < 0.02 && Math.abs(s[1] - c[1]) < 0.02);
                            return i < 0 ? 0 : i;
                        }
                        onActivated: index => studio.updateCurrent(l => l.effect.centre = spots[index])
                    }
                    SectionLabel {
                        caption: i18n("Colour spread")
                        visible: fx.isWheel
                    }
                    SynSlider {
                        objectName: "wheelSpread"
                        Layout.fillWidth: true
                        visible: fx.isWheel
                        from: 0.5
                        to: 3
                        stepSize: 0.5
                        value: fx.isWheel && fx.cur.effect.spread !== undefined ? fx.cur.effect.spread : 1
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.spread = v);
                        }
                    }

                    // Starlight density
                    SectionLabel {
                        caption: i18n("Density")
                        visible: fx.info && fx.info.density === true
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.info && fx.info.density === true
                        from: 0.02
                        to: 0.6
                        value: fx.cur && fx.cur.effect.density ? fx.cur.effect.density : 0.15
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.density = v);
                        }
                    }

                    // Audio Meter options
                    SectionLabel {
                        caption: i18n("Audio source")
                        visible: fx.isMeter
                    }
                    SynDropdown {
                        objectName: "audioSource"
                        visible: fx.isMeter
                        Layout.fillWidth: true
                        readonly property var names: fx.audioSources.map(s => s.name)
                        model: [i18n("What the PC is playing")].concat(fx.audioSources.map(s => s.monitor ? i18n("Output: %1", s.description) : i18n("Input: %1", s.description)))
                        currentIndex: {
                            if (!fx.isMeter || !fx.cur.effect.source) {
                                return 0;
                            }
                            const i = names.indexOf(fx.cur.effect.source);
                            return i < 0 ? 0 : i + 1;
                        }
                        onActivated: i => studio.updateCurrent(l => {
                            if (i === 0) {
                                delete l.effect.source;
                            } else {
                                l.effect.source = names[i - 1];
                            }
                        })
                    }
                    SectionLabel {
                        caption: i18n("Display")
                        visible: fx.isMeter
                    }
                    ModeTabs {
                        objectName: "meterMode"
                        visible: fx.isMeter
                        minTabWidth: 110
                        model: [i18n("Bands"), i18n("Level")]
                        currentIndex: fx.isMeter && fx.cur.effect.mode === "level" ? 1 : 0
                        onActivated: index => studio.updateCurrent(l => l.effect.mode = index === 1 ? "level" : "spectrum")
                    }
                    SynCheckBox {
                        objectName: "peakHold"
                        visible: fx.isMeter
                        text: i18n("Hold peaks")
                        checked: fx.isMeter && !!fx.cur.effect.peak_hold
                        onToggled: {
                            const v = checked;
                            studio.updateCurrent(l => l.effect.peak_hold = v);
                        }
                    }
                    SectionLabel {
                        caption: i18n("Sensitivity")
                        visible: fx.isMeter
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isMeter
                        from: 0.5
                        to: 3.0
                        value: fx.isMeter && fx.cur.effect.gain !== undefined ? fx.cur.effect.gain : 1.0
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.gain = v);
                        }
                    }
                    SectionLabel {
                        caption: i18n("Colour boost")
                        visible: fx.isMeter
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isMeter
                        from: 0.4
                        to: 2.0
                        value: fx.isMeter && fx.cur.effect.boost !== undefined ? fx.cur.effect.boost : 1.0
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.boost = v);
                        }
                    }
                    SynCheckBox {
                        visible: fx.isMeter
                        text: i18n("Flow (colours move across the bars)")
                        checked: fx.isMeter && !!fx.cur.effect.flow
                        onToggled: {
                            const v = checked;
                            studio.updateCurrent(l => l.effect.flow = v);
                        }
                    }

                    // Ambient options
                    SectionLabel {
                        caption: i18n("Screen area")
                        visible: fx.isAmbient
                    }
                    AreaPicker {
                        visible: fx.isAmbient
                        Layout.fillWidth: true
                        Layout.preferredHeight: width * 9 / 16
                        backend: studio.backend
                        area: fx.isAmbient && fx.cur.effect.area ? fx.cur.effect.area : [0, 0, 1, 1]
                        onAreaEdited: a => studio.updateCurrent(l => l.effect.area = a)
                    }
                    SectionLabel {
                        caption: i18n("Vividness")
                        visible: fx.isAmbient
                    }
                    SynSlider {
                        Layout.fillWidth: true
                        visible: fx.isAmbient
                        from: 0.6
                        to: 2.0
                        value: fx.isAmbient && fx.cur.effect.boost !== undefined ? fx.cur.effect.boost : 1.3
                        onMoved: {
                            const v = value;
                            studio.updateCurrent(l => l.effect.boost = v);
                        }
                    }

                    // Image options
                    SectionLabel {
                        caption: i18n("Picture")
                        visible: fx.isImage
                    }
                    PixThumb {
                        objectName: "imageThumb"
                        visible: fx.isImage
                        Layout.fillWidth: true
                        Layout.preferredHeight: 90
                        w: fx.isImage ? (fx.cur.effect.w || 0) : 0
                        h: fx.isImage ? (fx.cur.effect.h || 0) : 0
                        pixels: fx.isImage ? (fx.cur.effect.data || "") : ""
                    }
                    SynButton {
                        objectName: "chooseImage"
                        visible: fx.isImage
                        text: fx.isImage && fx.cur.effect.w > 0 ? i18n("Choose another picture…") : i18n("Choose a picture…")
                        implicitWidth: 220
                        implicitHeight: 30
                        onClicked: imageDlg.open()
                    }
                    Text {
                        visible: fx.isImage
                        Layout.fillWidth: true
                        text: fx.isImage && fx.cur.effect.w > 0 ? i18n("A %1×%2 copy of the picture is kept in the profile.", fx.cur.effect.w, fx.cur.effect.h) : i18n("The picture is shrunk to at most 64 pixels a side and stored in the profile, so exported profiles are self-contained.")
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 11
                        wrapMode: Text.Wrap
                    }
                    SectionLabel {
                        caption: i18n("Fit")
                        visible: fx.isImage
                    }
                    ModeTabs {
                        objectName: "imageFit"
                        visible: fx.isImage
                        minTabWidth: 84
                        model: [i18n("Stretch"), i18n("Fill"), i18n("Fit")]
                        currentIndex: fx.isImage ? Math.max(0, ["stretch", "cover", "contain"].indexOf(fx.cur.effect.fit || "stretch")) : 0
                        onActivated: index => studio.updateCurrent(l => l.effect.fit = ["stretch", "cover", "contain"][index])
                    }
                    SectionLabel {
                        caption: i18n("Scroll")
                        visible: fx.isImage
                    }
                    ModeTabs {
                        objectName: "imageScroll"
                        visible: fx.isImage
                        minTabWidth: 56
                        model: [i18n("None"), "←", "→", "↑", "↓"]
                        currentIndex: fx.isImage ? Math.max(0, ["none", "left", "right", "up", "down"].indexOf(fx.cur.effect.scroll || "none")) : 0
                        onActivated: index => studio.updateCurrent(l => l.effect.scroll = ["none", "left", "right", "up", "down"][index])
                    }
                }
                }
                Rectangle {
                    id: saveBar
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 56
                    color: Theme.panel
                    Rectangle {
                        width: parent.width
                        height: 1
                        color: Theme.borderSoft
                    }
                    SynButton {
                        anchors.centerIn: parent
                        text: i18n("Save")
                        primary: true
                        implicitWidth: 160
                        enabled: !studio.backend.readonly && studio.profile.name.trim() !== ""
                        onClicked: studio.saveProfile(true)
                    }
                }
            }
        }
    }

    // Loads a picture, shrinks it to at most 64 pixels a side and stores it in the layer.
    Image {
        id: pic
        visible: false
        cache: false
        autoTransform: true
        asynchronous: false
        onStatusChanged: if (status === Image.Ready) {
            const s = Math.min(1, 64 / Math.max(implicitWidth, implicitHeight));
            sampler.w = Math.max(1, Math.round(implicitWidth * s));
            sampler.h = Math.max(1, Math.round(implicitHeight * s));
            sampler.requestPaint();
        }
    }
    Canvas {
        id: sampler
        visible: false
        property int w: 1
        property int h: 1
        width: w
        height: h
        onPaint: {
            if (pic.status !== Image.Ready) {
                return;
            }
            const ctx = getContext("2d");
            ctx.reset();
            ctx.drawImage(pic, 0, 0, w, h);
            const d = ctx.getImageData(0, 0, w, h);
            const hex = studio.pixelsHex(d, w, h);
            const ww = w, hh = h;
            studio.updateCurrent(l => {
                if (l.effect.type === "image") {
                    l.effect.w = ww;
                    l.effect.h = hh;
                    l.effect.data = hex;
                }
            });
        }
    }
    FileDialog {
        id: imageDlg
        objectName: "imageDialog"
        title: i18n("Choose a picture")
        fileMode: FileDialog.OpenFile
        nameFilters: [i18n("Pictures (*.png *.jpg *.jpeg *.bmp *.gif *.webp)")]
        onAccepted: pic.source = selectedFile
    }

    FileDialog {
        id: exportDlg
        objectName: "exportDialog"
        title: i18n("Export lighting profile")
        fileMode: FileDialog.SaveFile
        nameFilters: [i18n("Tsuba lighting profile (*.json)")]
        defaultSuffix: "json"
        onAccepted: studio.exportProfile(selectedFile)
    }
    FileDialog {
        id: importDlg
        objectName: "importDialog"
        title: i18n("Import lighting profile")
        fileMode: FileDialog.OpenFile
        nameFilters: [i18n("Tsuba lighting profile (*.json)")]
        onAccepted: studio.importProfile(selectedFile)
    }

    SynapseImportDialog {
        id: synapseDlg
        objectName: "synapseImportDialog"
        anchors.fill: parent
        z: 50
        backend: studio.backend
        onImported: {
            studio.loadProfiles();
            Theme.profilesRev++;
        }
    }
}
