import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// "Scan for games": the installed games found by the backend as cover cards with checkboxes, a
// search filter and "Add selected". A full-page overlay (not a Popup) so it renders everywhere.
Rectangle {
    id: dlg

    property var games: []          // [{source, id, name, install_dir, art, exe}]
    property var links: []          // current linked games, to mark the ones already added
    property var picked: ({})       // key -> true
    property string filter: ""
    property bool scanned: false

    signal addSelected(var chosen)
    signal closed

    color: Theme.scrim
    visible: false

    // What a link created from this game matches on (folder when known, else the exe name).
    function matchFor(g) {
        return g.install_dir !== "" ? "dir:" + g.install_dir : (g.exe || "");
    }
    function keyOf(g) {
        return g.source + ":" + g.id;
    }
    function isLinked(g) {
        const m = matchFor(g).toLowerCase();
        return dlg.links.some(l => (m !== "" && (l.matches || "").toLowerCase() === m) || (l.name || "").toLowerCase() === g.name.toLowerCase());
    }
    readonly property var shown: games.filter(g => filter === "" || g.name.toLowerCase().indexOf(filter.toLowerCase()) >= 0)
    readonly property int pickedCount: Object.keys(picked).length

    function open(list) {
        games = list;
        picked = ({});
        filter = "";
        search.text = "";
        scanned = true;
        visible = true;
    }
    function toggle(g, on) {
        const p = Object.assign({}, picked);
        if (on) {
            p[keyOf(g)] = true;
        } else {
            delete p[keyOf(g)];
        }
        picked = p;
    }
    function selectAllShown() {
        const p = Object.assign({}, picked);
        shown.forEach(g => {
            if (!isLinked(g)) {
                p[keyOf(g)] = true;
            }
        });
        picked = p;
    }
    function chosen() {
        return games.filter(g => picked[keyOf(g)]);
    }

    // Swallow clicks on the dimmed area so the page below stays untouched.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onWheel: w => w.accepted = true
    }
    Keys.onEscapePressed: dlg.closed()

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(960, dlg.width - 60)
        height: Math.min(680, dlg.height - 50)
        color: Theme.panel
        border.color: Theme.borderSoft
        radius: 5

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 12

            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Text {
                    text: i18n("SCAN FOR GAMES")
                    color: Theme.accent
                    font.family: Theme.titleFont
                    font.pixelSize: 16
                }
                Text {
                    text: i18np("%1 game found", "%1 games found", dlg.games.length)
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                Item {
                    Layout.fillWidth: true
                }
                QQC2.TextField {
                    id: search
                    objectName: "gameSearch"
                    Layout.preferredWidth: 240
                    placeholderText: i18n("Search games")
                    onTextChanged: dlg.filter = text.trim()
                }
                SynButton {
                    text: i18n("Select all")
                    Layout.minimumWidth: 100
                    onClicked: dlg.selectAllShown()
                }
            }

            GridView {
                id: grid
                objectName: "gameGrid"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                readonly property int cols: Math.max(1, Math.floor(width / 156))
                cellWidth: Math.floor(width / cols)
                cellHeight: 268
                model: dlg.shown
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                delegate: Item {
                    id: cell
                    required property var modelData
                    readonly property bool linked: dlg.isLinked(modelData)
                    readonly property bool on: !!dlg.picked[dlg.keyOf(modelData)]
                    width: grid.cellWidth
                    height: grid.cellHeight
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 6
                        color: cell.on ? Theme.accentTint : (ch.hovered && !cell.linked ? Theme.hoverTint : Theme.bg)
                        border.color: cell.on ? Theme.accent : Theme.borderSoft
                        opacity: cell.linked ? 0.55 : 1
                        // Cover (2:3), or the first letter when no art is cached.
                        Rectangle {
                            id: cover
                            x: 8
                            y: 8
                            width: parent.width - 16
                            height: width * 1.4
                            color: Theme.panelRaised
                            clip: true
                            Image {
                                anchors.fill: parent
                                source: modelData.art
                                fillMode: Image.PreserveAspectCrop
                                sourceSize.width: 320
                                sourceSize.height: 450
                                asynchronous: true
                                smooth: true
                            }
                            Text {
                                visible: modelData.art === ""
                                anchors.centerIn: parent
                                text: modelData.name.charAt(0).toUpperCase()
                                color: Theme.textMuted
                                font.family: Theme.font
                                font.pixelSize: 40
                            }
                            Rectangle {
                                x: 6
                                y: 6
                                width: badge.implicitWidth + 12
                                height: 18
                                radius: 2
                                color: "#cc000000"   // device drawing: intentional (badge over cover art)
                                Text {
                                    id: badge
                                    anchors.centerIn: parent
                                    text: (cell.linked ? i18n("LINKED") : modelData.source.toUpperCase())
                                    color: cell.linked ? Theme.defaultAccent : "#ffffff"   // badge sits on dark scrim over art: intentional
                                    font.family: Theme.font
                                    font.pixelSize: 10
                                    font.letterSpacing: 1
                                }
                            }
                        }
                        RowLayout {
                            x: 8
                            y: cover.y + cover.height + 6
                            width: parent.width - 16
                            spacing: 6
                            SynCheckBox {
                                enabled: !cell.linked
                                checked: cell.on
                                focusPolicy: Qt.NoFocus
                                onToggled: dlg.toggle(modelData, checked)
                            }
                            Text {
                                Layout.fillWidth: true
                                textFormat: Text.PlainText
                                text: modelData.name
                                color: Theme.textBright
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                Layout.preferredHeight: 32
                                verticalAlignment: Text.AlignVCenter
                                clip: true
                            }
                        }
                        HoverHandler {
                            id: ch
                            cursorShape: cell.linked ? Qt.ArrowCursor : Qt.PointingHandCursor
                        }
                        TapHandler {
                            enabled: !cell.linked
                            onTapped: dlg.toggle(modelData, !cell.on)
                        }
                    }
                }
                Text {
                    anchors.centerIn: parent
                    visible: dlg.shown.length === 0
                    width: parent.width - 60
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    text: dlg.games.length === 0 ? i18n("No installed games were found. Tsuba looks for Steam, Lutris, Heroic and Bottles; you can still add any game by hand.") : i18n("No game matches “%1”.", dlg.filter)
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Text {
                    Layout.fillWidth: true
                    text: dlg.pickedCount > 0 ? i18np("%1 game selected", "%1 games selected", dlg.pickedCount) : i18n("Pick the games to link. Each is recognised by its install folder while it runs.")
                    color: dlg.pickedCount > 0 ? Theme.accent : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    elide: Text.ElideRight
                }
                SynButton {
                    objectName: "gameScanClose"
                    text: i18nc("button: close dialog", "Close")
                    Layout.minimumWidth: 90
                    onClicked: dlg.closed()
                }
                SynButton {
                    objectName: "gameScanAdd"
                    text: i18n("Add selected")
                    primary: true
                    Layout.minimumWidth: 130

                    enabled: dlg.pickedCount > 0
                    onClicked: dlg.addSelected(dlg.chosen())
                }
            }
        }
    }
}
