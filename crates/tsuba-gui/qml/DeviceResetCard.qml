import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Reset the selected on-board profile, or the whole device, to factory settings. Both need a
// typed confirmation (the daemon checks the phrase too). `host`: the Performance tab (devb,
// feat, pid, profile, readOnly, result).
SynCard {
    id: card

    required property var host
    // i18n() comes from KLocalizedContext in the app; the headless QML tests run without it.
    function tr(translated, plain) {
        return typeof i18n === "function" ? translated() : plain;
    }
    readonly property string profilePhrase: host.feat.profile_reset_phrase || ("reset profile " + host.profile)
    readonly property string devicePhrase: host.feat.factory_reset_phrase || ""

    signal resetDone()

    function finish(r) {
        const why = host.devb ? host.devb.status : "";
        host.result = r === "" ? tr(() => i18n("Failed: %1", why), "Failed: " + why) : r;
        if (r !== "") {
            Theme.dataRev++;
            resetDone();
        }
    }

    objectName: "deviceResetCard"
    title: i18n("Reset")
    help: i18n("Resetting a profile puts the key assignments, sensitivity and other settings stored in that on-board profile back to the factory defaults. A factory reset erases every on-board profile and macro of the device. Your profiles in Tsuba's library are kept either way; activate one again to put its settings back on the device.")

    RowLayout {
        spacing: 12
        SynButton {
            objectName: "resetProfileButton"
            visible: !!card.host.feat.can_reset_profile
            text: i18n("Reset profile %1…", card.host.profile)
            enabled: !card.host.readOnly
            onClicked: {
                profileField.text = "";
                profileDialog.open();
            }
        }
        SynButton {
            objectName: "factoryResetButton"
            visible: card.devicePhrase !== ""
            text: i18n("Factory reset…")
            enabled: !card.host.readOnly
            onClicked: {
                deviceField.text = "";
                understood.checked = false;
                deviceDialog.open();
            }
        }
    }

    component Body: Text {
        Layout.fillWidth: true
        Layout.preferredWidth: 420
        color: Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.fontSize
        wrapMode: Text.Wrap
    }

    QQC2.Dialog {
        id: profileDialog
        objectName: "resetProfileDialog"
        parent: QQC2.Overlay.overlay
        x: parent ? (parent.width - width) / 2 : 0
        y: parent ? (parent.height - height) / 3 : 0
        modal: true
        title: i18n("Reset on-board profile %1", card.host.profile)
        ColumnLayout {
            spacing: 12
            Body {
                text: i18n("This puts on-board profile %1 back to its factory settings. It can't be undone on the device.", card.host.profile)
            }
            Body {
                text: i18n("Type \"%1\" to confirm:", card.profilePhrase)
            }
            QQC2.TextField {
                id: profileField
                objectName: "resetProfileField"
                Layout.fillWidth: true
                placeholderText: card.profilePhrase
            }
            RowLayout {
                spacing: 12
                SynButton {
                    objectName: "resetProfileConfirm"
                    text: i18n("Reset profile")
                    primary: true
                    enabled: profileField.text === card.profilePhrase
                    onClicked: {
                        profileDialog.close();
                        card.finish(card.host.devb.reset_profile(card.host.pid, card.host.profile, profileField.text));
                    }
                }
                SynButton {
                    text: i18n("Cancel")
                    onClicked: profileDialog.close()
                }
            }
        }
    }

    QQC2.Dialog {
        id: deviceDialog
        objectName: "factoryResetDialog"
        parent: QQC2.Overlay.overlay
        x: parent ? (parent.width - width) / 2 : 0
        y: parent ? (parent.height - height) / 3 : 0
        modal: true
        title: i18n("Factory reset")
        ColumnLayout {
            spacing: 12
            Body {
                text: i18n("This erases every on-board profile and macro of %1 and restores its factory settings. It can't be undone on the device.", card.devicePhrase)
                color: Theme.warn
            }
            SynCheckBox {
                id: understood
                objectName: "factoryResetUnderstood"
                text: i18n("I understand the on-board profiles and macros will be erased")
            }
            Body {
                text: i18n("Type the device name \"%1\" to confirm:", card.devicePhrase)
            }
            QQC2.TextField {
                id: deviceField
                objectName: "factoryResetField"
                Layout.fillWidth: true
                enabled: understood.checked
                placeholderText: card.devicePhrase
            }
            RowLayout {
                spacing: 12
                SynButton {
                    objectName: "factoryResetConfirm"
                    text: i18n("Erase and reset")
                    primary: true
                    enabled: understood.checked && deviceField.text === card.devicePhrase
                    onClicked: {
                        deviceDialog.close();
                        card.finish(card.host.devb.factory_reset(card.host.pid, deviceField.text));
                    }
                }
                SynButton {
                    text: i18n("Cancel")
                    onClicked: deviceDialog.close()
                }
            }
        }
    }
}
