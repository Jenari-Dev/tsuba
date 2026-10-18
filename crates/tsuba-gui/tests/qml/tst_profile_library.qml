import QtQuick
import QtTest

// The device page's profile picker on the PC profile library: more profiles than on-board
// slots, switching through tsubad (asynchronous), pin / unpin and deleting the active profile.
Item {
    width: 1300
    height: 800
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property var calls: []
        property string active: "p3"
        property var pinned: ({ "p1": 1, "p2": 2 })
        property var deleted: []
        signal hardware_event(string pid, string event)
        signal library_changed(string pid)
        signal library_activated(string pid, bool ok, string result)
        signal synapse_scanned(string result)
        signal synapse_progress(string text)
        signal synapse_imported(string result)
        function state(p) { return "{}"; }
        function device_support(p) { return JSON.stringify({ pid: "00aa", kind: "mouse", supported: false, tested: true, allow_writes: true }); }
        function power(p) { return ""; }
        function env_var(n) { return ""; }
        function library_profiles(p) {
            const profiles = [];
            for (let i = 1; i <= 20; i++) {
                const id = "p" + i;
                if (deleted.indexOf(id) >= 0) {
                    continue;
                }
                profiles.push({ id: id, name: "Profile " + i, pinned_slot: pinned[id] || null, in_slot: !!pinned[id], active: id === active, synapse: false });
            }
            const slot = pinned[active] || 3;
            return JSON.stringify({ pid: "00aa", profiles: profiles, active: active, active_slot: slot, max_slots: 5, working_slot: null,
                                    slots: [{ slot: 1, name: "Profile 1" }, { slot: 2, name: "Profile 2" }, { slot: 3, name: "Profile 3" }],
                                    free_slots: [3, 4, 5].filter(s => !Object.values(pinned).includes(s)), attached: true });
        }
        function activate_library_profile(p, id) { calls = calls.concat(["activate " + id]); }
        function delete_library_profile(p, id) { deleted = deleted.concat([id]); calls = calls.concat(["delete " + id]); return true; }
        function pin_library_profile(p, id, slot) { const s = Object.assign({}, pinned); s[id] = 3; pinned = s; calls = calls.concat(["pin " + id]); return 3; }
        function unpin_library_profile(p, id) { const s = Object.assign({}, pinned); delete s[id]; pinned = s; calls = calls.concat(["unpin " + id]); return true; }
        function create_library_profile(p, n, from) { calls = calls.concat(["create " + n]); return "p21"; }
        function synapse_stores() { return "[]"; }
    }
    DevicePage {
        id: page
        anchors.fill: parent
        backend: fake
        pid: "00aa"
    }
    TestCase {
        name: "ProfileLibrary"
        when: windowShown

        function test_list_switch_pin_delete() {
            // Twenty profiles on a device with five slots: all listed, the active one known.
            compare(page.lib.profiles.length, 20);
            compare(page.currentName(), "Profile 3");
            compare(page.profile, 3, "tabs edit the slot the active profile runs in");

            // Picking one asks tsubad to switch, shows "Switching…", then the result.
            page.activate("p12");
            compare(page.switching, "p12");
            compare(fake.calls[fake.calls.length - 1], "activate p12");
            page.activate("p13");
            compare(fake.calls.length, 1, "one switch at a time");
            fake.active = "p12";
            const rev = page.libRev;
            fake.library_activated("00aa", true, JSON.stringify({ id: "p12", slot: 3, writes: 7, ms: 120, warnings: [] }));
            compare(page.switching, "");
            compare(page.currentName(), "Profile 12");
            verify(page.profileStatus.indexOf("Profile 12 is active (7 changes written)") === 0, page.profileStatus);
            verify(page.libRev > rev, "the tabs re-read the slot");

            // A failed switch says why.
            page.activate("p1");
            fake.library_activated("00aa", false, "device kept profile 3");
            compare(page.profileStatus, "Failed: device kept profile 3");

            // The hardware profile button: the pinned profile of the new slot becomes active.
            fake.active = "p2";
            fake.library_changed("00aa");
            wait(400);
            compare(page.currentName(), "Profile 2");
            compare(page.profile, 2);

            // Pin / unpin the active profile.
            fake.active = "p12";
            page.loadProfiles();
            const pinRow = findChild(page, "menuPin");
            compare(pinRow.caption, "Pin to device (slot 3)");
            pinRow.picked();
            compare(fake.calls[fake.calls.length - 1], "pin p12");
            compare(pinRow.caption, "Unpin from slot 3");
            pinRow.picked();
            compare(fake.calls[fake.calls.length - 1], "unpin p12");

            // Deleting the active profile switches to a pinned one first, then deletes.
            page.deleteProfile("p12");
            compare(fake.calls[fake.calls.length - 1], "activate p1");
            fake.active = "p1";
            fake.library_activated("00aa", true, JSON.stringify({ id: "p1", slot: 1, writes: 0, ms: 3, warnings: [] }));
            compare(fake.calls[fake.calls.length - 1], "delete p12");
            compare(page.lib.profiles.length, 19);
        }

        function test_read_only_blocks_switching() {
            fake.readonly = true;
            const n = fake.calls.length;
            page.activate("p5");
            compare(fake.calls.length, n);
            compare(page.switching, "");
            fake.readonly = false;
        }
    }
}
