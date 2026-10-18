import QtQuick
import QtTest 1.2

Item {
    width: 1100
    height: 1600

    QtObject {
        id: app
        property bool tray_available: false
        property bool autostart: false
        property bool service: true
        property var notif: ({ enabled: true, dpi: true, profile: true, dial: true, wheel: true, battery_low: true, battery_full: false, battery_threshold: 15, connection: true, sound: false })
        property int restarts: 0
        property int tests: 0
        property var exported: []
        signal diagnostics_exported(bool ok, string message)
        function autostart_enabled() { return autostart; }
        function set_autostart(on) { autostart = on; return true; }
        function service_enabled() { return service; }
        function set_service_enabled(on) { service = on; return true; }
        function notification_settings() { return JSON.stringify(notif); }
        function set_notification_settings(j) { notif = JSON.parse(j); return j; }
        function send_test_notification() { tests++; return true; }
        function restart_app() { restarts++; return false; }   // false: the test must not quit
        function diagnostics_file_name() { return "tsuba-diagnostics-2026-10-18.zip"; }
        function export_diagnostics(p) { exported = exported.concat([p]); }
    }
    QtObject {
        id: backend
        property bool connected: true
        function has_personal_pack(pid) { return false; }
        function builtin_pack(pid) { return ""; }
        function has_custom_picture(pid) { return false; }
    }
    SettingsPage {
        id: page
        width: parent.width
        height: parent.height
        backend: backend
        app: app
    }

    TestCase {
        name: "ShellSettings"
        when: windowShown

        function init() {
            app.notif = { enabled: true, dpi: true, profile: true, dial: true, wheel: true, battery_low: true, battery_full: false, battery_threshold: 15, connection: true, sound: false };
            app.autostart = false;
            backend.connected = true;
            page.reload();
            wait(30);
        }

        function test_notification_toggle_keeps_other_fields() {
            mouseClick(findChild(page, "notifyDpi"));
            compare(app.notif.dpi, false);
            compare(app.notif.profile, true);
            compare(app.notif.battery_threshold, 15);
        }

        function test_master_switch_disables_the_rest() {
            const dpi = findChild(page, "notifyDpi");
            verify(dpi.enabled);
            mouseClick(findChild(page, "notifyMaster"));
            compare(app.notif.enabled, false);
            wait(30);
            verify(!dpi.enabled);
        }

        function test_osd_option_is_stored() {
            mouseClick(findChild(page, "notifyOsd"));
            compare(app.notif.osd, true);
        }

        function test_battery_threshold() {
            const s = findChild(page, "batteryThreshold");
            s.value = 30;
            s.moved();
            compare(app.notif.battery_threshold, 30);
        }

        function test_daemon_down_disables_notifications() {
            backend.connected = false;
            wait(30);
            verify(!findChild(page, "notifyMaster").enabled);
            verify(!findChild(page, "testNotification").enabled);
        }

        function test_test_notification_button() {
            mouseClick(findChild(page, "testNotification"));
            compare(app.tests, 1);
        }

        function test_launch_at_login() {
            const t = findChild(page, "autostartToggle");
            verify(!t.checked);
            mouseClick(t);
            compare(app.autostart, true);
            wait(30);
            verify(t.checked);
        }

        function test_theme_pills() {
            findChild(page, "themeTabs").activated(1);
            compare(Theme.prefs.themeMode, "light");
            findChild(page, "themeTabs").activated(2);
            compare(Theme.prefs.themeMode, "dark");
            findChild(page, "themeTabs").activated(0);
            compare(Theme.prefs.themeMode, "auto");
        }

        function test_language_choice_asks_for_restart() {
            const restart = findChild(page, "restartButton");
            verify(!restart.visible);
            findChild(page, "languageBox").activated(2);
            compare(Theme.app.language, "ja");
            wait(30);
            verify(restart.visible);
            mouseClick(restart);
            compare(app.restarts, 1);
            // back to the language the app started with: no restart needed
            findChild(page, "languageBox").activated(0);
            compare(Theme.app.language, "system");
            verify(!page.languageChanged);
        }

        function test_tray_toggle_needs_a_tray() {
            const t = findChild(page, "trayToggle");
            verify(!t.enabled);
            app.tray_available = true;
            wait(30);
            verify(t.enabled);
            app.tray_available = false;
        }

        function test_diagnostics_result_is_shown() {
            app.diagnostics_exported(true, "/home/x/d.zip");
            verify(page.diagNote.indexOf("/home/x/d.zip") >= 0);
            verify(page.diagOk);
            app.diagnostics_exported(false, "disk full");
            verify(page.diagNote.indexOf("disk full") >= 0);
            verify(!page.diagOk);
        }
    }
}
