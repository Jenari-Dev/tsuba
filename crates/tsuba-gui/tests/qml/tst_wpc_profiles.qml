import QtQuick
import QtTest

// Profile menu additions (WP-C): import / export (single, several), a profile from factory
// defaults, lighting that follows the profile, and linked-game badges.
Item {
    width: 1300
    height: 800
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property var calls: []
        property var lighting: null
        signal hardware_event(string pid, string event)
        signal library_changed(string pid)
        signal library_activated(string pid, bool ok, string result)
        signal synapse_scanned(string result)
        signal synapse_progress(string text)
        signal synapse_imported(string result)
        function state(p) { return "{}"; }
        function device_support(p) { return JSON.stringify({ pid: "028d", kind: "keyboard", supported: true, tested: true, allow_writes: true }); }
        function power(p) { return ""; }
        function env_var(n) { return ""; }
        function synapse_stores() { return "[]"; }
        function library_profiles(p) {
            return JSON.stringify({ pid: "028d", profiles: [
                { id: "a1", name: "Fast / Slow: mix?", pinned_slot: 1, in_slot: true, active: true, lighting: lighting },
                { id: "b2", name: "Work", pinned_slot: null, in_slot: false, active: false, lighting: null }],
                active: "a1", active_slot: 1, max_slots: 5, working_slot: null, slots: [{ slot: 1, name: "default" }], free_slots: [2, 3, 4, 5], attached: true });
        }
        function app_links() {
            return JSON.stringify({ links: [{ name: "Elden Ring", matches: "eldenring", device_profiles: { "028d": "a1" } }, { name: "Chess", matches: "chess", device_profiles: { "028d": "a1" } }, { name: "Notes", matches: "notes", device_profiles: { "028d": "b2" } }, { name: "Old", matches: "old", device_profiles: { "028d": 3 } }], active: null });
        }
        function activate_library_profile(p, id) { calls = calls.concat(["activate " + id]); }
    }
    QtObject {
        id: fakeLib
        property var calls: []
        property var failPaths: []
        function call(method, args) {
            const a = JSON.parse(args);
            calls = calls.concat([[method, a]]);
            switch (method) {
            case "Tsuba1.GetLibraryImportPreview":
                if (failPaths.indexOf(a[1]) >= 0)
                    return JSON.stringify({ ok: false, error: "this profile is for device 00aa" });
                return JSON.stringify({ ok: true, value: JSON.stringify({ name: "P", software: a[1].indexOf("cmd") >= 0 ? [{ layer: 0, key: 160, kind: "launch", text: "konsole -e htop" }] : [], digest: a[1].indexOf("cmd") >= 0 ? "d1g3st" : "" }) });
            case "Tsuba1.ImportLibraryProfile":
                return failPaths.indexOf(a[1]) >= 0 ? JSON.stringify({ ok: false, error: "this profile is for device 00aa" }) : JSON.stringify({ ok: true, value: "newid" });
            case "Tsuba1.ExportLibraryProfile":
                return JSON.stringify({ ok: true, value: "" });
            case "ExportLibraryProfiles":
                return JSON.stringify({ ok: true, value: JSON.stringify(["/x/a.tsuba-profile.json", "/x/b.tsuba-profile.json"]) });
            case "CreateFactoryProfile":
                return JSON.stringify({ ok: true, value: "fact1" });
            case "SetProfileLighting":
                return JSON.stringify({ ok: true, value: "" });
            }
            return JSON.stringify({ ok: false, error: "unknown " + method });
        }
        function path_of_url(u) { return String(u).replace("file://", ""); }
        function home_url() { return "file:///home/u"; }
    }
    DevicePage {
        id: page
        anchors.fill: parent
        backend: fake
        pid: "028d"
        Component.onCompleted: lp.lib = fakeLib
    }
    TestCase {
        name: "WpcProfiles"
        when: windowShown

        function init() {
            fakeLib.calls = [];
            fakeLib.failPaths = [];
            fake.calls = [];
            fake.lighting = null;
            page.loadProfiles();
            page.profileStatus = "";
            page.nameMode = "";
        }

        function test_import_reports_each_file() {
            page.importFiles(["file:///t/a.tsuba-profile.json", "file:///t/b.json"]);
            compare(fakeLib.calls.map(c => c[0]), ["Tsuba1.GetLibraryImportPreview", "Tsuba1.ImportLibraryProfile", "Tsuba1.GetLibraryImportPreview", "Tsuba1.ImportLibraryProfile"]);
            compare(fakeLib.calls[3][1], ["028d", "/t/b.json", ""]);
            compare(page.profileStatus, "Imported 2 profiles");
            fakeLib.failPaths = ["/t/wrong.json"];
            page.importFiles(["file:///t/ok.json", "file:///t/wrong.json"]);
            compare(page.profileStatus, "Imported 1 profile; wrong.json: this profile is for device 00aa");
            page.importFiles(["file:///t/wrong.json"]);
            verify(page.profileStatus.indexOf("Failed: wrong.json") === 0, page.profileStatus);
        }
        function test_imported_commands_wait_for_approval() {
            page.importFiles(["file:///t/cmd.json", "file:///t/b.json"]);
            // Stops at the file with a command, listing it; nothing imported yet.
            compare(page.approval.software[0].text, "konsole -e htop");
            compare(fakeLib.calls.filter(c => c[0] === "Tsuba1.ImportLibraryProfile").length, 0);
            page.answerApproval(false);
            const imports = fakeLib.calls.filter(c => c[0] === "Tsuba1.ImportLibraryProfile");
            compare(imports[0][1], ["028d", "/t/cmd.json", ""], "declined: no digest, the commands are left out");
            compare(imports.length, 2);
            compare(page.profileStatus, "Imported 2 profiles");
            fakeLib.calls = [];
            page.importFiles(["file:///t/cmd.json"]);
            page.answerApproval(true);
            compare(fakeLib.calls.filter(c => c[0] === "Tsuba1.ImportLibraryProfile")[0][1], ["028d", "/t/cmd.json", "d1g3st"]);
        }
        function test_export_one_and_several() {
            compare(page.fileNameFor(page.current), "Fast _ Slow_ mix_.tsuba-profile.json");
            page.exportOne("file:///home/u/out.tsuba-profile.json");
            compare(fakeLib.calls[0], ["Tsuba1.ExportLibraryProfile", ["028d", "a1", "/home/u/out.tsuba-profile.json"]]);
            compare(page.profileStatus, "Exported “Fast / Slow: mix?”");
            // Several: nothing ticked → nothing to export; select all ticks every profile.
            page.exportSel = ({});
            compare(page.exportCount, 0);
            page.selectAllForExport(true);
            compare(page.exportCount, 2);
            page.exportSel = ({ "b2": true });
            page.exportMany("file:///home/u/profiles");
            const c = fakeLib.calls[fakeLib.calls.length - 1];
            compare(c[0], "ExportLibraryProfiles");
            compare(c[1], ["028d", JSON.stringify(["b2"]), "/home/u/profiles"]);
            compare(page.profileStatus, "Exported 2 profiles");
        }
        function test_menu_rows_exist_and_the_export_list_ticks() {
            for (const n of ["menuFactory", "menuLighting", "menuImport", "menuExport", "menuExportMany"]) {
                verify(findChild(page, n) !== null, n);
            }
            findChild(page, "moreMenu").open();
            findChild(page, "menuExportMany").picked();
            compare(page.nameMode, "exportmany");
            const all = findChild(page, "exportAll");
            verify(all !== null && all.visible);
            all.toggled();
            all.checked = true;
            page.selectAllForExport(true);
            verify(findChild(page, "exportChosen").enabled);
            page.nameMode = "";
            findChild(page, "moreMenu").close();
        }
        function test_factory_profile_is_created_and_activated() {
            page.nameMode = "factory";
            const field = findChild(page, "profileNameField");
            field.text = "Stock keys";
            findChild(page, "profileNameSave").clicked();
            compare(fakeLib.calls[0], ["CreateFactoryProfile", ["028d", "Stock keys"]]);
            compare(fake.calls[fake.calls.length - 1], "activate fact1", "like Synapse, the new profile becomes the active one");
        }
        function test_lighting_follows_the_profile_toggle() {
            const row = findChild(page, "menuLighting");
            compare(row.caption, "Lighting follows this profile");
            row.picked();
            compare(fakeLib.calls[0], ["SetProfileLighting", ["028d", "a1", "current"]]);
            fake.lighting = { effect: "static", brightness: 60 };
            page.loadProfiles();
            compare(row.caption, "✓ Lighting follows this profile (static)");
            row.picked();
            compare(fakeLib.calls[1], ["SetProfileLighting", ["028d", "a1", "off"]]);
        }
        function test_linked_games_show_on_the_profile_rows() {
            compare(page.linkedApps["a1"], ["Elden Ring", "Chess"]);
            compare(page.linkedApps["b2"], ["Notes"]);
            compare(page.linkedApps[3], undefined, "an old slot link belongs to no profile yet");
        }
    }
}
