import QtQuick
import QtTest

// WP-B: controls driven by the read-only feature probe (lift-off, debounce, resets, zones,
// lights off when locked, DPI stage order). Fake backends record what would be sent.
Item {
    id: root
    width: 1300
    height: 1000

    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "00aa" }, { pid: "028d" }])
        property var sent: []
        property var powerSent: []
        function performance(p, prof) { return JSON.stringify({ profile: prof, polling_hz: 1000, polling_options: [1000, 500], dpi_stages: [[800, 800], [1600, 1600], [3200, 3200]], dpi_active_stage: 1, free_spin: false, smart_reel: false, scroll_acceleration: false }); }
        function set_performance(p, j) { sent = sent.concat([JSON.parse(j)]); return "ok"; }
        function power(p) { return ""; }
        function game_mode(p) { return JSON.stringify({ active: false, disable_win: true, disable_menu: false, block_shortcuts: true }); }
        function assignments(p, prof, l) { return "[]"; }
        function actions(p) { return "[]"; }
        function config(p) { return "{}"; }
        function lighting_power(p) { return JSON.stringify({ idle_minutes: 0, display_off: false, display_off_minutes: 10, on_lock: false }); }
        function set_lighting_power(p, j) { powerSent = powerSent.concat([JSON.parse(j)]); return true; }
        function set_brightness(p, v) { return true; }
        function set_effect(p, j, b) { return true; }
        function save_lighting(p, j, b) { return true; }
        function copy_text(t) {}
    }

    QtObject {
        id: fakeDev
        property string status: ""
        property var featSent: []
        property var calls: []
        property var featJson: ({})
        function features(p, prof) { return JSON.stringify(featJson); }
        function set_features(p, j) { featSent = featSent.concat([JSON.parse(j)]); return "changed"; }
        function reset_profile(p, slot, confirm) { calls = calls.concat(["reset " + slot + " " + confirm]); return "on-board profile reset"; }
        function factory_reset(p, confirm) { calls = calls.concat(["factory " + confirm]); return "reset"; }
        function set_zone_effect(p, region, j, b) { calls = calls.concat(["zone " + region + " " + JSON.parse(j).type]); return true; }
        function set_zone_brightness(p, region, v) { calls = calls.concat(["zone brightness " + region]); return true; }
        function clear_zones(p) { calls = calls.concat(["clear zones"]); return true; }
        function set_effect_all(j, b) { calls = calls.concat(["all " + JSON.parse(j).type]); return "ok"; }
        function set_brightness_all(v) { calls = calls.concat(["all brightness " + v]); return "ok"; }
        function battery_lighting(p) { return "null"; }
        function set_battery_lighting(p, j) { return true; }
    }

    readonly property var mouseFeat: ({ profile: 1, supported: ["proximity_state", "debounce"], lift_sensor: true, debounce: { make: 8, make_mask: 28, "break": 8, break_mask: 28 }, smart_tracking: { asymmetric: false, distance_mm: 2, lift_mm: 3, landing_mm: 2 }, smart_tracking_saved: false, can_reset_profile: true, model: "Basilisk V3 Pro (wired)", support_url: "https://example.invalid", connection: "wired", factory_reset_phrase: "Basilisk V3 Pro (wired)", profile_reset_phrase: "reset profile 1" })
    readonly property var kbFeat: ({ profile: 1, supported: ["debounce", "profile_debounce"], lift_sensor: false, debounce: { make: 0, make_mask: 20, "break": 25, break_mask: 15 }, can_reset_profile: true, model: "BlackWidow V4 Pro", factory_reset_phrase: "BlackWidow V4 Pro", profile_reset_phrase: "reset profile 1" })

    Component {
        id: perfComp
        PerformanceTab {
            anchors.fill: parent
            backend: fake
            devb: fakeDev
            pid: "00aa"
            st: ({ kind: "mouse", firmware: "2.01", mode: "Normal", lighting: [] })
        }
    }
    Component {
        id: lightComp
        LightingTab {
            anchors.fill: parent
            backend: fake
            devb: fakeDev
            pid: "00aa"
            st: ({ kind: "mouse", lighting: [
                { id: 1, name: "scroll wheel", rows: 1, cols: 1, effect: "Spectrum", colours: [], brightness: 100, supported: ["Off", "Static", "Breathing", "Spectrum", "Reactive", "CustomFrame"] },
                { id: 4, name: "logo", rows: 1, cols: 1, effect: "Spectrum", colours: [], brightness: 100, supported: ["Off", "Static", "Breathing", "Spectrum", "Reactive", "CustomFrame"] },
                { id: 10, name: "underglow", rows: 1, cols: 11, effect: "Spectrum", colours: [], brightness: 100, supported: ["Off", "Static", "Breathing", "Spectrum", "Reactive", "CustomFrame"] }
            ] })
        }
    }

    TestCase {
        name: "WpbFeatures"
        when: windowShown

        function init() {
            fake.sent = [];
            fake.powerSent = [];
            fakeDev.featSent = [];
            fakeDev.calls = [];
        }

        function test_mouse_lift_off_shown_and_sent() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(perfComp, root);
            wait(100);
            const card = findChild(tab, "smartTrackingCard");
            verify(card !== null && card.visible, "lift-off card shown");
            const deb = findChild(tab, "debounceCard");
            verify(deb === null || !deb.visible, "no key debounce on a mouse");
            card.send({ distance_mm: 3 });
            compare(fakeDev.featSent.length, 1);
            compare(fakeDev.featSent[0].smart_tracking.distance_mm, 3);
            compare(fakeDev.featSent[0].profile, 1);
            // Asymmetric with an impossible landing is corrected to below lift-off.
            card.send({ asymmetric: true, lift_mm: 2, landing_mm: 2 });
            compare(fakeDev.featSent[1].smart_tracking.landing_mm, 1);
        }

        function test_hidden_without_probe_answers() {
            fakeDev.featJson = ({ profile: 1, supported: [], lift_sensor: false });
            const tab = createTemporaryObject(perfComp, root);
            wait(100);
            const card = findChild(tab, "smartTrackingCard");
            verify(card === null || !card.visible, "no lift-off card when the sensor didn't answer");
            const reset = findChild(tab, "deviceResetCard");
            verify(reset === null || !reset.visible, "no reset card without reset support");
        }

        function test_keyboard_debounce_presets() {
            fakeDev.featJson = root.kbFeat;
            const tab = createTemporaryObject(perfComp, root, { pid: "028d", st: { kind: "keyboard", firmware: "1.00", mode: "Normal", lighting: [] } });
            wait(100);
            const card = findChild(tab, "debounceCard");
            verify(card !== null && card.visible, "debounce card shown");
            compare(card.presetIndex, -1, "0/20/25/15 is no preset");
            card.send(card.presets[1].v);
            compare(JSON.stringify(fakeDev.featSent[0].debounce), JSON.stringify({ make: 0, make_mask: 0, "break": 0, break_mask: 0 }));
        }

        function test_stage_reorder_keeps_active_stage() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(perfComp, root);
            wait(100);
            tab.activeStage = 1;
            tab.moveStage(0, 1);
            const s = fake.sent[fake.sent.length - 1];
            compare(JSON.stringify(s.dpi_stages), JSON.stringify([[1600, 1600], [800, 800], [3200, 3200]]));
            compare(s.dpi_active_stage, 2, "the active stage moved with its value");
            const n = fake.sent.length;
            tab.moveStage(0, -1);
            compare(fake.sent.length, n, "can't move the first stage up");
        }

        function test_profile_reset_needs_typed_phrase() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(perfComp, root);
            wait(100);
            const card = findChild(tab, "deviceResetCard");
            verify(card !== null && card.visible);
            mouseClick(findChild(card, "resetProfileButton"));
            wait(100);
            const confirm = findChild(card, "resetProfileConfirm");
            verify(!confirm.enabled, "disabled before typing");
            const field = findChild(card, "resetProfileField");
            field.text = "reset profile 2";
            verify(!confirm.enabled, "wrong phrase");
            field.text = "reset profile 1";
            verify(confirm.enabled, "right phrase");
            mouseClick(confirm);
            compare(fakeDev.calls[0], "reset 1 reset profile 1");
        }

        function test_factory_reset_needs_checkbox_and_name() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(perfComp, root);
            wait(100);
            const card = findChild(tab, "deviceResetCard");
            mouseClick(findChild(card, "factoryResetButton"));
            wait(100);
            const confirm = findChild(card, "factoryResetConfirm");
            const field = findChild(card, "factoryResetField");
            field.text = "Basilisk V3 Pro (wired)";
            verify(!confirm.enabled, "needs the checkbox too");
            findChild(card, "factoryResetUnderstood").checked = true;
            verify(confirm.enabled);
            mouseClick(confirm);
            compare(fakeDev.calls[0], "factory Basilisk V3 Pro (wired)");
        }

        function test_zone_effects_and_all_devices() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(lightComp, root);
            wait(150);
            verify(findChild(tab, "zoneBox").visible, "zone picker for a multi-zone mouse");
            tab.zone = 2;   // logo
            wait(50);
            verify(tab.choices.every(e => !tab.softOnHere(e)), "a zone offers built-in effects only");
            tab.effectIndex = tab.choices.findIndex(e => e.type === "static");
            tab.apply();
            compare(fakeDev.calls[fakeDev.calls.length - 1], "zone 4 static");
            tab.zone = 0;
            wait(50);
            tab.effectIndex = tab.choices.findIndex(e => e.type === "spectrum");
            tab.apply();
            verify(fakeDev.calls.indexOf("clear zones") >= 0, "a whole-device effect clears the zones");
            tab.allDevices = true;
            tab.apply();
            compare(fakeDev.calls[fakeDev.calls.length - 1], "all spectrum");
        }

        function test_off_when_locked_option() {
            fakeDev.featJson = root.mouseFeat;
            const tab = createTemporaryObject(lightComp, root);
            wait(150);
            const box = findChild(tab, "offWhenLocked");
            verify(box !== null && box.visible);
            mouseClick(box);
            compare(fake.powerSent.length, 1);
            compare(fake.powerSent[0].on_lock, true);
            compare(fake.powerSent[0].display_off_minutes, 10, "other options kept");
        }
    }
}
