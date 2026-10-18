import QtQuick
import QtTest 1.2

Item {
    width: 1000
    height: 400
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property var calls: []
        property string copied: ""
        property bool fail: false
        function set_allow_untested_writes(pid, allow) {
            calls = calls.concat([[pid, allow]]);
            if (fail) {
                status = "daemon said no";
                return "";
            }
            return JSON.stringify({ pid: pid, name: "Huntsman Mini", kind: "keyboard", tested: false, supported: true, allow_untested_writes: allow, allow_writes: allow });
        }
        function copy_text(t) { copied = t; }
        function env_var(n) { return ""; }
    }
    property var sup: ({ pid: "0257", name: "Huntsman Mini", kind: "keyboard", layout: "60", tested: false, supported: true,
        allow_untested_writes: false, allow_writes: false, features: { onboard_macros: true, game_mode: true } })
    UntestedBanner {
        id: bar
        width: parent.width
        backend: fake
        pid: "0257"
        support: sup
        st: ({ firmware: "v1.2", mode: "Normal" })
        responded: ["profiles", "lighting"]
        onUpdated: s => sup = s
    }
    TestCase {
        name: "UntestedBanner"
        when: windowShown

        function init() {
            fake.calls = [];
            fake.copied = "";
            fake.fail = false;
            fake.status = "";
            sup = { pid: "0257", name: "Huntsman Mini", kind: "keyboard", layout: "60", tested: false, supported: true,
                allow_untested_writes: false, allow_writes: false, features: { onboard_macros: true, game_mode: true } };
            bar.confirming = false;
            bar.note = "";
        }

        function test_visible_only_when_untested() {
            verify(bar.visible);
            compare(findChild(bar, "bannerText").text, "Huntsman Mini hasn't been tested with Tsuba yet. Settings are shown read-only.");
            sup = Object.assign({}, sup, { tested: true });
            verify(!bar.visible);
            sup = Object.assign({}, sup, { tested: false, supported: false });
            verify(!bar.visible, "headsets (unsupported) get the info page, not the banner");
        }

        function test_confirmation_required_before_writing() {
            // Switching on only asks.
            bar.toggle(true);
            verify(bar.confirming);
            verify(findChild(bar, "confirmBox").visible);
            compare(fake.calls.length, 0);
            // Cancel: still nothing written.
            bar.confirming = false;
            compare(fake.calls.length, 0);
            verify(!bar.allowed);
            // Confirm: exactly one call, then the strip switches to its "allowed" form.
            bar.toggle(true);
            findChild(bar, "confirmAllow").clicked();
            compare(fake.calls.length, 1);
            compare(fake.calls[0][0], "0257");
            compare(fake.calls[0][1], true);
            verify(bar.allowed);
            verify(!bar.confirming);
            compare(findChild(bar, "bannerText").text, "Changes allowed — please report how it works.");
            verify(findChild(bar, "reportButton").visible);
        }

        function test_failure_keeps_blocked_and_shows_message() {
            fake.fail = true;
            bar.toggle(true);
            findChild(bar, "confirmAllow").clicked();
            compare(fake.calls.length, 1);
            verify(!bar.allowed);
            verify(findChild(bar, "bannerNote").text.indexOf("daemon said no") >= 0);
        }

        function test_switching_off_needs_no_confirmation() {
            sup = Object.assign({}, sup, { allow_untested_writes: true, allow_writes: true });
            bar.toggle(false);
            compare(fake.calls.length, 1);
            compare(fake.calls[0][1], false);
            verify(!bar.allowed);
        }

        function test_report_text() {
            sup = Object.assign({}, sup, { allow_untested_writes: true });
            findChild(bar, "reportButton").clicked();
            const t = fake.copied;
            verify(t.indexOf("Name: Huntsman Mini") >= 0, t);
            verify(t.indexOf("Product ID: 0257") >= 0);
            verify(t.indexOf("Kind: keyboard (60)") >= 0);
            verify(t.indexOf("Tested by Tsuba: no") >= 0);
            verify(t.indexOf("Firmware: v1.2") >= 0);
            verify(t.indexOf("Mode: Normal") >= 0);
            verify(t.indexOf("Settings that answered: profiles, lighting") >= 0);
            verify(findChild(bar, "bannerNote").text.indexOf("GitHub issue") >= 0);
        }
    }
}
