#!/usr/bin/env python3
"""List QML string literals that are not wrapped in i18n()/i18nc()/i18np() and look user-visible.

Usage: tools/scan-i18n.py [--all] [files...]   (default: crates/tsuba-gui/qml/*.qml)
A line with `// i18n-ok` (or a block between `// i18n-ok-begin` and `// i18n-ok-end`) is skipped: key caps,
internal ids, protocol names. Exit status 1 when a suspicious literal is found. Exceptions live in tools/i18n-allow.txt as
`File.qml: literal` lines (or `*: literal` for any file). A literal is suspicious when it contains a
letter-run of 2+ characters, a space or sentence punctuation, is not an identifier-like token, and is
not in an obviously technical position (property key, id, import, enum compare, colour...).
"""
import re, sys, glob, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QML = os.path.join(ROOT, "crates/tsuba-gui/qml")


def tokens(src):
    """Yield (kind, text, line, start, end) for strings; skips comments."""
    i, n, line = 0, len(src), 1
    out = []
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1; i += 1
        elif src.startswith("//", i):
            while i < n and src[i] != "\n": i += 1
        elif src.startswith("/*", i):
            j = src.find("*/", i + 2); j = n if j < 0 else j + 2
            line += src.count("\n", i, j); i = j
        elif c in "\"'":
            j = i + 1; buf = []
            while j < n and src[j] != c:
                if src[j] == "\\":
                    buf.append(src[j:j+2]); j += 2
                else:
                    buf.append(src[j]); j += 1
            out.append((c, "".join(buf), line, i, j + 1)); i = j + 1
        elif c == "`":
            j = i + 1
            while j < n and src[j] != "`":
                j += 2 if src[j] == "\\" else 1
            out.append(("`", src[i+1:j], line, i, j + 1)); line += src.count("\n", i, j); i = j + 1
        else:
            i += 1
    return out


I18N = re.compile(r"i18n[a-z]*\s*\(\s*$")


def wrapped(src, start):
    pre = src[max(0, start - 200):start]
    # first arg of i18n( ... ), or second/third of i18nc( ctx, "..." ) / i18np( "..", "..")
    m = re.search(r"i18n[a-z]*\s*\(\s*(?:(?:\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')\s*,\s*)*$", pre)
    return bool(m)


TECH = re.compile(r"^(/[\w/.\-]*|#[0-9a-fA-F]{3,8}|[a-z][a-zA-Z0-9_]*|[A-Z_]+|[\w./:+\-]*\.(qml|svg|png|json|toml|ini)|qrc:.*|file:.*|image://.*|[\W\d_]*)$")
UI_KEY = re.compile(r"(text|title|label|placeholderText|toolTip|tip|name|description|subtitle|hint|heading|message|caption|summary|status|result|error|detail|body|note|header|display)\w*\s*:\s*$", re.I)


# Positions where a literal is a value being compared / looked up / logged, not text shown to the user.
TECH_BEFORE = re.compile(
    r"(===?|!==?|indexOf\(|replace\(|docsUrl\(|console\.\w+\(|startsWith\(|endsWith\(|\.json\(|\.call\(|case|"
    r"\b(?:id|code|pid|kind|type|key|op|mode|source|style|fontFamily|objectName|category|effect|target|method|icon\w*|"
    r"state|role|name_id|action)\s*:|\[)\s*$"
)


def allow():
    a = set()
    p = os.path.join(ROOT, "tools/i18n-allow.txt")
    if os.path.exists(p):
        for l in open(p, encoding="utf-8"):
            l = l.rstrip("\n")
            if l and not l.startswith("#"): a.add(l)
    return a


def ignored_lines(lines):
    """Lines carrying `// i18n-ok`, or between `// i18n-ok-begin` and `// i18n-ok-end` (key caps, ids...)."""
    out, on = set(), False
    for n, l in enumerate(lines, 1):
        if "i18n-ok-begin" in l:
            on = True
        if on or "i18n-ok" in l:
            out.add(n)
        if "i18n-ok-end" in l:
            on = False
    return out


def scan(files, show_all=False):
    al = allow(); bad = []
    for f in files:
        src = open(f, encoding="utf-8").read()
        lines = src.split("\n")
        skip = ignored_lines(lines)
        base = os.path.basename(f)
        for kind, text, line, s, e in tokens(src):
            if wrapped(src, s) or line in skip:
                continue
            if kind == "`":
                lit = re.sub(r"\$\{[^}]*\}", "", text)
            else:
                lit = text
            if not re.search(r"[A-Za-z぀-ヿ一-鿿]{2}", lit):
                continue
            if f"{base}: {text}" in al or f"*: {text}" in al:
                continue
            before = src[max(0, s - 40):s]
            if TECH_BEFORE.search(before) and not show_all and not re.search(r"\s", text.strip()):
                continue
            ctx = lines[line - 1].strip()
            if TECH.match(text) and not show_all:
                continue
            if re.match(r"^\s*(import|source|id)\b", ctx) and not show_all:
                continue
            bad.append((base, line, text, ctx))
    return bad


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    files = args or sorted(glob.glob(QML + "/*.qml"))
    bad = scan(files, "--all" in sys.argv)
    for b, l, t, c in bad:
        print(f"{b}:{l}: {t!r}   | {c[:110]}")
    print(f"{len(bad)} unwrapped literal(s)", file=sys.stderr)
    sys.exit(1 if bad else 0)
