// Tsuba focus reporter (KWin script): tells tsubad which window has the keyboard focus, so a
// linked game can apply its profile only while the game is in front. It reports the window's
// process id, class, name and title to tsubad over D-Bus; nothing else leaves KWin and
// nothing is stored.

const TSUBA = ["io.github.JenAriDev.Tsuba", "/io/github/JenAriDev/Tsuba", "io.github.JenAriDev.Tsuba1.Library", "FocusChanged"];

function report(w) {
    if (!w) {
        callDBus(...TSUBA, "0", "", "", "");
        return;
    }
    callDBus(...TSUBA, String(w.pid), String(w.resourceClass), String(w.resourceName), String(w.caption));
}

// Plasma 6 has windowActivated / activeWindow; Plasma 5 has clientActivated / activeClient.
const activated = workspace.windowActivated || workspace.clientActivated;
activated.connect(report);
report(workspace.activeWindow || workspace.activeClient);
