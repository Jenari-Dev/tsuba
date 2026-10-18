import QtQuick
import QtTest 1.2

Item {
    width: 1100
    height: 1400

    QtObject {
        id: backend
        property bool connected: true
        function library_profiles(pid) { return pid === "028d" ? JSON.stringify({ profiles: [{ id: "a", name: "Work" }], active: "a" }) : ""; }
        function device_support(pid) { return pid === "00aa" ? JSON.stringify({ tested: false, supported: true }) : ""; }
        function power(pid) { return pid === "00aa" ? JSON.stringify({ battery_percent: 12, charging: false }) : "{}"; }
        function device_pack(pid) { return ""; }
    }
    QtObject {
        id: app
        property int started: 0
        function start_service() { started++; }
        function copy_text(t) {}
    }
    HomePage {
        id: page
        width: parent.width
        height: parent.height
        backend: backend
        app: app
    }

    function all(item, name, out) {
        out = out || [];
        if (item.objectName === name) {
            out.push(item);
        }
        for (const c of item.children) {
            all(c, name, out);
        }
        return out;
    }

    TestCase {
        name: "ShellHome"
        when: windowShown

        function init() {
            page.devices = [];
            backend.connected = true;
            Theme.app.showTips = true;
            wait(30);
        }

        function test_no_device_shows_the_setup_guide() {
            const g = findChild(page, "setupGuide");
            verify(g.visible);
            page.devices = [{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }];
            wait(30);
            verify(!g.visible);
        }

        function test_groups_and_badges() {
            page.devices = [
                { pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" },
                { pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }
            ];
            wait(60);
            compare(page.groups.length, 2);
            compare(page.groups[0].kind, "keyboard");
            compare(page.groups[1].kind, "mouse");
            const bats = all(page, "batteryBadge").filter(b => b.visible);
            compare(bats.length, 1, "only the mouse has a battery");
            const untested = all(page, "untestedBadge").filter(b => b.visible);
            compare(untested.length, 1, "the untested model is flagged");
        }

        function test_connection_badge_follows_the_firmware_answer() {
            page.devb = { features: pid => pid === "028d" ? JSON.stringify({ connection: "wireless" }) : JSON.stringify({ connection: "wired" }) };
            page.devices = [
                { pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" },
                { pid: "00aa", name: "Basilisk V3 Pro", kind: "mouse" }
            ];
            wait(60);
            const texts = all(page, "connectionBadge").filter(b => b.visible).map(b => b.children[0].text);
            compare(texts.length, 2);
            verify(texts.indexOf("WIRELESS") >= 0 && texts.indexOf("WIRED") >= 0, texts.join());
            page.devb = null;
        }

        function test_single_group_has_no_heading() {
            page.devices = [{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }];
            wait(30);
            compare(page.groups.length, 1);
        }

        function test_non_affiliation_notice_is_on_the_page() {
            const t = findChild(page, "nonAffiliation").text;
            verify(t.indexOf("not affiliated with or endorsed by Razer Inc.") >= 0);
        }

        function test_tips_can_be_switched_off() {
            const tip = findChild(page, "tipBox");
            verify(tip.visible);
            Theme.app.showTips = false;
            wait(30);
            verify(!tip.visible);
        }

        function test_daemon_down_offers_to_start_it() {
            backend.connected = false;
            page.envReport = { service: { unit_installed: true } };
            wait(60);
            const g = findChild(page, "setupGuide");
            compare(g.state, "stopped");
            mouseClick(findChild(page, "guideStart"));
            compare(app.started, 1);
        }
    }
}
