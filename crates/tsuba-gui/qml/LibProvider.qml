import QtQuick

// Gives a page its LibraryBackend (the Rust client of tsubad's Library interface). It is
// created on demand because the type lives in the app's own QML module, which the headless
// QML tests don't have: they set `lib` to a fake instead, and without either the helpers
// report "not available" rather than failing.
Item {
    id: provider

    property var lib: null
    // The error text of the last failed call.
    property string error: ""
    visible: false

    // Created on first use, so users that start up in any order find it.
    property bool tried: false
    function ensure() {
        if (!lib && !tried) {
            tried = true;
            try {
                lib = Qt.createQmlObject("import io.github.jenaridev.tsuba\nLibraryBackend {}", provider, "LibProvider.qml"); // i18n-ok
            } catch (e) {
                // Under the headless QML tests the app's module doesn't exist: no noise there.
                if (String(e).indexOf("is not installed") < 0) { // i18n-ok
                    console.warn("LibProvider: " + e);
                }
                lib = null;
            }
        }
        return lib;
    }

    // Call a daemon method with string arguments; the reply string, or null on failure.
    function call(method, args) {
        ensure();
        if (!lib || typeof lib.call !== "function") {
            error = i18n("The helper service isn't available");
            return null;
        }
        const r = JSON.parse(lib.call(method, JSON.stringify((args || []).map(String))));
        error = r.ok ? "" : r.error;
        return r.ok ? r.value : null;
    }
    // The reply parsed as JSON, or `fallback` on failure.
    function json(method, args, fallback) {
        const v = call(method, args);
        if (v === null || v === "") {
            return fallback;
        }
        try {
            return JSON.parse(v);
        } catch (e) {
            return fallback;
        }
    }
    function ok(method, args) {
        return call(method, args) !== null;
    }
    function desktopApps() {
        ensure();
        return lib && typeof lib.desktop_apps === "function" ? JSON.parse(lib.desktop_apps()) : [];
    }
    function launchPresets() {
        ensure();
        return lib && typeof lib.launch_presets === "function" ? JSON.parse(lib.launch_presets()) : [];
    }
    function homeUrl() {
        ensure();
        return lib && typeof lib.home_url === "function" ? lib.home_url() : "file:///";
    }
    function pathOfUrl(url) {
        ensure();
        return lib && typeof lib.path_of_url === "function" ? lib.path_of_url(String(url)) : decodeURIComponent(String(url).replace(/^file:\/\//, ""));
    }
}
