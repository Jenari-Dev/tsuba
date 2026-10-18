import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Text function. Letters are typed with the desktop's keyboard layout (a German or French board
// gets the right keys), as an on-board macro that works without Tsuba running. Characters no key
// types on that layout (emoji) make Tsuba type the text itself instead, while the service runs.
ColumnLayout {
    id: ed

    required property var panel
    property var layouts: []
    property int layoutIdx: 0
    property string text: ""
    property int delay: 10
    // What the last preview found: {layout, skipped}.
    property var plan: ({ layout: "", skipped: "" })

    spacing: 8

    function load() {
        const l = panel.lp.json("TextLayouts", [], { layouts: [], active: "" });
        layouts = l.layouts;
        layoutIdx = 0;
    }
    // Read the lists (applications, audio devices, layouts) only once the editor is shown.
    property bool loaded: false
    function ensureLoaded() {
        if (!loaded) {
            loaded = true;
            load();
        }
    }
    onVisibleChanged: if (visible) ensureLoaded()
    Component.onCompleted: if (visible) ensureLoaded()

    function layoutName() {
        return layouts.length > 0 ? layouts[Math.min(layoutIdx, layouts.length - 1)] : "";
    }
    // Key events for the text on the chosen layout (the US table when the daemon can't say).
    function events() {
        const r = panel.lp.json("TextToEvents", [text, String(delay), layoutName()], null);
        if (r) {
            return r;
        }
        const us = KeyNames.textToEvents(text, delay);
        return { events: us.events, skipped: us.skipped, layout: i18n("US (built in)") };
    }
    function restage() {
        if (text.length === 0) {
            panel.clearStage();
            plan = { layout: "", skipped: "" };
            return;
        }
        const r = events();
        plan = { layout: r.layout, skipped: r.skipped };
        const label = i18n("Type “%1”", text.length > 24 ? text.slice(0, 24) + "…" : text.replace(/\n/g, " "));
        panel.stage("text", label, () => r.skipped.length > 0 || r.events.length === 0
            ? panel.assignAction({ type: "type", text: text })
            : panel.assignEventsMacro(i18n("Text: %1", text.slice(0, 24).replace(/\n/g, " ")), r.events));

    }

    QQC2.TextField {
        id: textInput
        objectName: "textInput"
        Layout.fillWidth: true
        placeholderText: i18n("Text to type")
        onTextEdited: {
            ed.text = text;
            ed.restage();
        }
    }
    RowLayout {
        spacing: 8
        Text {
            text: i18n("Delay between characters (ms)")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        QQC2.SpinBox {
            objectName: "textDelay"
            from: 0
            to: 500
            value: ed.delay
            editable: true
            onValueModified: {
                ed.delay = value;
                ed.restage();
            }
        }
    }
    RowLayout {
        visible: ed.layouts.length > 1
        spacing: 8
        Text {
            text: i18n("Keyboard layout")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        SynDropdown {
            objectName: "textLayout"
            Layout.fillWidth: true
            model: ed.layouts
            currentIndex: ed.layoutIdx
            onActivated: i => {
                ed.layoutIdx = i;
                ed.restage();
            }
        }
    }
    Text {
        objectName: "textPlan"
        visible: ed.text.length > 0
        Layout.fillWidth: true
        text: ed.plan.skipped.length > 0
            ? i18n("“%1” can't be typed with keys on the %2 layout, so Tsuba types this text itself while its service runs (needs wtype, ydotool or xdotool).", ed.plan.skipped, ed.plan.layout)
            : i18n("Typed as an on-board macro using the %1 layout; it works without Tsuba running.", ed.plan.layout)
        color: ed.plan.skipped.length > 0 ? Theme.warn : Theme.textMuted
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
}
