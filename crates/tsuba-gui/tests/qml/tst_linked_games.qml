import QtQuick
import QtTest

Item {
    width: 1300
    height: 800
    QtObject {
        id: fake
        signal devices_changed_on_bus()
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "00aa", name: "Basilisk V3 Pro (wired)", kind: "mouse" }])
        property string saved: ""
        function app_links() { return saved === "" ? JSON.stringify({ links: [], active: null }) : JSON.stringify(Object.assign(JSON.parse(saved), { active: null })); }
        function set_app_links(j) { saved = j; return true; }
        function running_programs() { return JSON.stringify(["eldenring.exe", "firefox"]); }
        function chroma_profiles() { return JSON.stringify({ profiles: [{ name: "fire wheel", devices: {} }], active: "" }); }
        function library_profiles(p) { return JSON.stringify({ active: "a1", active_slot: 1, profiles: [{ id: "a1", name: "Profile 1", pinned_slot: 1 }, { id: "b2", name: "FPS", pinned_slot: null }] }); }
    }
    LinkedGamesPage {
        id: page
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "LinkedGames"
        when: windowShown
        function test_add_and_pick() {
            wait(200);
            page.store(page.links.concat([{ name: "", matches: "", enabled: true, device_profiles: {}, chroma: null }]));
            page.sel = 0;
            page.edit(l => { l.matches = "eldenring.exe"; l.name = "eldenring"; });
            page.edit(l => l.device_profiles["00aa"] = "b2");
            page.edit(l => l.chroma = "fire wheel");
            const saved = JSON.parse(fake.saved).links[0];
            compare(saved.matches, "eldenring.exe");
            compare(saved.device_profiles["00aa"], "b2");
            compare(page.deviceProfiles["00aa"].map(o => o.name), ["Profile 1  (slot 1)", "FPS"]);
            compare(saved.chroma, "fire wheel");
            compare(page.label(saved), "eldenring");
        }
    }
}
