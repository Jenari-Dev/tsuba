import QtQuick
import QtTest 1.2

Item {
    width: 1300
    height: 800
    property int calls: 0
    QtObject {
        id: fake
        signal devices_changed_on_bus()
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }])
        property string saved: ""
        property var profileNames: ["", "FPS"]
        property int profileCalls: 0
        function app_links() { return saved === "" ? JSON.stringify({ links: [], active: null }) : JSON.stringify(Object.assign(JSON.parse(saved), { active: null })); }
        function set_app_links(j) { saved = j; return true; }
        function running_programs() { return "[]"; }
        function chroma_profiles() { return JSON.stringify({ profiles: [], active: "" }); }
        function library_profiles(p) {
            profileCalls++;
            return JSON.stringify({ active: "p1", active_slot: 1, profiles: profileNames.map((n, i) => ({ id: "p" + (i + 1), name: n, pinned_slot: null })) });
        }
        function installed_games() {
            return JSON.stringify([
                { source: "steam", id: "550", name: "Left 4 Dead 2", install_dir: "/lib/steamapps/common/Left 4 Dead 2", art: "" },
                { source: "steam", id: "4000", name: "Garry's Mod", install_dir: "/lib/steamapps/common/GarrysMod", art: "" },
                { source: "bottles", id: "b:x", name: "Old Game", install_dir: "", art: "", exe: "old.exe" }
            ]);
        }
        function env_var(n) { return ""; }
    }
    LinkedGamesPage {
        id: page
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "GameScan"
        when: windowShown

        function test_scan_filter_add() {
            const d = page.children.filter(c => c.objectName === "gameScanDialog")[0];
            verify(d, "dialog exists");
            page.scanGames();
            verify(d.visible);
            compare(d.shown.length, 3);
            d.filter = "gar";
            compare(d.shown.length, 1);
            d.filter = "";
            d.toggle(d.games[0], true);
            d.toggle(d.games[2], true);
            compare(d.pickedCount, 2);
            page.addGames(d.chosen());
            verify(!d.visible);
            const l = JSON.parse(fake.saved).links;
            compare(l.length, 2);
            compare(l[0].matches, "dir:/lib/steamapps/common/Left 4 Dead 2");
            compare(l[0].name, "Left 4 Dead 2");
            compare(l[1].matches, "old.exe");
            // Already-linked games are marked on the next scan.
            page.scanGames();
            verify(d.isLinked(d.games[0]));
            verify(!d.isLinked(d.games[1]));
            d.visible = false;
        }

        function test_profile_lists_refresh() {
            page.loadChoices();
            compare(page.deviceProfiles["00aa"].length, 2);
            fake.profileNames = ["", "FPS", "Racing"];
            Theme.profilesRev++;
            compare(page.deviceProfiles["00aa"].length, 3);
            fake.profileNames = ["", "FPS", "Racing", "Extra"];
            page.visible = false;
            page.visible = true;
            compare(page.deviceProfiles["00aa"].length, 4);
            // A device appearing reloads the lists too.
            fake.profileNames = ["", "A"];
            fake.devices = JSON.stringify([{ pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }, { pid: "028d", name: "BlackWidow", kind: "keyboard" }]);
            compare(page.deviceProfiles["028d"].length, 2);
        }
    }
}
