import QtQuick
import QtTest 1.2

Item {
    width: 1300
    height: 800
    QtObject {
        id: fake
        property bool readonly: false
        property string scannedPath: ""
        property string importSel: ""
        signal synapse_scanned(string result)
        signal synapse_progress(string text)
        signal synapse_imported(string result)
        function synapse_stores() { return JSON.stringify(["/mnt/win/Users/a/store.leveldb"]); }
        function synapse_scan(p) { scannedPath = p; }
        function synapse_import(p, sel) { importSel = sel; }
        function env_var(n) { return ""; }
        function view() {
            return JSON.stringify({
                store: "/mnt/win/Users/a/store.leveldb", connected: true, errors: [], ms: 5,
                devices: [
                    { product_id: 170, name: "Basilisk V3 Pro", pid: "00aa", importable: true, verified: true, note: "", attached: true, target: "00aa", writable: true,
                      free_pins: 1,
                      profiles: [{ guid: "m1", name: "Aim", active: true, in_library: false, summary: ["Mouse button 4 → B"], device_wide: [], warnings: [], macros: ["aa"] },
                                 { guid: "m2", name: "Other", active: false, in_library: true, summary: [], device_wide: ["Brightness 50%"], warnings: ["x"], macros: [] }] },
                    { product_id: 653, name: "BlackWidow V4 Pro", pid: "028d", importable: true, verified: true, note: "", attached: true, target: "028d", writable: true,
                      free_pins: 0,
                      profiles: [{ guid: "k1", name: "Typing", active: true, in_library: false, summary: ["M1 → F5"], device_wide: ["Polling rate 1000 Hz"], warnings: [], macros: [] }] },
                    { product_id: 2814, name: "G733", pid: null, importable: false, verified: false, note: "Not in Tsuba's device database.", attached: false, target: null, writable: false,
                      free_pins: 0, profiles: [{ guid: "h", name: "H", active: false, in_library: false, summary: [], device_wide: [], warnings: [], macros: [] }] }
                ],
                macros: [{ guid: "aa", name: "Rapid", events: 4, warnings: [], in_library: false }, { guid: "bb", name: "Other", events: 2, warnings: [], in_library: true }],
                lighting: [{ id: "s1", name: "Stars", layers: 2, devices: ["BlackWidow V4 Pro"], warnings: [], importable: true, exists: false },
                           { id: "s2", name: "Headset only", layers: 0, devices: [], warnings: ["Nothing"], importable: false, exists: false }]
            });
        }
    }
    SynapseImportDialog {
        id: dlg
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "SynapseImport"
        when: windowShown

        function row(k) {
            return findChild(dlg, k);
        }

        function test_scan_pick_import() {
            dlg.open("device:028d");
            verify(dlg.visible);
            compare(dlg.phase, "scanning");
            compare(fake.scannedPath, "/mnt/win/Users/a/store.leveldb");
            fake.synapse_scanned(fake.view());
            compare(dlg.phase, "ready");
            // The device whose page opened the dialog comes first; unknown devices last.
            compare(dlg.devices.map(d => d.pid), ["028d", "00aa", null]);
            wait(50);
            // Profiles go into the library: no slot limit (one free slot on the mouse doesn't cap it).
            verify(row("p:00aa:m1").checkable);
            verify(row("p:00aa:m2").checkable);
            dlg.setPicked("p:00aa:m1", true);
            verify(row("p:00aa:m2").checkable, "picking one doesn't lock the other");
            dlg.setPicked("p:00aa:m2", true);
            dlg.setPicked("p:00aa:m2", false);
            // Pinning is offered only where a slot is free.
            verify(findChild(dlg, "pin:00aa").enabled);
            verify(!findChild(dlg, "pin:028d").enabled, "no free slot on the keyboard");
            dlg.setPin("00aa", true);
            verify(!row("p:null:h").checkable, "unknown devices can't be imported");
            verify(!row("l:s2").checkable);
            // The macro the picked profile uses comes along.
            verify(dlg.neededMacros["aa"]);
            verify(row("m:aa").forcedOn);
            verify(!row("m:bb").forcedOn);
            dlg.setPicked("l:s1", true);
            dlg.deviceSettings = { "028d": "k1" };
            compare(JSON.parse(dlg.selectionJson()), { profiles: { "00aa": ["m1"] }, device_settings: { "028d": "k1" }, pin: { "00aa": true }, macros: [], lighting: ["s1"] });
            verify(dlg.summaryText().indexOf("1 profile, 1 macro, 1 effect") === 0, dlg.summaryText());
            const btn = findChild(dlg, "synapseImport");
            verify(btn.enabled);
            dlg.startImport();
            compare(dlg.phase, "importing");
            compare(JSON.parse(fake.importSel).lighting, ["s1"]);
            fake.synapse_progress("Saving macro “Rapid”…");
            compare(dlg.progressLines.length, 1);
            fake.synapse_imported(JSON.stringify({ done: ["a", "b"], failed: ["c"] }));
            compare(dlg.phase, "done");
            compare(dlg.result.failed.length, 1);
            dlg.close();
            verify(!dlg.visible);
        }

        function test_read_only_and_errors() {
            fake.readonly = true;
            dlg.open("macros");
            fake.synapse_scanned(fake.view());
            dlg.setPicked("m:bb", true);
            verify(!findChild(dlg, "synapseImport").enabled);
            fake.readonly = false;
            verify(findChild(dlg, "synapseImport").enabled);
            dlg.scan();
            fake.synapse_scanned(JSON.stringify({ error: "No Synapse data was found there." }));
            compare(dlg.phase, "error");
            verify(!findChild(dlg, "synapseImport").enabled);
            dlg.close();
        }
    }
}
