import QtQuick
import QtTest

// Linked games (WP-C): per-game polling rate, focus-only links, remove confirmation; and the
// Game Mode shortcut list.
Item {
    width: 1300
    height: 900
    QtObject {
        id: fake
        signal devices_changed_on_bus()
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }, { pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }])
        property string saved: JSON.stringify({ links: [{ name: "Elden", matches: "eldenring.exe", enabled: true, device_profiles: {}, chroma: null, art: "" }] })
        function app_links() { return JSON.stringify(Object.assign(JSON.parse(saved), { active: null })); }
        function set_app_links(j) { saved = j; return true; }
        function running_programs() { return "[]"; }
        function chroma_profiles() { return JSON.stringify({ profiles: [], active: "" }); }
        function library_profiles(p) { return JSON.stringify({ active: "a1", active_slot: 1, profiles: [{ id: "a1", name: "Profile 1", pinned_slot: 1 }] }); }
        function performance(pid, profile) { return pid === "00aa" ? JSON.stringify({ polling_hz: 1000, polling_options: [1000, 500, 125] }) : JSON.stringify({ polling_hz: 1000, polling_options: [] }); }
    }
    LinkedGamesPage {
        id: page
        anchors.fill: parent
        backend: fake
    }
    GameModeCombos {
        id: combos
        visible: false
        property var last: null
        onChosen: list => last = list
    }
    TestCase {
        name: "WpcLinks"
        when: windowShown

        function test_polling_rate_per_game_only_for_devices_that_have_rates() {
            wait(100);
            compare(Object.keys(page.pollingOptions), ["00aa"], "the keyboard reports no selectable rates here");
            compare(page.pollingOptions["00aa"], [125, 500, 1000]);
            page.sel = 0;
            wait(50);
            verify(findChild(page, "pollingRow_00aa") !== null);
            verify(findChild(page, "pollingRow_028d") === null);
            page.edit(l => {
                l.polling_hz = l.polling_hz || {};
                l.polling_hz["00aa"] = 500;
            });
            const l = JSON.parse(fake.saved).links[0];
            compare(l.polling_hz["00aa"], 500);
            compare(l.matches, "eldenring.exe", "the rest of the link is kept");
        }
        function test_focus_only_flag_round_trips() {
            page.sel = 0;
            wait(50);
            page.edit(l => l.only_when_focused = true);
            compare(JSON.parse(fake.saved).links[0].only_when_focused, true);
            const cb = findChild(page, "onlyFocused");
            verify(cb !== null);
            compare(cb.checked, true);
        }
        function test_removing_a_game_needs_confirmation() {
            page.sel = 0;
            wait(50);
            mouseClick(findChild(page, "removeLink"));
            verify(page.confirmRemove);
            compare(JSON.parse(fake.saved).links.length, 1, "still there");
            // Picking another row (or none) cancels the question.
            page.sel = -1;
            page.sel = 0;
            verify(!page.confirmRemove);
            page.confirmRemove = true;
            page.removeSelected();
            compare(JSON.parse(fake.saved).links.length, 0);
            compare(page.sel, -1);
        }
        function test_game_mode_shortcut_list() {
            combos.visible = true;
            compare(combos.all, true);
            combos.combos = ["alt_tab"];
            compare(combos.all, false);
            compare(findChild(combos, "gm_alt_tab").checked, true);
            compare(findChild(combos, "gm_alt_f4").checked, false);
            // Ticking another adds it; unticking the last one leaves an empty list; "all" is null.
            combos.chosen(["alt_tab", "alt_f4"]);
            compare(combos.last, ["alt_tab", "alt_f4"]);
            findChild(combos, "gmAll").toggled();
            compare(combos.catalogue.map(c => c[0]), ["alt_tab", "alt_f4", "alt_space", "meta", "ctrl_esc"]);
            combos.combos = null;
            compare(findChild(combos, "gm_alt_tab").visible, false);
        }
    }
}
