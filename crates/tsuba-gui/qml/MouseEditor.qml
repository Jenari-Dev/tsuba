import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Mouse function: buttons, scrolling (optionally repeating while held), and a button click
// together with keyboard keys held (Ctrl+click and friends, played by a small macro).
ColumnLayout {
    id: ed

    required property var panel
    // Repeat while the key / button is held (firmware "turbo"), with the rate per second.
    property bool turbo: false
    property int turboRate: 7
    // The click-with-keys builder.
    property int comboMods: 0
    property int comboKey: 0
    property int comboButton: 0
    readonly property var buttonNames: [i18n("Left click"), i18n("Right click"), i18n("Middle click"), i18n("Mouse button 4"), i18n("Mouse button 5"), i18n("Scroll up"), i18n("Scroll down")]

    spacing: 6

    function turboData(code) {
        const ms = Math.round(1000 / Math.max(1, turboRate));
        return [code, ms >> 8, ms & 255];
    }
    // Take the repeat setting from what the key does now.
    function syncFromAssignment() {
        const a = panel.assignment;
        if (a && a.function === 14 && a.data.length >= 3) {
            turbo = true;
            const ms = (a.data[1] << 8) | a.data[2];
            if (ms > 0) {
                turboRate = Math.max(1, Math.min(30, Math.round(1000 / ms)));
            }
        } else {
            turbo = false;
        }
    }
    Connections {
        target: ed.panel
        function onAssignmentChanged() {
            ed.syncFromAssignment();
        }
    }
    Component.onCompleted: syncFromAssignment()

    // Keys of the combo: modifiers down, optional key down, the click, then back up.
    function comboEvents() {
        const mods = [[1, 0xe0], [2, 0xe1], [4, 0xe2], [8, 0xe3]].filter(m => comboMods & m[0]).map(m => m[1]);
        const ev = mods.map(h => ({ type: "key", hid: h, down: true }));
        if (comboKey > 0) {
            ev.push({ type: "key", hid: comboKey, down: true });
        }
        ev.push({ type: "delay", ms: 15 });
        if (comboButton >= 5) {
            ev.push({ type: "scroll", up: comboButton === 5 });
        } else {
            ev.push({ type: "button", button: comboButton + 1, down: true }, { type: "delay", ms: 15 }, { type: "button", button: comboButton + 1, down: false });
        }
        ev.push({ type: "delay", ms: 15 });
        if (comboKey > 0) {
            ev.push({ type: "key", hid: comboKey, down: false });
        }
        mods.slice().reverse().forEach(h => ev.push({ type: "key", hid: h, down: false }));
        return ev;
    }
    function stageCombo() {
        if (comboMods === 0 && comboKey === 0) {
            return;
        }
        const name = KeyNames.chord(comboMods, comboKey) + " + " + buttonNames[comboButton];
        const events = comboEvents();
        panel.stage("combo", name, () => panel.assignComboMacro(name, events));
    }
    function pick(id, label, fn, code) {
        panel.stage(id, label, () => (turbo && code !== undefined) ? panel.save(14, turboData(code)) : fn());
    }

    SectionHeading { text: i18n("BUTTONS") }
    Repeater {
        model: [[i18n("Left click"), 1], [i18n("Right click"), 2], [i18n("Scroll click"), 3], [i18n("Mouse button 4"), 4], [i18n("Mouse button 5"), 5], [i18n("Double click"), -1]]
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            on: !!ed.panel.assignment && (modelData[1] === -1 ? ed.panel.assignment.function === 11 : ([1, 14].indexOf(ed.panel.assignment.function) >= 0 && ed.panel.assignment.data[0] === modelData[1]))
            chosen: ed.panel.stagedId === "mouse:" + modelData[1]
            onPicked: ed.pick("mouse:" + modelData[1], modelData[0], () => modelData[1] === -1 ? ed.panel.save(11, [1]) : ed.panel.save(1, [modelData[1]]), modelData[1] === -1 ? undefined : modelData[1])
        }
    }

    SectionHeading { text: i18n("SCROLLING") }
    Repeater {
        model: [[i18n("Scroll up"), 9], [i18n("Scroll down"), 10], [i18n("Scroll left"), 104], [i18n("Scroll right"), 105]]
        delegate: OptionRow {
            required property var modelData
            objectName: "scroll" + modelData[1]
            label: modelData[0]
            on: !!ed.panel.assignment && [1, 14].indexOf(ed.panel.assignment.function) >= 0 && ed.panel.assignment.data[0] === modelData[1]
            chosen: ed.panel.stagedId === "mouse:" + modelData[1]
            onPicked: ed.pick("mouse:" + modelData[1], ed.turbo ? i18n("%1 (repeating)", modelData[0]) : modelData[0], () => ed.panel.save(1, [modelData[1]]), modelData[1])
        }
    }
    RowLayout {
        spacing: 8
        SynCheckBox {
            objectName: "repeatHeld"
            text: i18n("Repeat while held")
            checked: ed.turbo
            onToggled: {
                ed.turbo = checked;
                ed.panel.clearStage();
            }
        }
        QQC2.SpinBox {
            objectName: "repeatRate"
            enabled: ed.turbo
            from: 1
            to: 30
            value: ed.turboRate
            onValueModified: {
                ed.turboRate = value;
                ed.panel.clearStage();
            }
        }
        Text {
            text: i18n(i18n("per second"))
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
    }
    Text {
        Layout.fillWidth: true
        text: i18n("Pick a button or scroll direction after choosing the repeat rate.")
        visible: ed.turbo
        color: Theme.textMuted
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }

    SectionHeading { text: i18n("CLICK WITH KEYS HELD") }
    Text {
        Layout.fillWidth: true
        text: i18n("For Ctrl+click, Shift+click and the like. Saved as a small macro that presses the keys, clicks and lets go.")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
    RowLayout {
        spacing: 2
        Repeater {
            model: [["Ctrl", 1], ["Shift", 2], ["Alt", 4], ["Meta", 8]]
            delegate: SynCheckBox {
                required property var modelData
                objectName: "comboMod" + modelData[0]
                text: modelData[0]
                checked: (ed.comboMods & modelData[1]) !== 0
                onToggled: {
                    ed.comboMods = checked ? ed.comboMods | modelData[1] : ed.comboMods & ~modelData[1];
                    ed.stageCombo();
                }
            }
        }
    }
    SynDropdown {
        objectName: "comboButton"
        Layout.fillWidth: true
        model: ed.buttonNames
        currentIndex: ed.comboButton
        onActivated: i => {
            ed.comboButton = i;
            ed.stageCombo();
        }
    }
    RowLayout {
        spacing: 8
        Text {
            text: ed.comboKey > 0 ? i18n("and hold %1", KeyNames.hidName(ed.comboKey)) : i18n("and hold a key (optional)")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        SynButton {
            visible: ed.comboKey > 0
            text: i18n("Clear")
            implicitWidth: 60
            onClicked: {
                ed.comboKey = 0;
                ed.stageCombo();
            }
        }
    }
    KeyPicker {
        objectName: "comboKeyPicker"
        Layout.fillWidth: true
        showModifiers: false
        onPicked: (data, usage) => {
            ed.comboKey = usage;
            ed.stageCombo();
        }
    }
}
