pragma Singleton

import QtQuick

// What a device model gets in the app: which family of views its kind uses and which tabs the
// device page shows, decided from the daemon's GetDeviceSupport JSON (kind, features, effects…).
// Tested models (BlackWidow V4 Pro, Basilisk V3 Pro) always keep the full tab set.
QtObject {
    // Singular kind name ("Mouse"); `kind` is the daemon's id.
    function kindLabel(kind) {
        switch (kind) {
        case "keyboard": return i18nc("device kind", "Keyboard");
        case "laptop_keyboard": return i18nc("device kind", "Laptop keyboard");
        case "keypad": return i18nc("device kind", "Keypad");
        case "mouse": return i18nc("device kind", "Mouse");
        case "mousemat": return i18nc("device kind", "Mousemat");
        case "mousedock": return i18nc("device kind", "Mouse dock");
        case "headset_stand": return i18nc("device kind", "Headset stand");
        case "headset": return i18nc("device kind", "Headset");
        case "speaker": return i18nc("device kind", "Speaker");
        case "egpu": return i18nc("device kind", "External GPU box");
        case "monitor": return i18nc("device kind", "Monitor");
        case "accessory": return i18nc("device kind", "Accessory");
        case "receiver": return i18nc("device kind", "Wireless receiver");
        default: return i18nc("device kind", "Device");
        }
    }
    // Plural group headings, in the order the supported-devices browser lists them.
    readonly property var groupOrder: ["keyboard", "laptop_keyboard", "keypad", "mouse", "mousemat", "mousedock", "headset_stand",
        "speaker", "egpu", "monitor", "accessory", "receiver", "headset"]
    function groupLabel(kind) {
        switch (kind) {
        case "keyboard": return i18nc("device group heading", "Keyboards");
        case "laptop_keyboard": return i18nc("device group heading", "Laptop keyboards");
        case "keypad": return i18nc("device group heading", "Keypads");
        case "mouse": return i18nc("device group heading", "Mice");
        case "mousemat": return i18nc("device group heading", "Mousemats");
        case "mousedock": return i18nc("device group heading", "Mouse docks");
        case "headset_stand": return i18nc("device group heading", "Headset stands");
        case "headset": return i18nc("device group heading", "Headsets");
        case "speaker": return i18nc("device group heading", "Speakers");
        case "egpu": return i18nc("device group heading", "External GPU boxes");
        case "monitor": return i18nc("device group heading", "Monitors");
        case "accessory": return i18nc("device group heading", "Accessories");
        case "receiver": return i18nc("device group heading", "Wireless receivers");
        default: return i18nc("device group heading", "Other devices");
        }
    }

    // "keyboard" (key views), "mouse" (button views), "lighting" (lighting only), "info" (nothing to control yet).
    function family(kind) {
        switch (kind) {
        case "keyboard":
        case "laptop_keyboard":
        case "keypad":
            return "keyboard";
        case "mouse":
            return "mouse";
        case "mousemat":
        case "mousedock":
        case "headset_stand":
        case "accessory":
        case "speaker":
        case "egpu":
        case "monitor":
            return "lighting";
        default:
            return "info";
        }
    }

    function feat(sup, name) {
        return !!(sup && sup.features && sup.features[name]);
    }

    // Key / button remapping: mice always have buttons; keyboards and keypads only where the
    // database shows on-board macro keys (the same command set), and verified models.
    function remaps(sup) {
        const fam = family(sup.kind);
        if (fam === "mouse") {
            return true;
        }
        return fam === "keyboard" && (!!sup.tested || feat(sup, "onboard_macros") || feat(sup, "dedicated_macro_keys") || (sup.features && sup.features.macro_keys > 0));
    }

    function hasPerformance(sup) {
        return (sup.polling_rates && sup.polling_rates.length > 0) || sup.max_dpi > 0 || feat(sup, "dpi_stages") || feat(sup, "scroll_modes")
            || feat(sup, "scroll_acceleration") || feat(sup, "smart_reel") || feat(sup, "dial") || feat(sup, "game_mode");
    }

    function hasLighting(sup) {
        return (sup.effects && sup.effects.length > 0) || !!sup.matrix || (sup.leds && sup.leds.length > 0);
    }

    // Display title for a tab id from tabsFor() ("Customize", "Performance", "Lighting", "Power", "Info").
    function tabTitle(id) {
        switch (id) {
        case "Customize": return i18nc("device tab", "Customize");
        case "Performance": return i18nc("device tab", "Performance");
        case "Lighting": return i18nc("device tab", "Lighting");
        case "Power": return i18nc("device tab", "Power");
        case "Info": return i18nc("device tab", "Info");
        default: return id;
        }
    }

    // Tab ids for the device page, from the support JSON; `hasPower` is whether the device
    // answered a power query. Without support information (old daemon) the classic three tabs.
    function tabsFor(sup, hasPower) {
        const classic = hasPower ? ["Customize", "Performance", "Lighting", "Power"] : ["Customize", "Performance", "Lighting"];
        if (!sup || !sup.kind) {
            return classic;
        }
        if (sup.supported === false || family(sup.kind) === "info") {
            return ["Info"];
        }
        if (sup.tested) {
            return classic;
        }
        const t = [];
        if (remaps(sup)) {
            t.push("Customize");
        }
        if (hasPerformance(sup)) {
            t.push("Performance");
        }
        if (hasLighting(sup)) {
            t.push("Lighting");
        }
        if (hasPower || feat(sup, "battery")) {
            t.push("Power");
        }
        return t.length ? t : ["Info"];
    }

    // Per-LED painting works from the drawing's LED hotspots on these kinds, when the model has a custom-frame effect.
    function perLed(sup, packKind) {
        const ok = ["keyboard", "laptop_keyboard", "keypad", "mousemat"].indexOf(packKind) >= 0;
        return ok && (!sup || !sup.effects || sup.effects.indexOf("custom_frame") >= 0);
    }

    readonly property var featureIds: ["dpi_stages", "battery", "wireless", "idle_time", "charging_lights", "scroll_modes", "scroll_acceleration", "smart_reel", "dial", "dedicated_macro_keys", "onboard_macros", "game_mode", "keyswitch_optical"]
    function featureLabel(id) {
        switch (id) {
        case "dpi_stages": return i18nc("device feature", "Adjustable sensitivity (DPI) stages");
        case "battery": return i18nc("device feature", "Battery level");
        case "wireless": return i18nc("device feature", "Wireless");
        case "idle_time": return i18nc("device feature", "Sleep timer");
        case "charging_lights": return i18nc("device feature", "Charging light");
        case "scroll_modes": return i18nc("device feature", "Tactile / free-spin wheel");
        case "scroll_acceleration": return i18nc("device feature", "Scroll acceleration");
        case "smart_reel": return i18nc("device feature", "Auto free-spin wheel");
        case "dial": return i18nc("device feature", "Dial");
        case "dedicated_macro_keys": return i18nc("device feature", "Dedicated macro keys");
        case "onboard_macros": return i18nc("device feature", "On-board macros");
        case "game_mode": return i18nc("device feature", "Game mode");
        case "keyswitch_optical": return i18nc("device feature", "Optical key switches");
        default: return id;
        }
    }
    function effectLabel(id) {
        switch (id) {
        case "off": return i18nc("lighting effect", "Off");
        case "static": return i18nc("lighting effect", "Static");
        case "breathing": return i18nc("lighting effect", "Breathing");
        case "spectrum": return i18nc("lighting effect", "Spectrum cycling");
        case "wave": return i18nc("lighting effect", "Wave");
        case "reactive": return i18nc("lighting effect", "Reactive");
        case "starlight": return i18nc("lighting effect", "Starlight");
        case "ripple": return i18nc("lighting effect", "Ripple");
        case "wheel": return i18nc("lighting effect", "Wheel");
        case "custom_frame": return i18nc("lighting effect", "Per-LED colours");
        default: return id;
        }
    }

    // Plain-language feature list for the facts view.
    function featureList(sup) {
        const out = [];
        const f = sup.features || {};
        featureIds.forEach(k => {
            if (f[k]) {
                out.push(featureLabel(k));
            }
        });
        if (f.macro_keys > 0) {
            out.push(i18np("%1 macro key", "%1 macro keys", f.macro_keys));
        }
        return out;
    }
    function effectList(sup) {
        return (sup.effects || []).map(e => effectLabel(e));
    }

    // Status id: "unsupported" / "tested" / "untested" (compare this, not the label).
    function statusId(sup) {
        if (sup.supported === false) {
            return "unsupported";
        }
        return sup.tested ? "tested" : "untested";
    }
    // The status as shown: "Tested" / "Untested" / "Not supported".
    function statusOf(sup) {
        switch (statusId(sup)) {
        case "unsupported": return i18nc("device support status", "Not supported");
        case "tested": return i18nc("device support status", "Tested");
        default: return i18nc("device support status", "Untested");
        }
    }
}
