import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Every model in Tsuba's device database, grouped by kind and searchable, with what the
// database knows about the selected one. Read-only; needs no device.
Rectangle {
    id: page

    required property var backend
    property var all: []
    property string query: ""
    property string selectedPid: ""
    property var sel: ({})
    property var selPack: null

    color: Theme.bg

    // The rows shown: filtered by the search, ordered by kind group then name; "group" feeds the section headings.
    readonly property var rows: {
        const q = query.trim().toLowerCase();
        const order = DeviceKinds.groupOrder;
        return all.filter(d => q === "" || d.name.toLowerCase().indexOf(q) >= 0 || d.pid.indexOf(q) >= 0
                || (DeviceKinds.groupLabels[d.kind] || "").toLowerCase().indexOf(q) >= 0
                || DeviceKinds.statusOf(d).toLowerCase().indexOf(q) >= 0)
            .map(d => Object.assign({ group: DeviceKinds.groupLabels[d.kind] || "Other", rank: order.indexOf(d.kind) < 0 ? 99 : order.indexOf(d.kind) }, d))
            .sort((a, b) => a.rank - b.rank || a.name.localeCompare(b.name));
    }

    function packOf(pid) {
        const j = backend.device_pack(pid);
        return j === "" ? null : JSON.parse(j);
    }
    function thumbOf(pid) {
        const p = packOf(pid);
        return p && p.views && p.views.top ? p.dir + p.views.top.image : "";
    }
    function choose(pid) {
        selectedPid = pid;
        const j = backend.device_support(pid);
        sel = j === "" ? ({}) : JSON.parse(j);
        selPack = packOf(pid);
    }
    function load() {
        const j = backend.known_devices();
        all = j === "" ? [] : JSON.parse(j);
    }

    Component.onCompleted: {
        load();
        const pre = backend.env_var("TSUBA_SUPPORTED_PID");   // headless UI check
        if (pre !== "") {
            choose(pre);
        }
        const q = backend.env_var("TSUBA_SUPPORTED_QUERY");
        if (q !== "") {
            query = q;
        }
    }

    component Badge: Rectangle {
        id: b
        property string status: ""
        implicitWidth: bt.implicitWidth + 14
        implicitHeight: 18
        radius: 9
        color: "transparent"
        border.color: status === "Tested" ? Theme.accent : Theme.borderSoft
        Text {
            id: bt
            anchors.centerIn: parent
            text: b.status.toUpperCase()
            color: b.status === "Tested" ? Theme.accent : (b.status === "Untested" ? Theme.textDim : Theme.textMuted)
            font.family: Theme.font
            font.pixelSize: 9
            font.letterSpacing: 0.5
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 24

        // ---- List
        ColumnLayout {
            Layout.preferredWidth: 440
            Layout.maximumWidth: 440
            Layout.fillWidth: false
            Layout.fillHeight: true
            spacing: 12

            Text {
                text: "SUPPORTED DEVICES"
                color: Theme.accent
                font.family: Theme.titleFont
                font.pixelSize: Theme.fontSize
            }
            Text {
                Layout.fillWidth: true
                text: page.all.length + " models. Tested means checked on real hardware; the rest come from community sources and start out read-only."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.WordWrap
            }
            QQC2.TextField {
                id: search
                objectName: "searchField"
                Layout.fillWidth: true
                placeholderText: "Search by name, type or ID"
                text: page.query
                onTextChanged: page.query = text
            }
            ListView {
                id: list
                objectName: "deviceList"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: page.rows
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                section.property: "group"
                section.delegate: Rectangle {
                    required property string section
                    width: ListView.view.width
                    height: 30
                    color: Theme.bg
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: parent.section.toUpperCase()
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        font.letterSpacing: 0.5
                    }
                }
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    readonly property string status: DeviceKinds.statusOf(modelData)
                    width: ListView.view.width - 10
                    height: 52
                    color: page.selectedPid === modelData.pid ? Theme.accentTint : (rh.hovered ? "#14ffffff" : Theme.panel)
                    border.color: page.selectedPid === modelData.pid ? Theme.accent : "transparent"
                    Image {
                        id: thumb
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        width: 64
                        height: 40
                        source: page.thumbOf(row.modelData.pid)
                        sourceSize.height: 80
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        smooth: true
                        mipmap: true
                        opacity: row.modelData.supported === false ? 0.5 : 1
                    }
                    Text {
                        anchors.left: thumb.right
                        anchors.leftMargin: 12
                        anchors.right: badge.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: row.modelData.name
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                        elide: Text.ElideRight
                    }
                    Badge {
                        id: badge
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        status: row.status
                    }
                    HoverHandler {
                        id: rh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: page.choose(row.modelData.pid)
                    }
                }
                Text {
                    anchors.centerIn: parent
                    visible: page.rows.length === 0
                    text: page.all.length === 0 ? "The device list isn't available (is tsubad running?)." : "No models match."
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                }
            }
        }

        // ---- Facts for the selected model
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.fillHeight: true
            color: Theme.card
            radius: 5

            Text {
                anchors.centerIn: parent
                visible: page.selectedPid === ""
                text: "Pick a model to see what Tsuba knows about it."
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }
            QQC2.ScrollView {
                anchors.fill: parent
                anchors.margins: 24
                visible: page.selectedPid !== ""
                contentWidth: availableWidth
                clip: true
                ColumnLayout {
                    width: parent.width
                    spacing: 14
                    RowLayout {
                        spacing: 16
                        Image {
                            Layout.preferredWidth: 200
                            Layout.preferredHeight: 140
                            source: page.thumbOf(page.selectedPid)
                            sourceSize.height: 280
                            fillMode: Image.PreserveAspectFit
                            smooth: true
                            mipmap: true
                        }
                        ColumnLayout {
                            spacing: 6
                            Text {
                                objectName: "detailName"
                                Layout.fillWidth: true
                                text: page.sel.name || ""
                                color: Theme.textBright
                                font.family: Theme.font
                                font.pixelSize: 18
                                wrapMode: Text.WordWrap
                            }
                            Badge {
                                status: DeviceKinds.statusOf(page.sel)
                            }
                            Text {
                                text: "Product ID " + page.selectedPid
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: page.sel.supported === false ? "Tsuba can't control this device yet."
                            : (page.sel.tested ? "Checked on real hardware." : "Not tested yet: when you plug it in, its settings are read-only until you allow changes.")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        visible: !!page.selPack && !!page.selPack.approximate
                        Layout.fillWidth: true
                        text: "The drawing is approximate — key positions may differ on the real device."
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.WordWrap
                    }
                    SupportFacts {
                        Layout.fillWidth: true
                        support: page.sel
                    }
                }
            }
        }
    }
}
