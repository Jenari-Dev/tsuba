import QtQuick
import QtTest 1.2

Item {
    width: 400
    height: 300
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        readonly property var db: [
            { pid: "0257", name: "Huntsman Mini", kind: "keyboard", tested: false, supported: true },
            { pid: "0015", name: "Naga", kind: "mouse", tested: false, supported: true },
            { pid: "0501", name: "Kraken 7.1", kind: "headset", tested: false, supported: false },
            { pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard", tested: true, supported: true }
        ]
        function known_devices() { return JSON.stringify(db); }
        function device_pack(pid) { return ""; }
        function device_support(pid) { return JSON.stringify(db.find(d => d.pid === pid) || {}); }
        function env_var(n) { return ""; }
    }
    SupportedDevicesPage {
        id: browser
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "DeviceKinds"
        when: windowShown

        function kb(extra) {
            return Object.assign({ kind: "keyboard", tested: false, supported: true, effects: ["off", "static"], features: {}, polling_rates: [] }, extra);
        }

        function test_families() {
            compare(DeviceKinds.family("keyboard"), "keyboard");
            compare(DeviceKinds.family("laptop_keyboard"), "keyboard");
            compare(DeviceKinds.family("keypad"), "keyboard");
            compare(DeviceKinds.family("mouse"), "mouse");
            ["mousemat", "mousedock", "headset_stand", "accessory", "speaker", "egpu", "monitor"].forEach(k => compare(DeviceKinds.family(k), "lighting", k));
            compare(DeviceKinds.family("headset"), "info");
            compare(DeviceKinds.family("receiver"), "info");
        }

        function test_verified_devices_keep_their_tabs() {
            compare(DeviceKinds.tabsFor({ kind: "keyboard", tested: true, supported: true }, false), ["Customize", "Performance", "Lighting"]);
            compare(DeviceKinds.tabsFor({ kind: "mouse", tested: true, supported: true, features: { battery: true } }, true), ["Customize", "Performance", "Lighting", "Power"]);
            // No support information at all (older daemon): the classic set.
            compare(DeviceKinds.tabsFor({}, false), ["Customize", "Performance", "Lighting"]);
        }

        function test_untested_tabs_follow_features() {
            // Plain keyboard, nothing remappable: lighting only.
            compare(DeviceKinds.tabsFor(kb({}), false), ["Lighting"]);
            // Macro keys: remapping appears.
            compare(DeviceKinds.tabsFor(kb({ features: { onboard_macros: true } }), false), ["Customize", "Lighting"]);
            // Polling rates / game mode: Performance.
            compare(DeviceKinds.tabsFor(kb({ polling_rates: [1000] }), false), ["Performance", "Lighting"]);
            compare(DeviceKinds.tabsFor(kb({ features: { game_mode: true } }), false), ["Performance", "Lighting"]);
            // Mouse: buttons always; battery adds Power.
            compare(DeviceKinds.tabsFor({ kind: "mouse", supported: true, effects: ["off"], features: { dpi_stages: true, battery: true }, polling_rates: [] }, false),
                ["Customize", "Performance", "Lighting", "Power"]);
            // Mousemat: lighting only.
            compare(DeviceKinds.tabsFor({ kind: "mousemat", supported: true, effects: ["off", "static"], features: {}, polling_rates: [] }, false), ["Lighting"]);
            // Dock with a battery: lighting + power.
            compare(DeviceKinds.tabsFor({ kind: "mousedock", supported: true, effects: ["static"], features: { battery: true }, polling_rates: [] }, false), ["Lighting", "Power"]);
            // Headsets and anything unsupported: just the info page.
            compare(DeviceKinds.tabsFor({ kind: "headset", supported: false, effects: ["off"] }, false), ["Info"]);
            compare(DeviceKinds.tabsFor(kb({ supported: false }), false), ["Info"]);
            // A model with nothing to control falls back to the info page.
            compare(DeviceKinds.tabsFor({ kind: "accessory", supported: true, effects: [], features: {}, polling_rates: [] }, false), ["Info"]);
        }

        function test_per_led_needs_custom_frame_and_a_led_drawing() {
            verify(DeviceKinds.perLed({ effects: ["off", "custom_frame"] }, "keyboard"));
            verify(DeviceKinds.perLed({ effects: ["off", "custom_frame"] }, "mousemat"));
            verify(!DeviceKinds.perLed({ effects: ["off", "static"] }, "keyboard"));
            verify(!DeviceKinds.perLed({ effects: ["custom_frame"] }, "mouse"));
            verify(DeviceKinds.perLed({}, "keyboard"), "no support info: as before");
        }

        function test_status_labels() {
            compare(DeviceKinds.statusOf({ tested: true, supported: true }), "Tested");
            compare(DeviceKinds.statusOf({ tested: false, supported: true }), "Untested");
            compare(DeviceKinds.statusOf({ tested: false, supported: false }), "Not supported");
        }

        function test_browser_groups_and_filters() {
            compare(browser.rows.length, 4);
            // Order: keyboards, mice, headsets (kind group order), name within a group.
            compare(browser.rows.map(r => r.group), ["Keyboards", "Keyboards", "Mice", "Headsets"]);
            compare(browser.rows[0].name, "BlackWidow V4 Pro");
            browser.query = "kra";
            compare(browser.rows.length, 1);
            compare(browser.rows[0].pid, "0501");
            browser.query = "untested";
            compare(browser.rows.length, 2, "status is searchable (headsets read Not supported)");
            browser.query = "mice";
            compare(browser.rows.map(r => r.pid), ["0015"]);
            browser.query = "";
            browser.choose("0015");
            compare(browser.sel.name, "Naga");
        }
    }
}
