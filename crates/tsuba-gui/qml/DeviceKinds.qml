pragma Singleton

import QtQuick

// What a device model gets in the app: which family of views its kind uses and which tabs the
// device page shows, decided from the daemon's GetDeviceSupport JSON (kind, features, effects…).
// Tested models (BlackWidow V4 Pro, Basilisk V3 Pro) always keep the full tab set.
QtObject {
    readonly property var kindLabels: ({
        "keyboard": "Keyboard", "laptop_keyboard": "Laptop keyboard", "keypad": "Keypad", "mouse": "Mouse",
        "mousemat": "Mousemat", "mousedock": "Mouse dock", "headset_stand": "Headset stand", "headset": "Headset",
        "speaker": "Speaker", "egpu": "External GPU box", "monitor": "Monitor", "accessory": "Accessory", "receiver": "Wireless receiver"
    })
    // Plural group headings, in the order the supported-devices browser lists them.
    readonly property var groupOrder: ["keyboard", "laptop_keyboard", "keypad", "mouse", "mousemat", "mousedock", "headset_stand",
        "speaker", "egpu", "monitor", "accessory", "receiver", "headset"]
    readonly property var groupLabels: ({
        "keyboard": "Keyboards", "laptop_keyboard": "Laptop keyboards", "keypad": "Keypads", "mouse": "Mice",
        "mousemat": "Mousemats", "mousedock": "Mouse docks", "headset_stand": "Headset stands", "headset": "Headsets",
        "speaker": "Speakers", "egpu": "External GPU boxes", "monitor": "Monitors", "accessory": "Accessories", "receiver": "Wireless receivers"
    })

    function kindLabel(kind) {
        return kindLabels[kind] || "Device";
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

    // Tab names for the device page, from the support JSON; `hasPower` is whether the device
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

    readonly property var featureLabels: ({
        "dpi_stages": "Adjustable sensitivity (DPI) stages",
        "battery": "Battery level",
        "wireless": "Wireless",
        "idle_time": "Sleep timer",
        "charging_lights": "Charging light",
        "scroll_modes": "Tactile / free-spin wheel",
        "scroll_acceleration": "Scroll acceleration",
        "smart_reel": "Auto free-spin wheel",
        "dial": "Dial",
        "dedicated_macro_keys": "Dedicated macro keys",
        "onboard_macros": "On-board macros",
        "game_mode": "Game mode",
        "keyswitch_optical": "Optical key switches"
    })
    readonly property var effectLabels: ({
        "off": "Off", "static": "Static", "breathing": "Breathing", "spectrum": "Spectrum cycling", "wave": "Wave",
        "reactive": "Reactive", "starlight": "Starlight", "ripple": "Ripple", "wheel": "Wheel", "custom_frame": "Per-LED colours"
    })

    // Plain-language feature list for the facts view.
    function featureList(sup) {
        const out = [];
        const f = sup.features || {};
        Object.keys(featureLabels).forEach(k => {
            if (f[k]) {
                out.push(featureLabels[k]);
            }
        });
        if (f.macro_keys > 0) {
            out.push(f.macro_keys + " macro keys");
        }
        return out;
    }
    function effectList(sup) {
        return (sup.effects || []).map(e => effectLabels[e] || e);
    }

    // "Tested" / "Untested" / "Not supported"
    function statusOf(sup) {
        if (sup.supported === false) {
            return "Not supported";
        }
        return sup.tested ? "Tested" : "Untested";
    }
}
