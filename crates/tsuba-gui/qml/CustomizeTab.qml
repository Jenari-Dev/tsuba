import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

Item {
    id: tab

    required property var backend
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    required property string pid
    required property bool isMouse

    property int keyLayer: 0
    property int profile: 1
    // The library profile being edited and how many there are (shortcut suggestions, hints).
    property string profileId: ""
    property int profileCount: 2
    onProfileChanged: {
        load();
        loadScroll();
    }
    // Bumped when another library profile was written into the same on-board slot.
    property int rev: 0
    onRevChanged: {
        load();
        loadScroll();
    }
    // Optional local image pack (Razer renders + hotspots); our own artwork otherwise.
    readonly property var pack: {
        Theme.picRev;
        const p = backend.device_pack(pid);
        return p === "" ? null : JSON.parse(p);
    }
    property var byPos: ({})
    property int selected: -1
    property string selectedName: ""

    // Keys that activate Shift layer on this device (from the standard layer).
    property var hypershiftKeys: []
    property var hypershiftPositions: []
    // Factory defaults for "reset layer" (mice: known table; keyboards: the first dump).
    function defaultFor(pos, layer) {
        return KeyNames.defaultFor(isMouse, pos, layer === undefined ? keyLayer : layer);
    }
    property string resetResult: ""
    property bool resetFailed: false
    function setResult(text, failed) {
        resetFailed = !!failed;
        resetResult = text;
    }
    property bool listView: false
    // Left-handed: swap the left and right click buttons.
    function swapHands() {
        const l = byPos[1], r = byPos[2];
        if (!l || !r) {
            return;
        }
        const a = backend.set_assignment(pid, JSON.stringify({ profile: profile, key: 1, layer: keyLayer, function: r.function, data: r.data }));
        const b = a === "" ? "" : backend.set_assignment(pid, JSON.stringify({ profile: profile, key: 2, layer: keyLayer, function: l.function, data: l.data }));
        const ok = a !== "" && b !== "";
        setResult(ok ? i18n("Left and right buttons swapped") : i18n("Failed: %1", backend.status), !ok);
        load();
    }
    function resetLayer() {
        let n = 0, failed = 0;
        for (const k in byPos) {
            const d = defaultFor(+k, keyLayer);
            const a = byPos[k];
            if (!d || (a.function === d.function && JSON.stringify(a.data) === JSON.stringify(d.data))) {
                continue;
            }
            const r = backend.set_assignment(pid, JSON.stringify({ profile: profile, key: +k, layer: keyLayer, function: d.function, data: d.data }));
            r === "" ? failed++ : n++;
        }
        if (failed > 0) {
            setResult(i18np("Reset %1 key to default, %2 failed", "Reset %1 keys to default, %2 failed", n, failed), true);
        } else {
            setResult(i18np("Reset %1 key to default", "Reset %1 keys to default", n), false);
        }
        load();
    }

    function keyName(pos) {
        const spots = pack && pack.views && pack.views.top ? pack.views.top.hotspots : [];
        const h = spots.find(x => x.pos === pos);
        const n = h ? h.name : i18nc("fallback name of a key", "key %1", pos);
        return n === "Function" ? "Fn" : n;   // i18n-ok: key cap
    }

    function load() {
        const s = backend.assignments(pid, profile, keyLayer);
        const m = {};
        if (s !== "") {
            JSON.parse(s).forEach(a => m[a.key] = a);
        }
        // Labels for keys bound to software actions (dynamic Razer keys).
        const labels = {};
        const byKey = {};
        JSON.parse(backend.actions(pid) || "[]").forEach(x => {
            labels[pid + ":" + x.code] = x.label;
            if (x.profile === profile && x.layer === keyLayer) {
                byKey[x.key] = x.label;
            }
        });
        KeyNames.currentPid = pid;
        KeyNames.actionLabels = labels;
        KeyNames.actionByKey = byKey;
        byPos = m;
        const base = keyLayer === 0 ? s : backend.assignments(pid, profile, 0);
        const hsList = base === "" ? [] : JSON.parse(base).filter(a => a.function === 12 || (a.function === 17 && a.data[0] === 1));
        hypershiftKeys = hsList.map(a => keyName(a.key));
        hypershiftPositions = hsList.map(a => a.key);
    }

    // Scroll wheel settings (mice): same calls as PERFORMANCE's Scrolling card.
    property var perf: ({})
    readonly property bool hasScroll: isMouse && (perf.free_spin !== undefined || perf.smart_reel !== undefined || perf.scroll_acceleration !== undefined)
    function loadScroll() {
        if (!isMouse) {
            return;
        }
        const s = backend.performance(pid, profile);
        perf = s === "" ? ({}) : JSON.parse(s);
    }
    function applyScroll(settings) {
        const r = backend.set_performance(pid, JSON.stringify(Object.assign({ profile: profile }, settings)));
        if (r === "") {
            setResult(i18n("Failed: %1", backend.status), true);
        }
        loadScroll();
    }

    signal openMacros()
    onKeyLayerChanged: load()
    Connections {
        target: Theme
        function onDataRevChanged() {
            tab.loadScroll();
            tab.load();
        }
    }
    Component.onCompleted: {
        loadScroll();
        // Names for keys bound to on-board macros.
        const mj = backend.macros();
        if (mj !== "") {
            const names = {};
            const devs = JSON.parse(mj).devices;
            for (const p in devs) {
                devs[p].macros.forEach(m => names[m.id] = m.name);
            }
            KeyNames.macroNames = names;
        }
        load();
        if (backend.env_var("TSUBA_LAYER") === "1") {   // headless UI checks
            keyLayer = 1;
        }
        const pre = backend.env_var("TSUBA_SELECT");
        if (pre !== "") {
            selected = parseInt(pre);
            selectedName = i18n("Selected (%1)", pre);
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Assignment panel docks on the left while a key / button is selected.
        AssignmentPanel {
            Layout.preferredWidth: tab.selected >= 0 ? 330 : 0
            Layout.fillHeight: true
            visible: tab.selected >= 0
            backend: tab.backend
            readOnly: tab.readOnly
            pid: tab.pid
            profile: tab.profile
            profileId: tab.profileId
            profileCount: tab.profileCount
            isMouse: tab.isMouse
            keyLayer: tab.keyLayer
            target: tab.selected
            targetName: tab.selectedName
            assignment: tab.byPos[tab.selected] || null
            hypershift: tab.keyLayer === 1
            allAssignments: tab.byPos
            hypershiftPositions: tab.hypershiftPositions
            onAssigned: tab.load()
            onClosed: tab.selected = -1
            onOpenMacros: tab.openMacros()
        }

        // The page scrolls when the render, its callouts and the cards below don't all fit.
        Flickable {
            id: pageFlick
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: width
            contentHeight: Math.max(height, mainCol.implicitHeight + 28)
            boundsBehavior: Flickable.StopAtBounds
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

        ColumnLayout {
            id: mainCol
            x: 20
            y: 8
            width: pageFlick.width - 40
            height: pageFlick.contentHeight - 28
            spacing: 14

            // Render on a faint dotted grid
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                // Tall enough for the callout columns (and the inset label) of a mouse render.
                Layout.minimumHeight: Math.max(300, mouseArea.item && mouseArea.item.neededH !== undefined ? mouseArea.item.neededH : 0)

                Canvas {
                    anchors.fill: parent
                    onWidthChanged: requestPaint()
                    onHeightChanged: requestPaint()
                    Connections {
                        target: Theme
                        function onDarkChanged() {
                            requestPaint();
                        }
                    }
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.clearRect(0, 0, width, height);
                        ctx.fillStyle = Theme.borderSoft;
                        for (let x = 12; x < width; x += 24) {
                            for (let y = 12; y < height; y += 24) {
                                ctx.fillRect(x, y, 2, 2);
                            }
                        }
                    }
                }
                Loader {
                    id: mouseArea
                    anchors.fill: parent
                    sourceComponent: tab.listView && tab.isMouse ? buttonList : (tab.pack ? (tab.isMouse ? mouseImage : keyboardImage) : (tab.isMouse ? mouseView : keyboardView))
                }
            }

            ApproxNote {
                objectName: "approxNote"
                visible: !!tab.pack && !!tab.pack.approximate
                Layout.alignment: Qt.AlignHCenter
                backend: tab.backend
                pid: tab.pid
            }

            // Scroll wheel (mice)
            SynCard {
                objectName: "scrollWheelCard"
                visible: tab.hasScroll
                Layout.alignment: Qt.AlignHCenter
                Layout.maximumWidth: 900
                title: i18n("Scroll wheel")
                help: i18n("Tactile gives the wheel distinct notches; Free-spin lets it turn freely. Auto free-spin switches between them by how fast you scroll, and scroll acceleration scrolls further the faster you spin.")
                RowLayout {
                    spacing: 28
                    RowLayout {
                        visible: tab.perf.free_spin !== undefined
                        spacing: 12
                        Text {
                            text: i18n("Scrolling mode")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        SynPillTabs {
                            enabled: !tab.readOnly
                            model: [i18nc("scroll wheel mode", "Tactile"), i18nc("scroll wheel mode", "Free-spin")]
                            currentIndex: tab.perf.free_spin ? 1 : 0
                            onActivated: i => tab.applyScroll({ free_spin: i === 1 })
                        }
                    }
                    RowLayout {
                        visible: tab.perf.smart_reel !== undefined
                        spacing: 12
                        Text {
                            text: i18n("Auto free-spin")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        Text {
                            text: i18nc("Razer name of the auto free-spin feature", "(Smart-Reel)")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        SynToggle {
                            enabled: !tab.readOnly
                            checked: !!tab.perf.smart_reel
                            onToggled: on => tab.applyScroll({ smart_reel: on })
                        }
                    }
                    RowLayout {
                        visible: tab.perf.scroll_acceleration !== undefined
                        spacing: 12
                        Text {
                            text: i18n("Scroll acceleration")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        SynToggle {
                            enabled: !tab.readOnly
                            checked: !!tab.perf.scroll_acceleration
                            onToggled: on => tab.applyScroll({ scroll_acceleration: on })
                        }
                    }
                }
            }

            // List toggle + Standard | Shift layer + help
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: 10
                Rectangle {
                    visible: tab.isMouse
                    Layout.preferredWidth: 40
                    Layout.preferredHeight: 34
                    radius: 17
                    color: lt.hovered ? Theme.controlFill : "transparent"
                    border.color: Theme.controlBorder
                    Rectangle {
                        anchors.centerIn: parent
                        width: 16
                        height: 12
                        color: "transparent"
                        border.color: Theme.text
                        border.width: 1
                        Rectangle {
                            x: 1
                            y: 1
                            width: 5
                            height: parent.height - 2
                            color: tab.listView ? Theme.accent : Theme.text
                        }
                    }
                    HoverHandler {
                        id: lt
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: tab.listView = !tab.listView
                    }
                }
                SynPillTabs {
                    model: [i18nc("key layer", "Standard"), i18nc("key layer", "Shift layer")]
                    currentIndex: tab.keyLayer
                    onActivated: index => tab.keyLayer = index
                }
                HelpButton {
                    size: 18
                    text: i18n("Standard is the layer you normally use. Shift layer holds a second set of assignments that are active only while you hold a key set to “Shift layer”, so each key can do two things.")
                }
                // Synapse-style hint: which key turns Shift layer on.
                Text {
                    visible: tab.keyLayer === 1
                    Layout.maximumWidth: 520
                    wrapMode: Text.Wrap
                    text: tab.hypershiftKeys.length
                        ? i18n("Hold %1 and press a key to use its shift-layer assignment", tab.hypershiftKeys.join(i18nc("separator between alternative keys", " or ")))
                        : i18n("No shift-layer key yet: pick a key on STANDARD and choose Shift layer")
                    color: Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
            }

            // Tools for the layer
            SynCard {
                Layout.alignment: Qt.AlignHCenter
                Layout.maximumWidth: 900
                title: i18nc("card title", "Buttons")
                help: i18n("Reset puts this layer back to its factory assignments. The device picture can be replaced with your own photo; the buttons stay where Tsuba's drawing has them.")
                Flow {
                    Layout.fillWidth: true
                    spacing: 10
                    SynButton {
                        text: i18n("Reset layer to default")
                        visible: tab.keyLayer === 0
                        enabled: !tab.readOnly
                        onClicked: resetConfirm.open()
                    }
                    SynButton {
                        visible: tab.isMouse
                        text: i18n("Swap left / right (left-handed)")
                        enabled: !tab.readOnly
                        onClicked: tab.swapHands()
                    }
                    SynButton {
                        text: tab.isMouse ? i18n("Mouse settings") : i18n("Keyboard settings")
                        onClicked: tab.backend.open_system_settings(tab.isMouse ? "mouse" : "keyboard")
                    }
                    Text {
                        visible: tab.resetResult !== ""
                        text: tab.resetResult
                        height: 32
                        verticalAlignment: Text.AlignVCenter
                        color: tab.resetFailed ? Theme.warn : Theme.accent
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: tab.keyLayer === 1
                        ? i18n("These assignments apply only while a shift-layer key is held. Click a key to change its shift-layer assignment.")
                        : i18n("Click a key or button to see its assignment. Remapped keys are shown in green.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                // Own picture of the device (the buttons stay where Tsuba's drawing has them).
                PictureControls {
                    objectName: "devicePicture"
                    backend: tab.backend
                    pid: tab.pid
                    label: i18n("Device picture")
                }
            }
            QQC2.Dialog {
                id: resetConfirm
                anchors.centerIn: parent
                modal: true
                title: i18n("Reset bindings")
                standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
                QQC2.Label {
                    width: 360
                    wrapMode: Text.Wrap
                    text: i18n("Put every key/button on this profile's STANDARD layer back to its default?")
                }
                onAccepted: tab.resetLayer()
            }
        }
        }
    }

    // Every button as a list (names from the render's hotspots).
    Component {
        id: buttonList
        ListView {
            clip: true
            spacing: 6
            model: Object.keys(tab.byPos).map(Number).sort((a, b) => a - b)
            delegate: Rectangle {
                required property int modelData
                readonly property var spot: tab.pack ? [].concat(...Object.values(tab.pack.views).map(v => v.hotspots || [])).find(h => h.pos === modelData) : null
                width: ListView.view.width
                height: 44
                color: tab.selected === modelData ? Theme.accentTint : (lh.hovered ? Theme.hoverTint : Theme.panel)
                border.color: tab.selected === modelData ? Theme.accent : Theme.borderSoft
                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 16
                    Text {
                        width: 220
                        elide: Text.ElideRight
                        text: (parent.parent.spot ? parent.parent.spot.name : i18n("Button %1", parent.parent.modelData)).toUpperCase()
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.letterSpacing: 1
                    }
                    Text {
                        text: KeyNames.describe(tab.byPos[parent.parent.modelData])
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                }
                HoverHandler {
                    id: lh
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: {
                        tab.selected = modelData;
                        tab.selectedName = spot ? spot.name : i18n("Button %1", modelData);
                    }
                }
            }
        }
    }

    Component {
        id: keyboardImage
        KeyboardImageView {
            view: tab.pack.views.top
            dir: tab.pack.dir
            assignments: tab.byPos
            selected: tab.selected
            onPicked: (pos, name) => {
                tab.selected = pos;
                tab.selectedName = name;
            }
        }
    }
    Component {
        id: mouseImage
        MouseImageView {
            pack: tab.pack
            dir: tab.pack.dir
            assignments: tab.byPos
            selected: tab.selected
            onPicked: (pos, name) => {
                tab.selected = pos;
                tab.selectedName = name;
            }
        }
    }
    Component {
        id: keyboardView
        KeyboardView {
            assignments: tab.byPos
            selected: tab.selected
            onPicked: (pos, name) => {
                tab.selected = pos;
                tab.selectedName = name;
            }
        }
    }
    Component {
        id: mouseView
        MouseView {
            assignments: tab.byPos
            selected: tab.selected
            onPicked: (pos, name) => {
                tab.selected = pos;
                tab.selectedName = name;
            }
        }
    }
}
