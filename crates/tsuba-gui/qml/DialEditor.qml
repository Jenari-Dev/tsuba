import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

// The keyboard's command dial: which modes it has, their order (press the dial to step through
// them), their colours and whether each is on. Modes move with ▲ ▼ or by dragging the grip; the
// whole table can go back to the stock order.
SynCard {
    id: ed

    required property var backend
    required property string pid
    property int profile: 1
    property var st: ({})
    property bool readOnly: false
    signal changed()

    property string result: ""
    property bool confirmReset: false
    readonly property int rowHeight: 50
    readonly property var slots: st.dial || []

    // Dial press (key position 169, standard layer): warn when it no longer switches modes.
    readonly property int dialKey: 169
    property var dialAssign: null
    readonly property var dialDefault: KeyNames.defaultFor(false, dialKey, 0)
    readonly property bool dialRemapped: {
        const a = dialAssign;
        if (!a) {
            return false;
        }
        const d = dialDefault;
        if (d) {
            return a.function !== d.function || JSON.stringify(a.data) !== JSON.stringify(d.data);
        }
        // Default unknown: a RazerKey (17) outside the dynamic-action range counts as untouched.
        return a.function !== 17 || (a.data[0] >= 0x80 && a.data[0] <= 0x9f);
    }

    title: "Command dial"
    help: "The dial on top of the keyboard has several modes (for example volume, brightness, scroll). Pressing it cycles the mode; turning it acts on the current mode. Drag a mode (or use the arrows) to change the order the dial steps through. You can re-assign the dial press from Customize, which stops it switching modes."
    description: "Press the dial to step to the next mode (Shift + press goes back). The dial's LED shows the active mode's colour. Click a mode to make it active; switch modes off to skip them."

    LibProvider {
        id: provider
    }
    // Tests can inject the helper.
    property alias lp: provider

    function loadDial() {
        if (!st.dial) {
            dialAssign = null;
            return;
        }
        const s = backend.assignments(pid, profile, 0);
        const a = s === "" ? null : JSON.parse(s).find(x => x.key === dialKey);
        if (a) {
            const byKey = {};
            JSON.parse(backend.actions(pid) || "[]").forEach(x => {
                if (x.profile === profile && x.layer === 0) {
                    byKey[x.key] = x.label;
                }
            });
            KeyNames.currentPid = pid;
            KeyNames.actionByKey = byKey;
        }
        dialAssign = a || null;
    }
    Component.onCompleted: loadDial()
    onStChanged: loadDial()
    onProfileChanged: loadDial()
    Connections {
        target: Theme
        function onDataRevChanged() {
            ed.loadDial();
        }
    }

    function resetDialPress() {
        const d = dialDefault;
        if (!d) {
            return;
        }
        const r = backend.set_assignment(pid, JSON.stringify({ profile: profile, key: dialKey, layer: 0, function: d.function, data: d.data }));
        result = r === "" ? "Failed: " + backend.status : "Dial press reset to default";
        loadDial();
        changed();
    }

    function plain() {
        return slots.map(d => ({ function: d.function, colour: d.colour, enabled: d.enabled }));
    }
    // Write the slot table (keeping the mode that is active now active, wherever it ended up).
    function write(list, text) {
        if (!list.some(d => d.enabled)) {
            result = "Failed: at least one dial mode must stay on";
            return false;
        }
        const cur = slots.find(d => d.current);
        if (!backend.set_dial(pid, JSON.stringify(list))) {
            result = "Failed: " + backend.status;
            return false;
        }
        result = text;
        if (cur) {
            const i = list.findIndex(d => d.function === cur.function);
            if (i >= 0 && list[i].enabled) {
                backend.set_dial_mode(pid, i);
            }
        }
        changed();
        return true;
    }
    // Rewrite all dial slots with one slot changed ({colour} and/or {enabled}).
    function editDial(slot, change) {
        const list = plain();
        Object.assign(list[slot], change);
        write(list, "Command dial saved");
    }
    // Move the mode in slot `from` to slot `to`.
    function moveMode(from, to) {
        if (from === to || to < 0 || to >= slots.length) {
            return;
        }
        const list = plain();
        list.splice(to, 0, list.splice(from, 1)[0]);
        write(list, "Dial order saved");
    }
    function resetModes() {
        confirmReset = false;
        const list = provider.json("FactoryDial", [pid], null);
        if (!list) {
            result = "Failed: " + (provider.error !== "" ? provider.error : "the stock table isn't available");
            return;
        }
        write(list, "Dial modes are back to the stock order");
    }

    ColorDialog {
        id: dialPicker
        property int slot: 0
        title: "Dial mode colour"
        onAccepted: ed.editDial(slot, { colour: selectedColor.toString().replace("#", "").slice(-6) })
    }

    RowLayout {
        objectName: "dialRemapWarning"
        visible: ed.dialRemapped
        Layout.fillWidth: true
        spacing: 12
        Text {
            Layout.fillWidth: true
            text: "The dial press is remapped to " + KeyNames.describe(ed.dialAssign) + " — pressing the dial runs that instead of switching modes."
            color: Theme.accent
            font.family: Theme.font
            font.pixelSize: Theme.small
            wrapMode: Text.Wrap
        }
        Item {
            implicitWidth: resetDialBtn.implicitWidth
            implicitHeight: resetDialBtn.implicitHeight
            SynButton {
                id: resetDialBtn
                objectName: "resetDialPress"
                anchors.fill: parent
                text: "Reset dial press"
                enabled: !ed.readOnly && ed.dialDefault !== null
                onClicked: ed.resetDialPress()
            }
            HoverHandler { id: resetDialHover }
            QQC2.ToolTip {
                visible: resetDialHover.hovered && ed.dialDefault === null
                text: KeyNames.defaultUnknownTip
            }
        }
    }

    // The modes, in the order the dial steps through them.
    Item {
        id: list
        objectName: "dialList"
        Layout.fillWidth: true
        Layout.preferredHeight: ed.slots.length * ed.rowHeight
        Repeater {
            model: ed.slots
            delegate: Rectangle {
                id: slotRow
                required property var modelData
                required property int index
                objectName: "dialSlot" + index
                width: list.width
                height: ed.rowHeight - 4
                y: index * ed.rowHeight
                z: grip.active ? 10 : 0
                radius: 3
                color: modelData.current ? Theme.accentTint : (dh.hovered ? "#14ffffff" : Theme.bg)
                border.color: grip.active ? Theme.accent : (modelData.current ? Theme.accent : Theme.borderSoft)
                transform: Translate { y: grip.active ? grip.translation.y : 0 }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 12
                    spacing: 10
                    // Grip: drag to another place.
                    Text {
                        objectName: "dialGrip" + slotRow.index
                        text: "⠿"
                        color: grip.active || gh.hovered ? Theme.accent : Theme.textMuted
                        font.pixelSize: 18
                        Layout.preferredWidth: 16
                        HoverHandler {
                            id: gh
                            cursorShape: ed.readOnly ? Qt.ArrowCursor : Qt.OpenHandCursor
                        }
                        DragHandler {
                            id: grip
                            enabled: !ed.readOnly
                            target: null
                            yAxis.enabled: true
                            xAxis.enabled: false
                            onActiveChanged: {
                                if (!active) {
                                    const to = Math.max(0, Math.min(ed.slots.length - 1, slotRow.index + Math.round(translation.y / ed.rowHeight)));
                                    ed.moveMode(slotRow.index, to);
                                }
                            }
                        }
                    }
                    Column {
                        spacing: 0
                        Text {
                            objectName: "dialUp" + slotRow.index
                            text: "▲"
                            font.pixelSize: 10
                            color: slotRow.index === 0 || ed.readOnly ? Theme.borderSoft : (uh.hovered ? Theme.accent : Theme.textDim)
                            HoverHandler { id: uh }
                            TapHandler {
                                enabled: slotRow.index > 0 && !ed.readOnly
                                onTapped: ed.moveMode(slotRow.index, slotRow.index - 1)
                            }
                        }
                        Text {
                            objectName: "dialDown" + slotRow.index
                            text: "▼"
                            font.pixelSize: 10
                            color: slotRow.index === ed.slots.length - 1 || ed.readOnly ? Theme.borderSoft : (dnh.hovered ? Theme.accent : Theme.textDim)
                            HoverHandler { id: dnh }
                            TapHandler {
                                enabled: slotRow.index < ed.slots.length - 1 && !ed.readOnly
                                onTapped: ed.moveMode(slotRow.index, slotRow.index + 1)
                            }
                        }
                    }
                    Rectangle {
                        Layout.preferredWidth: 22
                        Layout.preferredHeight: 22
                        radius: 11
                        color: "#" + slotRow.modelData.colour
                        border.color: sw.hovered ? Theme.textBright : Theme.border
                        opacity: slotRow.modelData.enabled ? 1 : 0.35
                        HoverHandler {
                            id: sw
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            enabled: !ed.readOnly
                            onTapped: {
                                dialPicker.slot = slotRow.index;
                                dialPicker.selectedColor = "#" + slotRow.modelData.colour;
                                dialPicker.open();
                            }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: slotRow.modelData.function.replace(/([a-z])([A-Z])/g, "$1 $2")
                        color: slotRow.modelData.enabled ? Theme.text : Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    Text {
                        visible: slotRow.modelData.current
                        text: "ACTIVE"
                        color: Theme.accent
                        font.family: Theme.font
                        font.pixelSize: 11
                        font.letterSpacing: 1
                    }
                    SynToggle {
                        checked: slotRow.modelData.enabled
                        enabled: !ed.readOnly
                        onToggled: on => ed.editDial(slotRow.index, { enabled: on })
                    }
                }
                HoverHandler {
                    id: dh
                    cursorShape: slotRow.modelData.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                }
                TapHandler {
                    enabled: slotRow.modelData.enabled && !slotRow.modelData.current && !ed.readOnly
                    onTapped: {
                        if (ed.backend.set_dial_mode(ed.pid, slotRow.index)) {
                            ed.result = slotRow.modelData.function.replace(/([a-z])([A-Z])/g, "$1 $2") + " is the active dial mode";
                            ed.changed();
                        } else {
                            ed.result = "Failed: " + ed.backend.status;
                        }
                    }
                }
            }
        }
    }

    RowLayout {
        spacing: 10
        SynButton {
            objectName: "resetDialModes"
            visible: !ed.confirmReset
            text: "Reset modes to stock"
            enabled: !ed.readOnly
            onClicked: ed.confirmReset = true
        }
        Text {
            visible: ed.confirmReset
            text: "Put the modes back in the stock order and switch the stock ones on?"
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        SynButton {
            visible: ed.confirmReset
            text: "Cancel"
            implicitWidth: 80
            onClicked: ed.confirmReset = false
        }
        SynButton {
            objectName: "confirmResetDial"
            visible: ed.confirmReset
            text: "Reset"
            primary: true
            implicitWidth: 80
            onClicked: ed.resetModes()
        }
    }
    Text {
        objectName: "dialResult"
        visible: ed.result !== ""
        text: ed.result
        color: ed.result.startsWith("Failed") ? Theme.warn : Theme.accent
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
}
