import QtQuick
import QtTest 1.2

// Macro function in the left-docked assignment panel: picker, playback option, Manage link, SAVE.
Item {
    id: top
    width: 340
    height: 760

    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: "[]"
        property var assigned: []
        function macros() {
            return JSON.stringify({ library: [{ guid: "g1", name: "Copy line" }, { guid: "g2", name: "Sign off" }], devices: {} });
        }
        function env_var(n) { return ""; }
        function assign_macro(pid, json) {
            assigned.push(JSON.parse(json));
            return "Macro: ok";
        }
    }

    AssignmentPanel {
        id: panel
        anchors.fill: parent
        backend: fake
        pid: "00aa"
        target: 4
        targetName: "Button 4"
        profile: 1
    }

    SignalSpy {
        id: manageSpy
        target: panel
        signalName: "openMacros"
    }
    SignalSpy {
        id: closedSpy
        target: panel
        signalName: "closed"
    }

    TestCase {
        name: "MacroPicker"
        when: windowShown

        // Visual-tree search (delegates of a Repeater aren't QObject children of the panel).
        function find(item, name) {
            // Every category delegate carries every editor (only its own is shown): take the shown one.
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
        function init() {
            fake.assigned = [];
            manageSpy.clear();
            closedSpy.clear();
            panel.playback = 0;
            panel.macroIdx = 0;
            panel.openCategory("macro");
            wait(20);
        }
        function test_lists_library_inside_the_panel() {
            compare(panel.macroLib.length, 2);
            const pick = find(panel, "macroPick");
            verify(pick !== null && pick.visible, "dropdown shown in the panel");
            compare(pick.count, 2);
            compare(pick.textAt(1), "Sign off");
            const pb = find(panel, "playbackPick");
            verify(pb.visible);
            compare(pb.count, 5);
            verify(find(panel, "manageMacros").visible, "Manage macros link");
        }
        function test_manage_link_opens_macros_page_only_as_secondary() {
            mouseClick(find(panel, "manageMacros"));
            compare(manageSpy.count, 1);
            compare(closedSpy.count, 0, "panel stays docked");
        }
        function test_save_assigns_chosen_macro_and_playback() {
            panel.macroIdx = 1;
            panel.playback = 2;
            panel.commit();
            compare(fake.assigned.length, 1);
            compare(fake.assigned[0].guid, "g2");
            compare(fake.assigned[0].playback, "while_held");
            compare(fake.assigned[0].key, 4);
            compare(closedSpy.count, 1);
        }
        function test_save_without_macro_category_assigns_nothing() {
            panel.openCategory("media");
            panel.commit();
            compare(fake.assigned.length, 0);
        }
    }
}
