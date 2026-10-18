import QtQuick
import QtTest

Item {
    width: 1300
    height: 900
    QtObject {
        id: fake
        property string devices: JSON.stringify([{ pid: "028d", name: "BlackWidow V4 Pro (wired)", kind: "keyboard" }])
        property bool available: true
        property bool enabled: true
        property var sessions: []
        property var calls: []
        property string state1: "not_installed"
        property bool auto: false
        property bool web: false
        property bool webFails: false
        signal game_lighting_changed(string status)
        signal game_lighting_tested(bool ok, string message)
        function status() { return { enabled: enabled, port: enabled ? 54235 : null, sessions: sessions }; }
        function game_lighting() { return JSON.stringify({ available: available, running: true, enabled: enabled, web: web, status: status() }); }
        function set_game_lighting_web(on) { calls.push("web " + on); if (webFails) { return false; } web = on; return true; }
        function set_game_lighting(on) { calls.push("set " + on); enabled = on; return true; }
        function game_lighting_prefixes() {
            return JSON.stringify({ dll_dir: "/usr/share/tsuba/game-lighting", auto: auto, auto_installed: [], prefixes: [
                { id: "steam:1245620", source: "steam", name: "Elden Ring", path: "/s/compatdata/1245620/pfx", art: "", state: state1 },
                { id: "steam:2", source: "steam", name: "Vendor Game", path: "/s/compatdata/2/pfx", art: "", state: "foreign" }
            ] });
        }
        function game_lighting_install(ids) { calls.push("install " + ids); state1 = "installed"; return JSON.stringify({ ok: true, message: "Installed in 1 of 1 game." }); }
        function game_lighting_uninstall(ids) { calls.push("uninstall " + ids); state1 = "not_installed"; return JSON.stringify({ ok: true, message: "Removed from 1 of 1 game." }); }
        function game_lighting_add_prefix(p) { return ""; }
        function set_game_lighting_auto(on) { auto = on; calls.push("auto " + on); return true; }
        function game_lighting_test() { calls.push("test"); }
    }
    GameLightingPage {
        id: page
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "GameLighting"
        when: windowShown
        function find(name) { return findChild(page, name); }

        function test_1_status_lines() {
            compare(page.headline, "Waiting for a game");
            fake.game_lighting_changed(JSON.stringify({ enabled: true, port: 54235, sessions: [{ id: 1, title: "Cyberpunk 2077", devices_active: ["028d"] }] }));
            compare(page.headline, "Cyberpunk 2077 is controlling your lighting");
            verify(page.subline.indexOf("BlackWidow V4 Pro") >= 0, page.subline);
            fake.game_lighting_changed(JSON.stringify({ enabled: true, port: 54235, sessions: [{ id: 1, title: "Menu", devices_active: [] }] }));
            compare(page.headline, "Menu is connected");
            fake.game_lighting_changed(JSON.stringify({ enabled: false, port: 54235, sessions: [] }));
            compare(page.headline, "Game lighting is off");
        }

        function test_2_old_daemon() {
            fake.available = false;
            page.reloadStatus();
            verify(page.headline.indexOf("doesn't have game lighting") >= 0, page.headline);
            compare(find("glToggle").enabled, false);
            compare(find("glTestButton").enabled, false);
            fake.available = true;
            fake.enabled = true;
            page.reloadStatus();
            compare(find("glToggle").enabled, true);
        }

        function test_3_toggle_and_test() {
            fake.calls = [];
            find("glToggle").toggled(false);
            compare(fake.calls[0], "set false");
            compare(page.gl.enabled, false);
            find("glToggle").toggled(true);
            verify(find("glTestButton").enabled);
            find("glTestButton").clicked();
            verify(fake.calls.indexOf("test") >= 0);
            compare(page.testing, true);
            fake.game_lighting_tested(true, "Test pattern sent (60 frames)");
            compare(page.testing, false);
            compare(page.testResult, "Test pattern sent (60 frames)");
        }

        function test_4_install_flow() {
            page.reloadPrefixes();
            compare(page.scan.prefixes.length, 2);
            verify(find("glInstallAll").enabled);
            fake.calls = [];
            page.apply(["steam:1245620"], true);
            compare(fake.calls[0], 'install ["steam:1245620"]');
            compare(page.scan.prefixes[0].state, "installed");
            compare(page.message, "Installed in 1 of 1 game.");
            // Only a foreign prefix left: nothing to install for all.
            verify(!find("glInstallAll").enabled);
            page.apply(["steam:1245620"], false);
            compare(page.scan.prefixes[0].state, "not_installed");
            compare(page.stateLabel("foreign"), "Another library is installed");
        }

        function test_5_auto() {
            find("glAuto").toggle();
            find("glAuto").toggled();
            verify(fake.calls.indexOf("auto true") >= 0, JSON.stringify(fake.calls));
            compare(page.scan.auto, true);
        }
    
        function test_6_web_toggle() {
            page.reloadStatus();
            const box = find("glWeb");
            verify(box);
            compare(box.checked, false);
            fake.calls = [];
            box.toggle();
            box.toggled();
            compare(fake.calls[0], "web true");
            compare(page.gl.web, true);
            compare(box.checked, true);
            // Failure: the box follows the daemon again.
            fake.webFails = true;
            box.toggle();
            box.toggled();
            compare(fake.calls[1], "web false");
            compare(page.gl.web, true);
            compare(box.checked, true);
            compare(page.messageOk, false);
            fake.webFails = false;
            fake.available = false;
            page.reloadStatus();
            compare(box.enabled, false);
            fake.available = true;
            page.reloadStatus();
        }

        function test_7_game_text_is_plain() {
            fake.game_lighting_changed(JSON.stringify({ enabled: true, port: 54235, sessions: [{ id: 1, title: "<img src='file:///etc/passwd'><b>Evil</b>", devices_active: ["028d"] }] }));
            const h = find("glHeadline");
            compare(h.textFormat, Text.PlainText);
            verify(h.text.indexOf("<b>Evil</b>") >= 0, h.text);
            compare(find("glMessage").textFormat, Text.PlainText);
        }
    }
}
