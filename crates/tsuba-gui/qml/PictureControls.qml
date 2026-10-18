import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs

// "Use my own picture" for one device: pick an image, line it up over Tsuba's drawing (the
// buttons / keys / LEDs stay where the drawing has them), save. "Remove my picture" goes back
// to Tsuba's drawing (or to a photo pack that was set aside).
ColumnLayout {
    id: ctl

    property var backend: null
    property string pid: ""
    property string deviceName: ""
    signal changed()

    spacing: 8

    // Pack JSON of Tsuba's own drawing (null if the device has none).
    readonly property var builtin: {
        const s = backend ? backend.builtin_pack(pid) : "";
        return s === "" ? null : JSON.parse(s);
    }
    readonly property bool hasBottom: !!builtin && !!builtin.views.bottom
    property int rev: 0
    readonly property bool custom: { rev; Theme.picRev; return !!backend && backend.has_custom_picture(pid); }
    property string view: "top"
    property string pending: ""
    property string note: ""
    property string label: ""         // compact inline layout: label, buttons and note on one row

    function chooseOwn() {
        view = "top";
        dlg.open();
    }
    function beginAlign(viewName, url) {
        if (!builtin || !builtin.views[viewName]) {
            return;
        }
        view = viewName;
        pending = url;
        const v = builtin.views[viewName];
        align.drawing = builtin.dir + v.image;
        align.frameW = v.width;
        align.frameH = v.height;
        align.source = url;
        align.reset();
        overlay.visible = true;
    }
    function save(x, y, w, h) {
        const ok = backend.set_custom_picture(pid, view, pending) && backend.set_picture_rect(pid, view, x, y, w, h);
        overlay.visible = false;
        if (!ok) {
            note = backend.status;
            return;
        }
        note = "Saved.";
        // Custom pictures count as "My picture pack"; make sure it is the one shown.
        Theme.prefs.art = "auto";
        refresh.restart();
    }
    function remove() {
        note = backend.remove_custom_picture(pid) ? "Back to Tsuba's drawing." : backend.status;
        refresh.restart();
    }
    // The app reads the art setting from its ini file, which QML's Settings writes a moment later.
    Timer {
        id: refresh
        interval: 300
        onTriggered: {
            ctl.rev++;
            Theme.picRev++;
            ctl.changed();
        }
    }

    Text {
        Layout.fillWidth: true
        visible: ctl.deviceName !== ""
        text: ctl.deviceName
        color: Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    Flow {
        spacing: 10
        Layout.fillWidth: true
        Text {
            visible: ctl.label !== ""
            height: 32
            verticalAlignment: Text.AlignVCenter
            text: ctl.label.toUpperCase()
            color: Theme.accent
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            rightPadding: 6
        }
        SynButton {
            objectName: "useOwnPicture"
            text: "Use my own picture…"
            enabled: !!ctl.builtin
            onClicked: ctl.chooseOwn()
        }
        SynButton {
            objectName: "useOwnUnderside"
            visible: ctl.hasBottom
            text: "Underside picture…"
            onClicked: {
                ctl.view = "bottom";
                dlg.open();
            }
        }
        SynButton {
            objectName: "removeOwnPicture"
            text: "Remove my picture"
            enabled: ctl.custom
            onClicked: ctl.remove()
        }
    }
    Text {
        Layout.fillWidth: true
        visible: ctl.note !== ""
        text: ctl.note
        color: ctl.note === "Saved." || ctl.note.indexOf("Back") === 0 ? Theme.accent : Theme.warn
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.WordWrap
    }

    FileDialog {
        id: dlg
        title: "Choose a picture"
        nameFilters: ["Pictures (*.png *.jpg *.jpeg *.webp *.svg)"]
        onAccepted: ctl.beginAlign(ctl.view, selectedFile.toString())
    }

    // Alignment step: a dimmed full-window layer above the page.
    Item {
        id: overlay
        objectName: "alignOverlay"
        parent: Window.contentItem
        visible: false
        anchors.fill: parent
        z: 1000
        Rectangle {
            anchors.fill: parent
            color: "#cc000000"
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.AllButtons
                onWheel: w => w.accepted = true
            }
        }
        PictureAlign {
            id: align
            anchors.centerIn: parent
            width: Math.min(parent.width - 80, 1000)
            height: Math.min(parent.height - 80, 760)
            title: "Line up your picture" + (ctl.view === "bottom" ? " (underside)" : "")
            onAccepted: (x, y, w, h) => ctl.save(x, y, w, h)
            onCancelled: overlay.visible = false
        }
    }

    Component.onCompleted: {
        const t = backend ? backend.env_var("TSUBA_ALIGN") : "";
        // headless UI check: a picture file to line up
        if (t !== "" && pid === backend.env_var("TSUBA_DEVICE")) {
            beginAlign(backend.env_var("TSUBA_ALIGN_VIEW") || "top", "file://" + t);
            const r = backend.env_var("TSUBA_ALIGN_SAVE");   // "x,y,w,h": save straight away
            if (r !== "") {
                const v = r.split(",").map(Number);
                save(v[0], v[1], v[2], v[3]);
            }
        }
    }
}
