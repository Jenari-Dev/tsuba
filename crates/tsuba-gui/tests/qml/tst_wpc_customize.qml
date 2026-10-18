import QtQuick
import QtTest

// The Customize assignment panel (WP-C): staged changes with SAVE / CANCEL, the grouped key
// picker, the shortcut catalogue, Launch / Audio / Text / Mouse editors and the first-remap notice.
Item {
    id: top
    width: 360
    height: 900

    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }, { pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }])
        property var assignments: []
        property var actionsSet: []
        property var macrosBound: []
        property var macrosSaved: []
        property var linksJson: JSON.stringify({ links: [] })
        property bool failSet: false
        function set_assignment(p, j) {
            if (failSet) {
                status = "device busy";
                return "";
            }
            assignments = assignments.concat([JSON.parse(j)]);
            return "ok";
        }
        function assign_action(p, j) { actionsSet = actionsSet.concat([JSON.parse(j)]); return "Action saved"; }
        function assign_macro(p, j) { macrosBound = macrosBound.concat([JSON.parse(j)]); return "Macro: ok"; }
        function save_macro(j) { macrosSaved = macrosSaved.concat([JSON.parse(j)]); return "newguid" + macrosSaved.length; }
        function macros() { return JSON.stringify({ library: macrosSaved.map((m, i) => Object.assign({ guid: "newguid" + (i + 1) }, m)), devices: {} }); }
        function actions(p) { return "[]"; }
        function env_var(n) { return ""; }
        function app_links() { return linksJson; }
        function chroma_profiles() { return JSON.stringify({ profiles: [{ name: "Fire" }, { name: "Ice" }], active: "" }); }
    }
    // Stands in for the Rust LibraryBackend.
    QtObject {
        id: fakeLib
        property var calls: []
        function call(method, args) {
            calls = calls.concat([[method, JSON.parse(args)]]);
            const a = JSON.parse(args);
            switch (method) {
            case "ListAudioDevices":
                return JSON.stringify({ ok: true, value: JSON.stringify({ sinks: [{ name: "sink.a", description: "Speakers", default: true }, { name: "sink.b", description: "Headphones", default: false }], sources: [{ name: "src.a", description: "Desk mic", default: true }] }) });
            case "TextLayouts":
                return JSON.stringify({ ok: true, value: JSON.stringify({ layouts: ["de", "us"], active: "de" }) });
            case "TextToEvents": {
                const skipped = a[0].replace(/[ -~]/g, "");
                return JSON.stringify({ ok: true, value: JSON.stringify({ events: skipped === "" ? [{ type: "key", hid: 4, down: true }, { type: "key", hid: 4, down: false }] : [], skipped: skipped, layout: a[2] }) });
            }
            }
            return JSON.stringify({ ok: false, error: "unknown " + method });
        }
        function desktop_apps() { return JSON.stringify([{ name: "Calculator", command: "kcalc", icon: "", terminal: false, categories: [] }, { name: "htop", command: "htop", icon: "", terminal: true, categories: [] }, { name: "Zed", command: "zed", icon: "", terminal: false, categories: [] }]); }
        function launch_presets() { return JSON.stringify([{ id: "calculator", label: "Calculator", command: "kcalc" }, { id: "terminal", label: "Terminal", command: "konsole" }, { id: "editor", label: "Text editor", command: "" }]); }
        function path_of_url(u) { return u; }
        function home_url() { return "file:///home"; }
    }

    AssignmentPanel {
        id: panel
        anchors.fill: parent
        backend: fake
        pid: "028d"
        isMouse: false
        profile: 1
        profileId: "p1"
        remapNotice: false
        target: 160
        targetName: "M1"
        assignment: ({ key: 160, function: 17, data: [0x20], label: "" })
        Component.onCompleted: lp.lib = fakeLib
    }

    TestCase {
        name: "WpcCustomize"
        when: windowShown

        function find(item, name) {
            if (item.objectName === name && item.visible) {
                return item;
            }
            const kids = [].concat(Array.from(item.children), item.contentItem ? [item.contentItem] : [], item.contentChildren ? Array.from(item.contentChildren) : []);
            for (const k of kids) {
                const r = find(k, name);
                if (r) {
                    return r;
                }
            }
            return null;
        }
        // Click the Nth option row of the editor that is open (OptionRow has objectName ""), by label.
        function option(label) {
            function walk(item) {
                if (item.visible && item.label === label && typeof item.picked === "function") {
                    return item;
                }
                const kids = [].concat(Array.from(item.children), item.contentItem ? [item.contentItem] : [], item.contentChildren ? Array.from(item.contentChildren) : []);
                for (const k of kids) {
                    const r = walk(k);
                    if (r) {
                        return r;
                    }
                }
                return null;
            }
            return walk(panel);
        }
        function init() {
            fake.assignments = [];
            fake.actionsSet = [];
            fake.macrosBound = [];
            fake.macrosSaved = [];
            fake.linksJson = JSON.stringify({ links: [] });
            fakeLib.calls = [];
            panel.noticeOpen = false;
            panel.remapNotice = false;
            panel.isMouse = false;
            panel.target = 160;
            panel.open = "";
            panel.staged = null;
            panel.turbo = false;
            panel.captured = null;
        }

        function test_choosing_only_stages_and_save_writes() {
            panel.openCategory("media");
            wait(20);
            const o = option("Mute");
            verify(o !== null);
            mouseClick(o);
            compare(fake.assignments.length, 0, "nothing is written before SAVE");
            compare(panel.stagedId, "media:" + 0xe2);
            verify(find(panel, "pendingNote") !== null, "the pending change is shown");
            panel.commit();
            compare(fake.assignments.length, 1);
            compare(fake.assignments[0].function, 10);
            compare(fake.assignments[0].data, [0, 0xe2]);
            compare(panel.staged, null);
        }
        function test_cancel_drops_the_change() {
            panel.openCategory("media");
            wait(20);
            mouseClick(option("Stop"));
            verify(panel.staged !== null);
            panel.cancel();
            compare(panel.staged, null);
            compare(fake.assignments.length, 0);
        }
        function test_save_with_nothing_chosen_just_closes() {
            const spy = createTemporaryObject(spyComp, top);
            panel.openCategory("sensitivity");
            panel.commit();
            compare(fake.assignments.length, 0);
            compare(spy.count, 1);
        }
        Component {
            id: spyComp
            SignalSpy {
                target: panel
                signalName: "closed"
            }
        }
        function test_failed_save_stays_open_with_the_error() {
            const spy = createTemporaryObject(spyComp, top);
            panel.openCategory("media");
            wait(20);
            mouseClick(option("Stop"));
            fake.failSet = true;
            panel.commit();
            fake.failSet = false;
            compare(spy.count, 0, "an error keeps the panel open");
            verify(panel.result.indexOf("Failed") === 0);
            compare(panel.stagedId, "media:" + 0xb7, "the choice is kept for another try");
        }

        function test_category_list_is_in_synapse_order() {
            const names = panel.categories.map(c => c.id);
            compare(names.slice(0, 5), ["default", "keyboard", "mouse", "sensitivity", "macro"]);
            verify(names.indexOf("interdevice") < names.indexOf("profile"));
            verify(names.indexOf("profile") < names.indexOf("lighting"));
            verify(names.indexOf("launch") < names.indexOf("media"));
            compare(names[names.length - 1], "disable");
        }

        // ---- grouped key picker
        function test_key_picker_groups_and_modifiers() {
            panel.openCategory("keyboard");
            wait(20);
            const picker = find(panel, "keyboardPicker");
            verify(picker !== null);
            compare(KeyNames.keyGroups.map(g => g[0]), ["Letters", "Numbers", "Function keys", "Navigation", "Editing and spacing", "Punctuation", "Numpad", "Modifiers"]);
            picker.group = 2;                 // function keys: F1..F12 then F13..F24
            wait(20);
            compare(find(picker, "keyChips").children.length - 1 >= 24, true);
            // Ctrl+Shift with F13.
            picker.mods = 3;
            picker.picked(KeyNames.pickedData(0x68, picker.mods), 0x68);
            compare(panel.captured, [3, 0x68]);
            panel.commit();
            compare(fake.assignments[0].function, 2);
            compare(fake.assignments[0].data, [3, 0x68]);
            // A modifier key on its own is a modifier bit, no usage.
            compare(KeyNames.pickedData(0xe1, 4), [2, 0]);
            compare(KeyNames.pickedData(0xe6, 0), [0x40, 0]);
            compare(KeyNames.groupOf(0x04), 0);
            compare(KeyNames.groupOf(0xe0), 7);
        }
        function test_turbo_keyboard_function() {
            panel.openCategory("keyboard");
            panel.turbo = true;
            panel.turboRate = 10;
            panel.stageKey([0, 0x04]);
            panel.commit();
            compare(fake.assignments[0].function, 13);
            compare(fake.assignments[0].data, [0, 4, 0, 100]);
        }

        // ---- shortcut catalogue
        function test_shortcut_catalogue_areas_and_custom_chord() {
            panel.openCategory("shortcuts");
            wait(20);
            const cat = find(panel, "shortcutCatalogue");
            compare(cat.areas.map(a => a[0]), ["system", "editing", "browser", "office", "creative", "video", "meeting"]);
            compare(cat.items[0][0], "Switch window (Alt+Tab)");
            mouseClick(option("Copy") || option("Switch window (Alt+Tab)"));
            panel.commit();
            compare(fake.assignments.length, 1);
            compare(fake.assignments[0].function, 2);
            compare(fake.assignments[0].data, [0x04, 0x2b]);
            // A custom chord: Ctrl+Alt chips plus the K key.
            fake.assignments = [];
            panel.openCategory("shortcuts");
            mouseClick(find(cat, "chipCtrl"));
            mouseClick(find(cat, "chipAlt"));
            compare(cat.customMods, 5);
            cat.customKey = 0x0e;
            cat.stageCustom();
            compare(find(cat, "customChord").text, "Ctrl+Alt+K");
            panel.commit();
            compare(fake.assignments[0].data, [5, 0x0e]);
            // Modifier chips alone are a valid chord too (e.g. a lone Ctrl).
            compare(KeyNames.chord(1, 0), "Ctrl");
        }
        function test_linked_app_suggests_the_matching_area() {
            fake.linksJson = JSON.stringify({ links: [{ name: "Firefox", matches: "firefox", device_profiles: { "028d": "p1" } }, { name: "Other", matches: "x", device_profiles: { "028d": "p2" } }] });
            panel.openCategory("shortcuts");
            const cat = find(panel, "shortcutCatalogue");
            compare(cat.linkedApps, ["Firefox"]);
            compare(cat.suggested, "browser");
            compare(cat.areas[0][0], "browser");
            verify(find(cat, "suggestedNote") !== null);
            compare(KeyNames.categoryForApp("Blender"), "creative");
            compare(KeyNames.categoryForApp("elden ring"), "");
            compare(KeyNames.categoryForApp("Discord"), "meeting");
        }

        // ---- launch
        function test_launch_presets_and_installed_apps() {
            panel.openCategory("launch");
            wait(20);
            const ed = find(panel, "launchEditor");
            compare(ed.presets.length, 3);
            const calc = find(ed, "preset_calculator");
            verify(calc.enabled);
            verify(!find(ed, "preset_editor").enabled, "a preset that isn't installed can't be chosen");
            mouseClick(calc);
            compare(panel.stagedId, "launch:kcalc");
            panel.commit();
            compare(fake.actionsSet.length, 1);
            compare(fake.actionsSet[0].action, { type: "launch", command: "kcalc" });
            compare(fake.actionsSet[0].key, 160);
            // Search narrows the installed list; a terminal app is wrapped in the terminal.
            ed.filter = "htop";
            compare(ed.shown.length, 1);
            compare(ed.commandFor(ed.shown[0]), "konsole -e htop");
            ed.filter = "";
            // A typed command and a website.
            ed.stageCommand("steam -silent");
            compare(panel.staged.label, "Launch steam -silent");
            ed.stageUrl("example.org");
            compare(panel.stagedId, "url:https://example.org");
            panel.commit();
            compare(fake.actionsSet[1].action, { type: "url", url: "https://example.org" });
        }

        // ---- audio
        function test_audio_editor_outputs_and_mic() {
            panel.openCategory("audio");
            wait(20);
            const ed = find(panel, "audioEditor");
            compare(ed.sinks.length, 2);
            compare(ed.sinkIdx, 0, "the current default is preselected");
            mouseClick(option("Microphone volume up"));
            panel.commit();
            compare(fake.actionsSet[0].action, { type: "audio", op: "mic_volume_up" });
            ed.sinkIdx = 1;
            mouseClick(find(ed, "setOutput"));
            panel.commit();
            compare(fake.actionsSet[1].action, { type: "audio", op: { set_default_output: { name: "sink.b", label: "Headphones" } } });
            mouseClick(find(ed, "setInput"));
            panel.commit();
            compare(fake.actionsSet[2].action.op.set_default_input.name, "src.a");
        }

        // ---- text
        function test_text_uses_the_layout_and_falls_back_for_emoji() {
            panel.openCategory("text");
            wait(20);
            const ed = find(panel, "textEditor");
            compare(ed.layouts, ["de", "us"]);
            ed.text = "ab";
            ed.restage();
            panel.commit();
            compare(fake.macrosSaved.length, 1, "typed with keys: an on-board macro");
            compare(fake.macrosSaved[0].events.length, 2);
            compare(fake.macrosBound[0].playback, "once");
            const call = fakeLib.calls.find(c => c[0] === "TextToEvents");
            compare(call[1], ["ab", "10", "de"], "the desktop's first layout is used");
            // Emoji can't be typed with keys: Tsuba types it itself.
            ed.text = "hi 😀";
            ed.restage();
            verify(find(ed, "textPlan").text.indexOf("types this text itself") > 0);
            panel.commit();
            compare(fake.actionsSet[0].action, { type: "type", text: "hi 😀" });
        }

        // ---- mouse
        function test_click_with_keys_held_builds_a_macro() {
            panel.isMouse = true;
            panel.pid = "00aa";
            panel.target = 4;
            panel.assignment = { key: 4, function: 1, data: [4], label: "" };
            panel.openCategory("mouse");
            wait(20);
            const ed = find(panel, "mouseEditor");
            ed.comboMods = 1;
            ed.comboButton = 0;
            ed.stageCombo();
            compare(panel.staged.label, "Ctrl + Left click");
            panel.commit();
            compare(fake.macrosSaved.length, 1);
            const ev = fake.macrosSaved[0].events;
            compare(ev[0], { type: "key", hid: 0xe0, down: true });
            compare(ev.filter(e => e.type === "button").map(e => e.down), [true, false]);
            compare(ev[ev.length - 1], { type: "key", hid: 0xe0, down: false });
            compare(fake.macrosBound[0].playback, "once");
            compare(fake.macrosBound[0].key, 4);
            // Ctrl+Shift + a held key + scroll: the held key goes down before the click.
            const m = panel.assignment;
            ed.comboMods = 3;
            ed.comboKey = 0x04;
            ed.comboButton = 5;
            const ev2 = ed.comboEvents();
            compare(ev2.slice(0, 3).map(e => e.hid), [0xe0, 0xe1, 0x04]);
            compare(ev2.filter(e => e.type === "scroll").length, 1);
            panel.assignment = m;
            panel.isMouse = false;
            panel.pid = "028d";
        }
        function test_repeating_scroll_uses_the_turbo_button_function() {
            panel.isMouse = true;
            panel.pid = "00aa";
            panel.target = 15;
            panel.assignment = { key: 15, function: 1, data: [1], label: "" };
            panel.openCategory("mouse");
            wait(20);
            const ed = find(panel, "mouseEditor");
            ed.turbo = true;
            ed.turboRate = 20;
            mouseClick(find(ed, "scroll9"));
            panel.commit();
            compare(fake.assignments.length, 1);
            compare(fake.assignments[0].function, 14);
            compare(fake.assignments[0].data, [9, 0, 50]);
            // The panel shows what the key does now: a repeating scroll-left takes its rate from the key.
            fake.assignments = [];
            panel.assignment = { key: 15, function: 14, data: [104, 0, 100], label: "" };
            compare(ed.turbo, true);
            compare(ed.turboRate, 10);
            panel.assignment = { key: 15, function: 1, data: [2], label: "" };
            compare(ed.turbo, false);
            panel.isMouse = false;
            panel.pid = "028d";
        }

        // ---- hints and notices
        function test_switch_profile_hint_needs_two_profiles() {
            panel.profileCount = 1;
            panel.openCategory("profile");
            verify(find(panel, "profileHint") !== null && find(panel, "profileHint").visible);
            panel.profileCount = 3;
            compare(find(panel, "profileHint"), null);
        }
        function test_first_remap_notice_shows_once() {
            panel.remapNotice = true;
            panel.openCategory("media");
            mouseClick(option("Stop"));
            panel.commit();
            verify(panel.noticeOpen, "the first remap shows the notice");
            verify(find(panel, "remapNotice").visible);
            mouseClick(find(panel, "remapNoticeOk"));
            verify(!panel.noticeOpen);
            // Never again (the flag is stored in app.ini).
            panel.openCategory("media");
            mouseClick(option("Mute"));
            panel.commit();
            verify(!panel.noticeOpen);
            // Putting a key back to its default isn't a remap.
            panel.remapNotice = true;
        }
    }
}
