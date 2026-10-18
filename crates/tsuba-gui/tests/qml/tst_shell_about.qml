import QtQuick
import QtTest 1.2

Item {
    width: 1000
    height: 900

    QtObject {
        id: app
        function app_version() { return "9.9.9"; }
        function project_url() { return "https://example.invalid/tsuba"; }
        function licence_text() { return "GNU GENERAL PUBLIC LICENSE test text"; }
        function third_party_licences() { return "THIRD PARTY test text"; }
        function system_details() { return JSON.stringify({ app: "9.9.9", daemon: "9.9.9", system: "Test OS", kernel: "7.0", session: "wayland", desktop: "KDE", arch: "x86_64" }); }
    }
    QtObject {
        id: backend
        property string copied: ""
        function copy_text(t) { copied = t; }
    }
    AboutPage {
        id: page
        width: parent.width
        height: parent.height
        backend: backend
        app: app
    }

    TestCase {
        name: "ShellAbout"
        when: windowShown

        function test_version_and_notice() {
            compare(findChild(page, "versionText").text, "Version 9.9.9");
            const n = findChild(page, "noticeText").text;
            verify(n.indexOf("not affiliated") >= 0 && n.indexOf("Razer Inc.") >= 0);
        }

        function test_licence_text_toggles() {
            const box = findChild(page, "licenceText");
            verify(!box.visible);
            mouseClick(findChild(page, "licenceToggle"));
            wait(30);
            verify(box.visible);
            compare(box.text, "GNU GENERAL PUBLIC LICENSE test text");
            mouseClick(findChild(page, "thirdPartyToggle"));
            wait(30);
            compare(box.text, "THIRD PARTY test text");
            mouseClick(findChild(page, "thirdPartyToggle"));
            mouseClick(findChild(page, "licenceToggle"));
            wait(30);
        }

        function test_copy_details_has_no_personal_fields() {
            mouseClick(findChild(page, "copyDetails"));
            verify(backend.copied.indexOf("Tsuba 9.9.9") === 0);
            verify(backend.copied.indexOf("Test OS") > 0);
            verify(backend.copied.indexOf("Kernel: 7.0") > 0);
        }
    }
}
