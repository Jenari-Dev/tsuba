import QtQuick
import QtTest

// Macro library list (WP-C): search, folders, not-in-use marker, missing-macro warning, rename /
// duplicate / folder / delete; the new event kinds and the recording options.
Item {
    width: 1400
    height: 860
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }])
        property var deleted: []
        readonly property var original: [
            { guid: "g1", name: "Copy line", group: "Work", events: [{ type: "key", hid: 4, down: true }] },
            { guid: "g2", name: "Sign off", group: "", events: [{ type: "key", hid: 5, down: true }] },
            { guid: "g3", name: "Open terminal", group: "Work", events: [{ type: "run", command: "konsole", slot: 0 }] },
            { guid: "g4", name: "Alt macro", group: "Games", events: [{ type: "delay", ms: 5 }] }]
        property var library: original
        function macros() { return JSON.stringify({ library: library, devices: {} }); }
        function delete_macro(g) { deleted = deleted.concat([g]); library = library.filter(m => m.guid !== g); return true; }
        function env_var(n) { return ""; }
    }
    QtObject {
        id: fakeLib
        property var calls: []
        property var usage: ({ used: { g1: [{ pid: "028d", profile: "Gaming", profile_id: "p", key: 160, layer: 0 }], g3: [{ pid: "028d", profile: "Work", profile_id: "w", key: 161, layer: 0 }] },
                              missing: [{ pid: "028d", profile: "Old", profile_id: "o", key: 7, layer: 0, guid: "gone" }] })
        function call(method, args) {
            const a = JSON.parse(args);
            calls = calls.concat([[method, a]]);
            if (method === "MacroUsage") {
                return JSON.stringify({ ok: true, value: JSON.stringify(usage) });
            }
            if (method === "DuplicateMacro") {
                return JSON.stringify({ ok: true, value: "g9" });
            }
            return JSON.stringify({ ok: true, value: "" });
        }
    }
    MacrosPage {
        id: page
        anchors.fill: parent
        backend: fake
        Component.onCompleted: {
            lp.lib = fakeLib;
            load();
        }
    }
    TestCase {
        name: "WpcMacros"
        when: windowShown

        function list() { return findChild(page, "macroList"); }
        function init() {
            fakeLib.calls = [];
            fake.deleted = [];
            fake.library = fake.original;
            page.load();
            const l = list();
            l.search = "";
            l.onlyUnused = false;
            l.collapsed = ({});
            l.done();
        }

        function test_folders_search_and_unused_filter() {
            const l = list();
            compare(l.rows.map(r => r.header || r.m.name), ["Unfiled", "Sign off", "Games", "Alt macro", "Work", "Copy line", "Open terminal"]);
            // Search matches names and folders.
            l.search = "term";
            compare(l.rows.map(r => r.header || r.m.name), ["Work", "Open terminal"]);
            l.search = "games";
            compare(l.rows.map(r => r.header || r.m.name), ["Games", "Alt macro"]);
            l.search = "";
            // Not in use: those no profile binds.
            compare(l.isUnused(l.lib[0]), false);
            compare(l.isUnused(l.lib[1]), true);
            compare(l.unusedCount, 2);
            l.onlyUnused = true;
            compare(l.rows.filter(r => r.m).map(r => r.m.name), ["Sign off", "Alt macro"]);
            l.onlyUnused = false;
            // Collapsing a folder hides its macros.
            l.collapsed = ({ "Work": true });
            compare(l.rows.filter(r => r.m).map(r => r.m.name), ["Sign off", "Alt macro"]);
            compare(l.rows.find(r => r.header === "Work").count, 2);
        }
        function test_unknown_usage_marks_nothing_and_missing_macros_warn() {
            const l = list();
            compare(l.isUnused(l.lib[1]), true);
            l.usage = null;
            compare(l.isUnused(l.lib[1]), false, "no marker until the daemon answered");
            l.usage = page.usage;
            const w = findChild(page, "missingWarning");
            verify(w.visible);
            verify(findChild(w, "missingWarning") === null);
        }
        function test_rename_duplicate_folder_delete() {
            const l = list();
            const m = l.lib[1];
            // Rename.
            l.begin("rename", m);
            findChild(page, "macroActionField").text = "Goodbye";
            l.apply();
            compare(fakeLib.calls.filter(c => c[0] === "RenameMacro")[0][1], ["g2", "Goodbye"]);
            // Duplicate: the copy opens in the editor.
            l.duplicate(m);
            compare(fakeLib.calls.filter(c => c[0] === "DuplicateMacro")[0][1], ["g2", "Sign off copy"]);
            // Folder: empty text takes the macro out of its folder.
            l.begin("folder", l.lib[0]);
            compare(findChild(page, "macroActionField").text, "Work");
            findChild(page, "macroActionField").text = "Home";
            l.apply();
            compare(fakeLib.calls.filter(c => c[0] === "SetMacroGroup")[0][1], ["g1", "Home"]);
            // Delete names where the macro is used.
            l.begin("delete", l.lib[0]);
            compare(l.usesOf(l.lib[0]), ["Gaming (key 160)"]);
            verify(findChild(page, "deleteWarning").text.indexOf("Gaming (key 160)") > 0);
            l.apply();
            compare(fake.deleted, ["g1"]);
            compare(l.lib.some(m => m.guid === "g1"), false, "the list reloads");
        }
        function test_empty_name_is_refused() {
            const l = list();
            l.begin("rename", l.lib[0]);
            findChild(page, "macroActionField").text = "  ";
            l.apply();
            compare(fakeLib.calls.filter(c => c[0] === "RenameMacro").length, 0);
        }

        // ---- editor additions
        function test_new_event_kinds_insert_and_describe() {
            page.newMacro();
            page.insertKind("media");
            page.insertKind("power");
            page.insertKind("cursor");
            page.insertKind("run");
            compare(page.events.map(e => e.type), ["media", "media", "power", "power", "cursor", "run"]);
            compare(page.describe(page.events[0]), "Play / Pause");
            compare(page.describe(page.events[2]), "System: Wake".replace("System: ", ""));
            compare(page.describe(page.events[4]), "Move cursor to 960, 540");
            compare(page.describe(page.events[5]), "Run command (not set)");
            verify(page.hasEmptyCommand, "a command-less Run event blocks saving");
            page.update(5, { type: "run", command: "konsole" });
            verify(!page.hasEmptyCommand);
            compare(page.describe(page.events[5]), "Run: konsole");
            compare(page.glyph(page.events[0]), "↓");
        }
        function test_recording_filter_and_pointer_tracking() {
            page.newMacro();
            page.recordFilter = 1;     // keys only
            page.record({ type: "button", button: 1, down: true });
            page.record({ type: "cursor", x: 5, y: 6 });
            compare(page.events.length, 0, "keys only ignores the mouse");
            page.record({ type: "key", hid: 4, down: true });
            compare(page.events.length, 1);
            page.recordFilter = 2;     // mouse only
            page.record({ type: "key", hid: 4, down: false });
            page.record({ type: "cursor", x: 5, y: 6 });
            compare(page.events.map(e => e.type).filter(t => t !== "delay"), ["key", "cursor"]);
            page.recordFilter = 0;
        }
    }
}
