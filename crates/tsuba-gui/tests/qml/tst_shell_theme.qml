import QtQuick
import QtTest 1.2

Item {
    width: 100
    height: 100

    function lum(c) {
        const f = v => v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
        return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    }
    function contrast(a, b) {
        const x = lum(a), y = lum(b);
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
    }

    TestCase {
        name: "ShellTheme"
        when: windowShown

        function cleanup() {
            Theme.prefs.themeMode = "auto";
            Theme.setAccent(Theme.defaultAccent);
        }

        function test_modes() {
            Theme.prefs.themeMode = "dark";
            verify(Theme.dark);
            verify(lum(Theme.bg) < 0.1);
            Theme.prefs.themeMode = "light";
            verify(!Theme.dark);
            verify(lum(Theme.bg) > 0.7);
            // "auto" with no desktop hint counts as dark (the original look)
            Theme.prefs.themeMode = "auto";
            compare(Theme.dark, Qt.styleHints.colorScheme !== Qt.Light);
        }

        function test_text_is_readable_in_both_schemes() {
            for (const mode of ["dark", "light"]) {
                Theme.prefs.themeMode = mode;
                verify(contrast(Theme.text, Theme.bg) >= 7, mode + " text on page " + contrast(Theme.text, Theme.bg));
                verify(contrast(Theme.text, Theme.card) >= 7, mode + " text on card");
                verify(contrast(Theme.textDim, Theme.card) >= 4.5, mode + " dim text on card " + contrast(Theme.textDim, Theme.card));
                verify(contrast(Theme.accentText, Theme.accent) >= 4.5, mode + " text on accent " + contrast(Theme.accentText, Theme.accent));
                verify(contrast(Theme.accent, Theme.card) >= 3, mode + " accent on card " + contrast(Theme.accent, Theme.card));
                verify(contrast(Theme.warn, Theme.card) >= 3, mode + " warn on card");
            }
        }

        function test_pale_accents_get_darker_in_light() {
            Theme.setAccent("#e8e8e8");
            Theme.prefs.themeMode = "dark";
            verify(Qt.colorEqual(Theme.accent, "#e8e8e8"));
            Theme.prefs.themeMode = "light";
            verify(lum(Theme.accent) < lum(Qt.rgba(0.91, 0.91, 0.91, 1)));
            verify(contrast(Theme.accent, Theme.card) >= 3);
            // the user's own choice is kept
            verify(Qt.colorEqual(Theme.userAccent, "#e8e8e8"));
        }

        function test_docs_url() {
            verify(Theme.docsUrl("TROUBLESHOOTING.md").endsWith("/docs/TROUBLESHOOTING.md"));
        }
    }
}
