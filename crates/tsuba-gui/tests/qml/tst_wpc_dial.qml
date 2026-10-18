import QtQuick
import QtTest

// Command dial editor (WP-C): reorder, reset to stock, and the guards.
Item {
    width: 800
    height: 900
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property var written: []
        property var modeSet: []
        property bool fail: false
        function set_dial(pid, json) {
            if (fail) {
                status = "device busy";
                return false;
            }
            written = written.concat([JSON.parse(json)]);
            return true;
        }
        function set_dial_mode(pid, slot) { modeSet = modeSet.concat([slot]); return true; }
        function assignments(p, prof, layer) { return JSON.stringify([{ key: 169, function: 17, data: [0x60], label: "" }]); }
        function actions(p) { return "[]"; }
    }
    QtObject {
        id: fakeLib
        property var calls: []
        function call(method, args) {
            calls = calls.concat([[method, JSON.parse(args)]]);
            if (method === "FactoryDial") {
                const names = ["KeyboardBrightness", "WindowsZoom", "SwitchApplications", "TrackJogging", "TrackSelector", "VerticalScrolling", "HorizontalScrolling", "SwitchBrowserTabs"];
                return JSON.stringify({ ok: true, value: JSON.stringify(names.map((n, i) => ({ function: n, colour: "00ff00", enabled: i < 4 }))) });
            }
            return JSON.stringify({ ok: false, error: "unknown" });
        }
    }
    DialEditor {
        id: ed
        width: 700
        backend: fake
        pid: "028d"
        profile: 1
        st: ({ dial: [
            { slot: 0, function: "KeyboardBrightness", colour: "00ff00", enabled: true, current: false },
            { slot: 1, function: "WindowsZoom", colour: "ff0000", enabled: true, current: true },
            { slot: 2, function: "SwitchApplications", colour: "0000ff", enabled: true, current: false },
            { slot: 3, function: "TrackJogging", colour: "ffff00", enabled: true, current: false },
            { slot: 4, function: "TrackSelector", colour: "008888", enabled: false, current: false },
            { slot: 5, function: "VerticalScrolling", colour: "880088", enabled: false, current: false },
            { slot: 6, function: "HorizontalScrolling", colour: "888888", enabled: false, current: false },
            { slot: 7, function: "SwitchBrowserTabs", colour: "884400", enabled: false, current: false }] })
        Component.onCompleted: lp.lib = fakeLib
    }
    TestCase {
        name: "WpcDial"
        when: windowShown

        function init() {
            fake.written = [];
            fake.modeSet = [];
            fake.fail = false;
            fakeLib.calls = [];
            ed.result = "";
            ed.confirmReset = false;
        }
        function functions(list) { return list.map(d => d.function); }

        function test_moving_a_mode_rewrites_the_table_in_the_new_order() {
            ed.moveMode(3, 1);
            compare(fake.written.length, 1);
            compare(functions(fake.written[0]).slice(0, 4), ["KeyboardBrightness", "TrackJogging", "WindowsZoom", "SwitchApplications"]);
            // Colours and on/off travel with their mode.
            compare(fake.written[0][1].colour, "ffff00");
            compare(fake.written[0][4].enabled, false);
            // The active mode (Windows Zoom) stays the active one, now in slot 2.
            compare(fake.modeSet, [2]);
            compare(ed.result, "Dial order saved");
        }
        function test_arrows_and_the_grip_move_by_one_and_by_drag() {
            const down = findChild(ed, "dialDown0");
            mouseClick(down);
            compare(functions(fake.written[0]).slice(0, 2), ["WindowsZoom", "KeyboardBrightness"]);
            fake.written = [];
            mouseClick(findChild(ed, "dialUp0"));
            compare(fake.written.length, 0, "the first mode can't go up");
            mouseClick(findChild(ed, "dialDown7"));
            compare(fake.written.length, 0, "the last mode can't go down");
            ed.moveMode(2, 2);
            ed.moveMode(2, 9);
            compare(fake.written.length, 0, "nonsense moves do nothing");
        }
        function test_switching_everything_off_is_refused() {
            const list = ed.plain();
            list.forEach(d => d.enabled = false);
            verify(!ed.write(list, "x"));
            compare(fake.written.length, 0);
            verify(ed.result.indexOf("at least one dial mode") > 0);
        }
        function test_a_failed_write_says_why() {
            fake.fail = true;
            ed.moveMode(1, 0);
            compare(ed.result, "Failed: device busy");
            compare(fake.modeSet.length, 0);
        }
        function test_reset_asks_first_and_writes_the_stock_table() {
            mouseClick(findChild(ed, "resetDialModes"));
            verify(ed.confirmReset);
            compare(fake.written.length, 0, "nothing happens before the confirmation");
            mouseClick(findChild(ed, "confirmResetDial"));
            compare(fakeLib.calls[0], ["FactoryDial", ["028d"]]);
            compare(fake.written.length, 1);
            compare(fake.written[0].filter(d => d.enabled).length, 4);
            compare(ed.result, "Dial modes are back to the stock order");
            verify(!ed.confirmReset);
        }
        function test_dial_press_remap_warning() {
            // The factory dial press is RazerKey 0x60; this key is still that.
            verify(!ed.dialRemapped);
            ed.dialAssign = { key: 169, function: 2, data: [0, 0x3a] };
            verify(ed.dialRemapped);
            verify(findChild(ed, "dialRemapWarning").visible);
        }
    }
}
