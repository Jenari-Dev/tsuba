#!/usr/bin/env python3
"""Generate Tsuba's built-in device drawings (line art) and their device.json packs.

Everything here is Tsuba's own: key geometry is the standard key-unit layout, the mouse outline
is drawn from scratch, and the per-key data (firmware key position, lighting-grid LED) are
hardware facts measured on the devices with `tsuba led-probe` / `tsuba keytest`.

Output: data/devices/<pid>/{device.json, top.svg, wrist.svg, bottom.svg}. Tsuba uses these when
the user has no personal picture pack in ~/.local/share/tsuba/devices/<pid>.

Every other device in the device database (data/devicedb/*.toml) gets a generic pack: a shared
drawing per layout in data/devices/_layouts/ (each pack's top.svg is a relative symlink to it)
and its own device.json marked `"approximate": true`.

Usage: python3 tools/gen-device-art.py   (from the repository root)
"""
import json
import math
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'data', 'devices')

# Line-art palette (neutral greys; lighting colours are drawn by the app on top).
BODY_FILL, BODY_LINE = '#151515', '#4c4c4c'
KEY_FILL, KEY_LINE = '#1d1d1d', '#5c5c5c'
LABEL = '#a8a8a8'
FAINT = '#333333'
FONT = 'Roboto, Noto Sans, sans-serif'


def svg(w, h, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">\n'
            + body + '</svg>\n')


def rr(x, y, w, h, r, fill, stroke, sw=2, extra=''):
    return (f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{r}" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{extra}/>\n')


def text(x, y, s, size, fill=LABEL, anchor='middle', weight='normal'):
    s = s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="{FONT}" font-size="{size}" fill="{fill}" '
            f'text-anchor="{anchor}" font-weight="{weight}">{s}</text>\n')


# ---------------------------------------------------------------------------------------------
# BlackWidow V4 Pro (1532:028d)
#
# Each key: (label, width in units, firmware position, LED (row, col), group tags, name).
# Positions are AT-101 style codes used by the firmware's key-assignment commands; LEDs are the
# custom-frame grid (row, column) — both verified on the hardware.
U = 80          # pixels per key unit
GAP = 7         # gap between neighbouring keycaps

MAIN_ROWS = [
    # y in units, keys
    (1.5, [('` ~', 1, 1, (1, 2), 'alphabets', '` ~'), ('1', 1, 2, (1, 3), 'alphabets,numberRows', '1'),
           ('2', 1, 3, (1, 4), 'alphabets,numberRows', '2'), ('3', 1, 4, (1, 5), 'alphabets,numberRows', '3'),
           ('4', 1, 5, (1, 6), 'alphabets,numberRows', '4'), ('5', 1, 6, (1, 7), 'alphabets,numberRows', '5'),
           ('6', 1, 7, (1, 8), 'alphabets,numberRows', '6'), ('7', 1, 8, (1, 9), 'alphabets,numberRows', '7'),
           ('8', 1, 9, (1, 10), 'alphabets,numberRows', '8'), ('9', 1, 10, (1, 11), 'alphabets,numberRows', '9'),
           ('0', 1, 11, (1, 12), 'alphabets,numberRows', '0'), ('- _', 1, 12, (1, 13), 'alphabets', '- _'),
           ('= +', 1, 13, (1, 14), 'alphabets', '= +'), ('Backspace', 2, 15, (1, 15), 'alphabets', 'Backspace')]),
    (2.5, [('Tab', 1.5, 16, (2, 2), 'alphabets', 'Tab'), ('Q', 1, 17, (2, 3), 'alphabets', 'Q'),
           ('W', 1, 18, (2, 4), 'wasd,alphabets', 'W'), ('E', 1, 19, (2, 5), 'alphabets', 'E'),
           ('R', 1, 20, (2, 6), 'alphabets', 'R'), ('T', 1, 21, (2, 7), 'alphabets', 'T'),
           ('Y', 1, 22, (2, 8), 'alphabets', 'Y'), ('U', 1, 23, (2, 9), 'alphabets', 'U'),
           ('I', 1, 24, (2, 10), 'alphabets', 'I'), ('O', 1, 25, (2, 11), 'alphabets', 'O'),
           ('P', 1, 26, (2, 12), 'alphabets', 'P'), ('[ {', 1, 27, (2, 13), 'alphabets', '[ {'),
           ('] }', 1, 28, (2, 14), 'alphabets', '] }'), ('\\ |', 1.5, 29, (2, 15), 'alphabets', '\\ |')]),
    (3.5, [('Caps Lock', 1.75, 30, (3, 2), 'alphabets', 'Caps Lock'), ('A', 1, 31, (3, 3), 'wasd,alphabets', 'A'),
           ('S', 1, 32, (3, 4), 'wasd,alphabets', 'S'), ('D', 1, 33, (3, 5), 'wasd,alphabets', 'D'),
           ('F', 1, 34, (3, 6), 'alphabets', 'F'), ('G', 1, 35, (3, 7), 'alphabets', 'G'),
           ('H', 1, 36, (3, 8), 'alphabets', 'H'), ('J', 1, 37, (3, 9), 'alphabets', 'J'),
           ('K', 1, 38, (3, 10), 'alphabets', 'K'), ('L', 1, 39, (3, 11), 'alphabets', 'L'),
           ('; :', 1, 40, (3, 12), 'alphabets', '; :'), ("' \"", 1, 41, (3, 13), 'alphabets', "' \""),
           ('Enter', 2.25, 43, (3, 15), 'alphabets', 'Enter')]),
    (4.5, [('Shift', 2.25, 44, (4, 2), 'alphabets', 'Left Shift'), ('Z', 1, 46, (4, 4), 'alphabets', 'Z'),
           ('X', 1, 47, (4, 5), 'alphabets', 'X'), ('C', 1, 48, (4, 6), 'alphabets', 'C'),
           ('V', 1, 49, (4, 7), 'alphabets', 'V'), ('B', 1, 50, (4, 8), 'alphabets', 'B'),
           ('N', 1, 51, (4, 9), 'alphabets', 'N'), ('M', 1, 52, (4, 10), 'alphabets', 'M'),
           (', <', 1, 53, (4, 11), 'alphabets', ', <'), ('. >', 1, 54, (4, 12), 'alphabets', '. >'),
           ('/ ?', 1, 55, (4, 13), 'alphabets', '/ ?'), ('Shift', 2.75, 57, (4, 15), 'alphabets', 'Right Shift')]),
    (5.5, [('Ctrl', 1.25, 58, (5, 2), 'alphabets', 'Left Ctrl'), ('Win', 1.25, 127, (5, 3), 'alphabets', 'Windows / Meta'),
           ('Alt', 1.25, 60, (5, 4), 'alphabets', 'Left Alt'), ('', 6.25, 61, (5, 8), 'alphabets', 'Space'),
           ('Alt', 1.25, 62, (5, 12), 'alphabets', 'Right Alt'), ('Fn', 1.25, 59, (5, 13), 'alphabets', 'Fn'),
           ('Menu', 1.25, 129, (5, 14), 'alphabets', 'Menu'), ('Ctrl', 1.25, 64, (5, 15), 'alphabets', 'Right Ctrl')]),
]
F_ROW = [  # (x in units, label, pos, led, group, name)
    (0, 'Esc', 110, (0, 2), None, 'Esc'),
    (2, 'F1', 112, (0, 4), 'functions', 'F1'), (3, 'F2', 113, (0, 5), 'functions', 'F2'),
    (4, 'F3', 114, (0, 6), 'functions', 'F3'), (5, 'F4', 115, (0, 7), 'functions', 'F4'),
    (6.5, 'F5', 116, (0, 8), 'functions', 'F5'), (7.5, 'F6', 117, (0, 9), 'functions', 'F6'),
    (8.5, 'F7', 118, (0, 10), 'functions', 'F7'), (9.5, 'F8', 119, (0, 11), 'functions', 'F8'),
    (11, 'F9', 120, (0, 12), 'functions', 'F9'), (12, 'F10', 121, (0, 13), 'functions', 'F10'),
    (13, 'F11', 122, (0, 14), 'functions', 'F11'), (14, 'F12', 123, (0, 15), 'functions', 'F12'),
    (15.25, 'PrtSc', 124, (0, 16), None, 'Print Screen'), (16.25, 'ScrLk', 125, (0, 17), None, 'Scroll Lock'),
    (17.25, 'Pause', 126, (0, 18), None, 'Pause'),
]
NAV = [  # (x, y, w, h, label, pos, led, group, name)
    (15.25, 1.5, 1, 1, 'Ins', 75, (1, 16), None, 'Insert'), (16.25, 1.5, 1, 1, 'Home', 80, (1, 17), None, 'Home'),
    (17.25, 1.5, 1, 1, 'PgUp', 85, (1, 18), None, 'Page Up'),
    (15.25, 2.5, 1, 1, 'Del', 76, (2, 16), None, 'Delete'), (16.25, 2.5, 1, 1, 'End', 81, (2, 17), None, 'End'),
    (17.25, 2.5, 1, 1, 'PgDn', 86, (2, 18), None, 'Page Down'),
    (16.25, 4.5, 1, 1, '↑', 83, (4, 17), 'arrows', 'Up'),
    (15.25, 5.5, 1, 1, '←', 79, (5, 16), 'arrows', 'Left'), (16.25, 5.5, 1, 1, '↓', 84, (5, 17), 'arrows', 'Down'),
    (17.25, 5.5, 1, 1, '→', 89, (5, 18), 'arrows', 'Right'),
    (18.5, 1.5, 1, 1, 'Num', 90, (1, 19), 'numpad', 'Num Lock'), (19.5, 1.5, 1, 1, '/', 95, (1, 20), 'numpad', 'Numpad /'),
    (20.5, 1.5, 1, 1, '*', 100, (1, 21), 'numpad', 'Numpad *'), (21.5, 1.5, 1, 1, '-', 105, (1, 22), 'numpad', 'Numpad -'),
    (18.5, 2.5, 1, 1, '7', 91, (2, 19), 'numpad', 'Numpad 7'), (19.5, 2.5, 1, 1, '8', 96, (2, 20), 'numpad', 'Numpad 8'),
    (20.5, 2.5, 1, 1, '9', 101, (2, 21), 'numpad', 'Numpad 9'), (21.5, 2.5, 1, 2, '+', 106, (2, 22), 'numpad', 'Numpad +'),
    (18.5, 3.5, 1, 1, '4', 92, (3, 19), 'numpad', 'Numpad 4'), (19.5, 3.5, 1, 1, '5', 97, (3, 20), 'numpad', 'Numpad 5'),
    (20.5, 3.5, 1, 1, '6', 102, (3, 21), 'numpad', 'Numpad 6'),
    (18.5, 4.5, 1, 1, '1', 93, (4, 19), 'numpad', 'Numpad 1'), (19.5, 4.5, 1, 1, '2', 98, (4, 20), 'numpad', 'Numpad 2'),
    (20.5, 4.5, 1, 1, '3', 103, (4, 21), 'numpad', 'Numpad 3'), (21.5, 4.5, 1, 2, 'Enter', 108, (4, 22), 'numpad', 'Numpad Enter'),
    (18.5, 5.5, 2, 1, '0', 99, (5, 20), 'numpad', 'Numpad 0'), (20.5, 5.5, 1, 1, '.', 104, (5, 21), 'numpad', 'Numpad .'),
]
MACRO = [('M5', 164, (1, 1), 1.5), ('M4', 163, (2, 1), 2.5), ('M3', 162, (3, 1), 3.5),
         ('M2', 161, (4, 1), 4.5), ('M1', 160, (5, 1), 5.5)]
# Lighting-grid (row, column) of the four media keys: ONE line to change in tools/media-leds.json
# (keys previous, play_pause, next, mute). Run `tsuba -d 028d led-guide --confirm` to find out
# which LED belongs to which key, put the answers there, then run this script (and
# tools/patch-pack-leds.py for a personal picture pack).
with open(os.path.join(ROOT, 'tools', 'media-leds.json')) as _f:
    MEDIA_LEDS = {k: tuple(v) for k, v in json.load(_f).items()}
MEDIA = [('Previous', 201, MEDIA_LEDS['previous'], '⏮'), ('Play / Pause', 205, MEDIA_LEDS['play_pause'], '⏯'),
         ('Next', 202, MEDIA_LEDS['next'], '⏭'), ('Mute', 200, MEDIA_LEDS['mute'], '')]
SIDE = [('Side button 1', 207), ('Side button 2', 208), ('Side button 3', 209)]
UNDERGLOW = [(6, c) for c in range(18)] + [(7, c) for c in range(20)]


def blackwidow_v4_pro():
    ox, oy = 3.0 * U, 1.35 * U        # top-left of the main block (Esc) on the canvas
    spots, keys_svg = [], []

    def key(x, y, w, h, label, pos, led, group, name, shape=None, size=None):
        px, py = ox + x * U + GAP / 2, oy + y * U + GAP / 2
        pw, ph = w * U - GAP, h * U - GAP
        if shape == 'round':
            cx, cy, r = px + pw / 2, py + ph / 2, min(pw, ph) / 2
            keys_svg.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" fill="{KEY_FILL}" stroke="{KEY_LINE}" stroke-width="2"/>\n')
        else:
            keys_svg.append(rr(px, py, pw, ph, 9, KEY_FILL, KEY_LINE))
            keys_svg.append(rr(px + 6, py + 5, pw - 12, ph - 14, 6, 'none', '#2a2a2a', 1.5))
        if label:
            fs = size or (22 if len(label) <= 2 else 17 if len(label) <= 5 else 15)
            keys_svg.append(text(px + pw / 2, py + ph / 2 + fs * 0.36 - 3, label, fs))
        h_ = {'name': name, 'pos': pos, 'led': list(led) if led else None, 'group': group,
              'rect': [round(px, 1), round(py, 1), round(pw, 1), round(ph, 1)]}
        if shape:
            h_['shape'] = shape
        spots.append(h_)

    for x, label, pos, led, group, name in F_ROW:
        key(x, 0, 1, 1, label, pos, led, group, name)
    for y, row in MAIN_ROWS:
        x = 0
        for label, w, pos, led, group, name in row:
            key(x, y, w, 1, label, pos, led, group, name)
            x += w
    for x, y, w, h, label, pos, led, group, name in NAV:
        key(x, y, w, h, label, pos, led, group, name)
    for name, pos, led, y in MACRO:
        key(-1.5, y, 1, 1, name, pos, led, 'macro', name)
    # Command dial (top-left) and the round media keys + volume roller (top-right).
    key(-1.6, -0.1, 1.2, 1.2, '', 169, (0, 1), 'extra', 'Command dial', shape='round')
    for i, (name, pos, led, glyph) in enumerate(MEDIA):
        key(18.55 + i * 1.0, 0, 0.9, 0.9, glyph, pos, led, 'multimedia', name, shape='round', size=20)
    # Mute: a small speaker with a cross, drawn (no emoji font needed).
    mx, my = ox + (18.55 + 3) * U + 0.45 * U - GAP / 2 + 3, oy + 0.45 * U
    keys_svg.append(f'<path d="M{mx - 13:.1f} {my - 5:.1f} h7 l8 -7 v24 l-8 -7 h-7 z" fill="{LABEL}"/>\n')
    keys_svg.append(f'<path d="M{mx + 5:.1f} {my - 5:.1f} l10 10 M{mx + 15:.1f} {my - 5:.1f} l-10 10" stroke="{LABEL}" stroke-width="2.5"/>\n')
    rx, ry, rw, rh = ox + 18.6 * U, oy - 0.95 * U, 3.8 * U, 0.6 * U
    keys_svg.append(rr(rx, ry, rw, rh, 14, KEY_FILL, KEY_LINE))
    for i in range(1, 26):   # knurling
        lx = rx + i * rw / 26
        keys_svg.append(f'<line x1="{lx:.1f}" y1="{ry + 7:.1f}" x2="{lx:.1f}" y2="{ry + rh - 7:.1f}" stroke="#3a3a3a" stroke-width="2"/>\n')
    spots.append({'name': 'Volume roller', 'pos': 204, 'led': None, 'group': 'extra',
                  'rect': [round(rx, 1), round(ry, 1), round(rw, 1), round(rh, 1)]})

    # Body outline around everything.
    bx0, by0 = ox - 2.05 * U, oy - 0.45 * U
    bx1, by1 = ox + 22.95 * U, oy + 6.95 * U
    body_svg = rr(bx0, by0, bx1 - bx0, by1 - by0, 22, BODY_FILL, BODY_LINE, 3)
    body_svg += f'<line x1="{bx0 + 20:.1f}" y1="{by1 - 0.38 * U:.1f}" x2="{bx1 - 20:.1f}" y2="{by1 - 0.38 * U:.1f}" stroke="{FAINT}" stroke-width="2"/>\n'

    # Side buttons on the left edge, drawn outside the body (Lighting views crop to the body).
    side_svg = ''
    sx = bx0 - 0.55 * U
    for i, (name, pos) in enumerate(SIDE):
        sy = oy + (1.6 + i * 1.35) * U
        side_svg += rr(sx, sy, 0.32 * U, 1.15 * U, 10, KEY_FILL, KEY_LINE)
        spots.append({'name': name, 'pos': pos, 'led': None, 'group': 'extra',
                      'rect': [round(sx, 1), round(sy, 1), round(0.32 * U, 1), round(1.15 * U, 1)]})

    w, h = int(bx1 + 0.4 * U), int(by1 + 0.2 * U)
    top = svg(w, h, body_svg + side_svg + ''.join(keys_svg))

    # Wrist rest: separate drawing, butted against the body's front edge.
    ww, wh = int(bx1 - bx0), int(2.3 * U)
    lines = ''.join(f'<line x1="{x}" y1="10" x2="{x - 60}" y2="{wh - 10}" stroke="#202020" stroke-width="3"/>\n'
                    for x in range(70, ww + 60, 26))
    wrist = svg(ww, wh, rr(1.5, 1.5, ww - 3, wh - 3, 26, '#121212', BODY_LINE, 3)
                + f'<clipPath id="c"><rect x="6" y="6" width="{ww - 12}" height="{wh - 12}" rx="22"/></clipPath>\n'
                + f'<g clip-path="url(#c)">{lines}</g>\n')
    wrist_rect = [round(bx0, 1), round(by1 + 1, 1), ww, wh]

    body = [round(bx0), round(by0), round(bx1), round(by1)]
    underglow_strips(spots, body, bottom=wrist_rect[1] + wrist_rect[3])
    doc = {'kind': 'keyboard', 'grid': [9, 24], 'art': 'tsuba',
           'views': {'top': {'image': 'top.svg', 'width': w, 'height': h, 'body': body,
                             'wrist': {'image': 'wrist.svg', 'rect': wrist_rect}, 'hotspots': spots}}}
    return doc, {'top.svg': top, 'wrist.svg': wrist}


def underglow_strips(spots, body, gap=10, thick=14, sep=3, bottom=None):
    """Underglow as thin touching segments just outside the outline: row 6 low columns down the
    left side, row 7 left to right along the front, row 6 high columns up the right side."""
    left = [(6, c) for c in range(9)]
    right = [(6, c) for c in range(9, 18)]
    front = [(7, c) for c in range(20)]
    bx0, by0, bx1, by1 = body
    lx, rx, fy = bx0 - gap - thick, bx1 + gap, (bottom or by1) + gap

    def cut(leds, a, b, fixed, vertical):
        n, span, d = len(leds), b - a, (1 if b > a else -1)
        for i, led in enumerate(leds):
            p0, p1 = a + span * i / n, a + span * (i + 1) / n - d * sep
            lo, hi = min(p0, p1), max(p0, p1)
            rect = ([fixed, round(lo, 1), thick, round(hi - lo, 1)] if vertical
                    else [round(lo, 1), fixed, round(hi - lo, 1), thick])
            spots.append({'name': f'Underglow {led[0]}-{led[1]}', 'pos': None, 'led': list(led),
                          'group': 'underglow', 'rect': rect})
    cut(left, by0, fy - sep, lx, True)
    cut(front, lx, rx + thick + sep, fy, False)
    cut(right, fy - sep, by0, rx, True)


# ---------------------------------------------------------------------------------------------
# Basilisk V3 Pro (1532:00aa): Tsuba's own illustration of a right-handed mouse with a flared thumb
# rest (drawn by hand on a coordinate grid; not traced from any render).
# Outline, clockwise from the front of the left button: front edge, right flank, back, thumb rest, left wall.
OUTLINE = [
    (232, 96),
    ((258, 60), (300, 50), (336, 50)),
    ((374, 50), (420, 58), (449, 90)),
    ((488, 134), (507, 214), (511, 300)),
    ((515, 382), (500, 452), (503, 532)),
    ((506, 622), (522, 702), (499, 772)),
    ((471, 852), (402, 878), (336, 878)),
    ((263, 878), (198, 852), (160, 792)),
    ((126, 738), (88, 690), (84, 618)),
    ((80, 560), (102, 522), (140, 500)),
    ((164, 484), (170, 444), (169, 402)),
    ((167, 302), (181, 202), (202, 142)),
    ((211, 118), (219, 107), (232, 96)),
]


def outline_d(pts, dx=0, dy=0):
    x, y = pts[0]
    d = f'M{x + dx:.1f} {y + dy:.1f} '
    for c1, c2, e in pts[1:]:
        d += f'C{c1[0] + dx:.1f} {c1[1] + dy:.1f} {c2[0] + dx:.1f} {c2[1] + dy:.1f} {e[0] + dx:.1f} {e[1] + dy:.1f} '
    return d + 'Z'


def sample_outline(pts, per=80):
    """Points along the outline (cubic Béziers), in drawing order."""
    out, cur = [], pts[0]
    for c1, c2, e in pts[1:]:
        for i in range(per):
            t = i / per
            mt = 1 - t
            out.append((mt ** 3 * cur[0] + 3 * mt * mt * t * c1[0] + 3 * mt * t * t * c2[0] + t ** 3 * e[0],
                        mt ** 3 * cur[1] + 3 * mt * mt * t * c1[1] + 3 * mt * t * t * c2[1] + t ** 3 * e[1]))
        cur = e
    return out


def underglow_path(offset=20, top_y=360):
    """The underglow follows the outline, `offset` px outside it, over the lower part of the body:
    from the left side at `top_y`, round the back, to the right side at `top_y`. Returned in LED order
    (front-left → left → back → right → front-right)."""
    pts = sample_outline(OUTLINE)
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    n = len(pts)
    off = []
    for i, (x, y) in enumerate(pts):
        px, py = pts[i - 1]
        nx_, ny_ = pts[(i + 1) % n]
        tx, ty = nx_ - px, ny_ - py
        ln = math.hypot(tx, ty) or 1
        nx, ny = ty / ln, -tx / ln            # normal; flip so it points away from the centre
        if (x - cx) * nx + (y - cy) * ny < 0:
            nx, ny = -nx, -ny
        off.append((x + nx * offset, y + ny * offset))
    # Drawing order runs right flank → back → left side: keep the stretch below top_y, then reverse.
    first = next(i for i, p in enumerate(off) if p[0] > cx and p[1] > top_y)
    last = max(i for i, p in enumerate(off) if p[0] < cx and p[1] > top_y)
    return off[first:last + 1][::-1]


def split_even(path, n, gap=7.0):
    """Cut a polyline into n touching pieces of equal length (a small gap between them)."""
    lens = [0.0]
    for a, b in zip(path, path[1:]):
        lens.append(lens[-1] + math.dist(a, b))
    total = lens[-1]

    def at(d):
        for i in range(1, len(lens)):
            if lens[i] >= d:
                f = (d - lens[i - 1]) / ((lens[i] - lens[i - 1]) or 1)
                (x0, y0), (x1, y1) = path[i - 1], path[i]
                return (x0 + (x1 - x0) * f, y0 + (y1 - y0) * f)
        return path[-1]
    pieces, mids = [], []
    for k in range(n):
        d0, d1 = total * k / n + gap / 2, total * (k + 1) / n - gap / 2
        steps = 12
        pieces.append([at(d0 + (d1 - d0) * j / steps) for j in range(steps + 1)])
        mids.append(at((d0 + d1) / 2))
    return pieces, mids


def basilisk_v3_pro():
    seam = '#3a3a3a'
    p = []
    p.append(f'<path d="{outline_d(OUTLINE)}" fill="{BODY_FILL}" stroke="{BODY_LINE}" stroke-width="4"/>\n')
    # Main buttons: the two inner edges form the wheel channel; the palm seam runs behind them.
    p.append(f'<path d="M304 54 C302 120 300 210 298 292" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    p.append(f'<path d="M368 54 C370 120 372 210 374 292" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    p.append(f'<path d="M176 336 C218 316 262 300 298 292 M374 292 C420 302 466 320 506 344" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    p.append(f'<path d="M298 292 C320 300 352 300 374 292" stroke="{seam}" stroke-width="2" fill="none"/>\n')
    # Scroll wheel in the channel, then the scroll-mode and sensitivity buttons behind it.
    p.append(rr(314, 128, 44, 122, 20, KEY_FILL, KEY_LINE, 3))
    for y in range(140, 242, 11):
        p.append(f'<line x1="321" y1="{y}" x2="351" y2="{y}" stroke="#2c2c2c" stroke-width="3"/>\n')
    p.append(rr(322, 268, 28, 22, 6, KEY_FILL, KEY_LINE, 2.5))
    p.append(rr(322, 306, 28, 30, 6, KEY_FILL, KEY_LINE, 2.5))
    # Side grips: a panel line just inside each flank.
    p.append(f'<path d="M488 236 C497 330 488 450 491 560 C494 640 500 690 486 740" stroke="#262626" stroke-width="10" fill="none" stroke-linecap="round"/>\n')
    p.append(f'<path d="M186 206 C180 280 180 360 184 430" stroke="#262626" stroke-width="10" fill="none" stroke-linecap="round"/>\n')
    # Thumb rest shelf edge.
    p.append(f'<path d="M142 512 C120 560 112 640 140 716" stroke="{seam}" stroke-width="2.5" fill="none"/>\n')
    # Thumb side: sensitivity clutch at the front, then the two side buttons.
    p.append(rr(184, 214, 22, 56, 9, KEY_FILL, KEY_LINE, 2.5))
    p.append(rr(168, 300, 22, 76, 9, KEY_FILL, KEY_LINE, 2.5))
    p.append(rr(166, 392, 22, 76, 9, KEY_FILL, KEY_LINE, 2.5))
    # Logo plate (a plain emblem area; the app lights it).
    p.append(rr(300, 650, 72, 72, 18, 'none', '#353535', 3))
    p.append('<circle cx="336" cy="686" r="16" fill="none" stroke="#454545" stroke-width="3"/>\n')
    # Underglow: 11 segments following the outline round the lower body.
    pieces, mids = split_even(underglow_path(), 11)
    for seg in pieces:
        d = 'M' + ' L'.join(f'{x:.1f} {y:.1f}' for x, y in seg)
        p.append(f'<path d="{d}" stroke="#4a4a4a" stroke-width="9" stroke-linecap="round" stroke-linejoin="round" fill="none"/>\n')

    # Tight canvas round the drawing.
    xs = [x for seg in pieces for x, _ in seg] + [80]
    w, h = int(max(xs) + 30), 905
    top = svg(w, h, ''.join(p))

    hot = [('Left Click', 1, (250, 140), 'left'), ('Right Click', 2, (422, 140), 'right'),
           ('Scroll Up', 9, (336, 150), 'right'), ('Scroll Click', 3, (336, 189), 'right'),
           ('Scroll Down', 10, (336, 228), 'right'), ('Scroll Left', 52, (308, 189), 'left'),
           ('Scroll Right', 53, (364, 189), 'right'), ('Scroll Mode Toggle', 106, (336, 279), 'right'),
           ('Sensitivity Stage Up', 96, (336, 321), 'right'), ('Sensitivity Clutch', 15, (195, 242), 'left'),
           ('Mouse Button 5', 5, (179, 338), 'left'), ('Mouse Button 4', 4, (177, 430), 'left')]
    leds = [{'name': 'scroll wheel', 'anchor': [336, 189]}, {'name': 'logo', 'anchor': [336, 686]}]
    leds += [{'name': f'underglow {i + 1}', 'anchor': [round(x, 1), round(y, 1)]} for i, (x, y) in enumerate(mids)]

    # Underside inset: a round view of the base with the profile button.
    b = []
    b.append('<circle cx="172" cy="172" r="168" fill="#121212" stroke="#4c4c4c" stroke-width="4"/>\n')
    b.append('<path d="M80 40 C120 10 224 10 264 40" stroke="#2c2c2c" stroke-width="6" fill="none"/>\n')
    b.append(rr(156, 108, 32, 80, 14, KEY_FILL, KEY_LINE, 3))
    b.append('<circle cx="172" cy="250" r="22" fill="none" stroke="#3c3c3c" stroke-width="4"/>\n')
    b.append(text(172, 222, 'PROFILE', 15, '#6a6a6a'))
    bottom = svg(344, 345, ''.join(b))

    doc = {'kind': 'mouse', 'art': 'tsuba',
           'views': {'top': {'image': 'top.svg', 'width': w, 'height': h,
                             'hotspots': [{'name': n, 'pos': pos, 'anchor': list(a), 'side': s} for n, pos, a, s in hot],
                             'leds': leds},
                     'bottom': {'image': 'bottom.svg', 'width': 344, 'height': 345,
                                'hotspots': [{'name': 'Profile Button', 'pos': 14, 'anchor': [172, 148], 'side': 'left'}],
                                'inset': {'cx': 172, 'cy': 172, 'r': 168}}}}
    return doc, {'top.svg': top, 'bottom.svg': bottom}


# ---------------------------------------------------------------------------------------------
# Generic packs for every other device in the database (data/devicedb/*.toml).
#
# One line drawing per layout is shared by all devices with that layout
# (data/devices/_layouts/<layout>.svg; each device's top.svg is a relative symlink to it), while
# every device gets its own device.json: hotspots, LED (row, column) guesses and its name. Key
# positions use the standard key-unit layouts; LED positions follow the usual row/column order of
# per-key Razer keyboards (Esc in row 0, numbers in row 1, …) and are best-effort, so these packs
# say `"approximate": true`. Firmware key positions are the standard AT-101 codes (the same ones
# the verified BlackWidow V4 Pro uses).
LAYOUT_DIR = '_layouts'


def _std_tables():
    """name → (standard LED (row, col), AT-101 position) from the verified keyboard's tables.
    The verified keyboard's grid has one extra leading column, so col - 1 is the usual column."""
    rc, pos = {}, {}
    for _, row in MAIN_ROWS:
        for _label, _w, p, led, _g, name in row:
            rc[name], pos[name] = (led[0], led[1] - 1), p
    for _x, _label, p, led, _g, name in F_ROW:
        rc[name], pos[name] = (led[0], led[1] - 1), p
    for *_, _label, p, led, _g, name in NAV:
        rc[name], pos[name] = (led[0], led[1] - 1), p
    for name, p, led, _y in MACRO:
        rc[name], pos[name] = (led[0], led[1] - 1), p
    return rc, pos


STD_RC, STD_POS = _std_tables()


def _main_rows(right_col=None, short_shift=False, menu=True):
    """ANSI main block as rows of (label, width, name, group). `right_col`: labels of an extra
    1-unit column on rows 1–3 (65 %/75 % boards); `short_shift` makes room for an Up arrow."""
    g, a = 'alphabets', 'arrows'
    r1 = [('` ~', 1, '` ~', g)] + [(c, 1, c, g) for c in '1234567890'] + [('- _', 1, '- _', g), ('= +', 1, '= +', g), ('Backspace', 2, 'Backspace', g)]
    r2 = [('Tab', 1.5, 'Tab', g)] + [(c, 1, c, g) for c in 'QWERTYUIOP'] + [('[ {', 1, '[ {', g), ('] }', 1, '] }', g), ('\\ |', 1.5, '\\ |', g)]
    r3 = [('Caps Lock', 1.75, 'Caps Lock', g)] + [(c, 1, c, g) for c in 'ASDFGHJKL'] + [('; :', 1, '; :', g), ("' \"", 1, "' \"", g), ('Enter', 2.25, 'Enter', g)]
    r4 = [('Shift', 2.25, 'Left Shift', g)] + [(c, 1, c, g) for c in 'ZXCVBNM'] + [(', <', 1, ', <', g), ('. >', 1, '. >', g), ('/ ?', 1, '/ ?', g)]
    if short_shift:
        r4 += [('Shift', 1.75, 'Right Shift', g), ('↑', 1, 'Up', a)]
        r5 = [('Ctrl', 1.25, 'Left Ctrl', g), ('Win', 1.25, 'Windows / Meta', g), ('Alt', 1.25, 'Left Alt', g),
              ('', 6.25, 'Space', g), ('Alt', 1, 'Right Alt', g), ('Fn', 1, 'Fn', g), ('Ctrl', 1, 'Right Ctrl', g),
              ('←', 1, 'Left', a), ('↓', 1, 'Down', a), ('→', 1, 'Right', a)]
    else:
        r4 += [('Shift', 2.75, 'Right Shift', g)]
        r5 = [('Ctrl', 1.25, 'Left Ctrl', g), ('Win', 1.25, 'Windows / Meta', g), ('Alt', 1.25, 'Left Alt', g),
              ('', 6.25, 'Space', g), ('Alt', 1.25, 'Right Alt', g), ('Fn', 1.25, 'Fn', g)]
        r5 += [('Menu', 1.25, 'Menu', g)] if menu else []
        r5 += [('Ctrl', 1.25, 'Right Ctrl', g)]
    rows = [r1, r2, r3, r4, r5]
    if right_col:
        for r, (label, name) in zip(rows, right_col):
            r.append((label, 1, name, None))
    return rows


def keyboard_keys(layout, macro=False):
    """Every key as (x, y, w, h, label, name, group, drawing row) in key units."""
    keys = []
    if layout in ('full', 'tkl'):
        frow = [(0, 'Esc', 'Esc')] + [(x, f'F{i + 1}', f'F{i + 1}') for i, x in enumerate([2, 3, 4, 5, 6.5, 7.5, 8.5, 9.5, 11, 12, 13, 14])]
        frow += [(15.25, 'PrtSc', 'Print Screen'), (16.25, 'ScrLk', 'Scroll Lock'), (17.25, 'Pause', 'Pause')]
        for x, label, name in frow:
            keys.append((x, 0, 1, 1, label, name, 'functions' if label.startswith('F') else None, 0))
        for i, row in enumerate(_main_rows()):
            x = 0
            for label, w, name, group in row:
                keys.append((x, 1.5 + i, w, 1, label, name, group, i + 1))
                x += w
        for x, y, w, h, label, _p, _led, group, name in NAV:
            if group == 'numpad' and layout == 'tkl':
                continue
            keys.append((x, y, w, h, label, name, group, int(y - 0.5)))
        if macro:
            for name, _p, _led, y in MACRO:
                keys.append((-1.5, y, 1, 1, name, name, 'macro', int(y - 0.5)))
    elif layout in ('75', 'laptop'):
        keys.append((0, 0, 1, 1, 'Esc', 'Esc', None, 0))
        for i in range(12):
            keys.append((1.25 + i * 1.0 + (i // 4) * 0.25, 0, 1, 1, f'F{i + 1}', f'F{i + 1}', 'functions', 0))
        keys.append((15, 0, 1, 1, 'Del', 'Delete', None, 0))
        for i, row in enumerate(_main_rows(right_col=[('Home', 'Home'), ('PgUp', 'Page Up'), ('PgDn', 'Page Down')], short_shift=True)):
            x = 0
            for label, w, name, group in row:
                keys.append((x, 1.25 + i, w, 1, label, name, group, i + 1))
                x += w
    elif layout == '65':
        for i, row in enumerate(_main_rows(right_col=[('Del', 'Delete'), ('PgUp', 'Page Up'), ('PgDn', 'Page Down')], short_shift=True)):
            x = 0
            for label, w, name, group in row:
                keys.append((x, i, w, 1, 'Esc' if name == '` ~' else label, 'Esc' if name == '` ~' else name, group, i))
                x += w
    elif layout == '60':
        for i, row in enumerate(_main_rows()):
            x = 0
            for label, w, name, group in row:
                keys.append((x, i, w, 1, 'Esc' if name == '` ~' else label, 'Esc' if name == '` ~' else name, group, i))
                x += w
    elif layout == 'keypad':
        # A gaming keypad: four rows of five keys (row 3 has four), a thumb key and a thumb pad.
        n = 1
        for r in range(4):
            for c in range(5 if r < 3 else 4):
                keys.append((c + r * 0.15, r, 1, 1, f'{n:02d}', f'Key {n:02d}', 'keypad', r))
                n += 1
        keys.append((5.4, 3.3, 1.6, 0.9, '', 'Thumb key', 'keypad', 4))
        keys.append((5.6, 1.9, 1.2, 1.2, '', 'Thumb pad', 'keypad', 4))
    return keys


def _key_svg(px, py, pw, ph, label, round_=False):
    out = []
    if round_:
        out.append(f'<circle cx="{px + pw / 2:.1f}" cy="{py + ph / 2:.1f}" r="{min(pw, ph) / 2:.1f}" fill="{KEY_FILL}" stroke="{KEY_LINE}" stroke-width="2"/>\n')
    else:
        out.append(rr(px, py, pw, ph, 9, KEY_FILL, KEY_LINE))
        out.append(rr(px + 6, py + 5, pw - 12, ph - 14, 6, 'none', '#2a2a2a', 1.5))
    if label:
        fs = 22 if len(label) <= 2 else 17 if len(label) <= 5 else 15
        out.append(text(px + pw / 2, py + ph / 2 + fs * 0.36 - 3, label, fs))
    return ''.join(out)


def keyboard_drawing(layout, macro):
    """(svg, geometry) for a keyboard layout; geometry = list of (key tuple, pixel rect) + body."""
    keys = keyboard_keys(layout, macro)
    xs0 = min(k[0] for k in keys)
    xs1 = max(k[0] + k[2] for k in keys)
    ys1 = max(k[1] + k[3] for k in keys)
    pad = 0.45 if layout != 'laptop' else 0.6
    ox, oy = (pad - xs0) * U + 40, pad * U + 40
    placed = []
    body = []
    bx0, by0 = ox + (xs0 - pad) * U, oy - pad * U
    bx1, by1 = ox + (xs1 + pad) * U, oy + (ys1 + pad) * U
    extra_h = 0
    if layout == 'laptop':
        extra_h = 4.2 * U    # palm rest + touchpad
    body.append(rr(bx0, by0, bx1 - bx0, by1 - by0 + extra_h, 22 if layout != 'keypad' else 60, BODY_FILL, BODY_LINE, 3))
    if layout == 'laptop':
        tw, th = 5.5 * U, 3.2 * U
        body.append(rr((bx0 + bx1) / 2 - tw / 2, by1 + 0.4 * U, tw, th, 16, '#121212', '#3c3c3c', 2.5))
    if layout == 'keypad':   # wrist rest outline
        body.append(rr(bx0 + 0.3 * U, by1 + 0.2 * U, (bx1 - bx0) * 0.7, 2.2 * U, 50, '#121212', BODY_LINE, 3))
    parts = []
    for k in keys:
        x, y, w, h, label, name, group, row = k
        px, py = ox + x * U + GAP / 2, oy + y * U + GAP / 2
        pw, ph = w * U - GAP, h * U - GAP
        parts.append(_key_svg(px, py, pw, ph, label, round_=(name == 'Thumb pad')))
        placed.append((k, [round(px, 1), round(py, 1), round(pw, 1), round(ph, 1)]))
    w = int(bx1 + 40)
    h = int(by1 + extra_h + (2.6 * U if layout == 'keypad' else 0) + 40)
    return svg(w, h, ''.join(body) + ''.join(parts)), placed, [round(bx0), round(by0), round(bx1), round(by1 + extra_h)], w, h


def keyboard_led(dev, k, rect, width):
    """Best-effort LED (row, col) for one key of a database keyboard."""
    x, y, w, h, label, name, group, row = k
    matrix = dev.get('matrix')
    if not matrix:
        return None
    rows, cols = matrix
    if rows * cols <= 1:
        return None
    layout = dev['layout']
    if layout in ('full', 'tkl') and rows >= 6 and cols >= 17 and name in STD_RC:
        r, c = STD_RC[name]
        c += 1 if cols >= 23 else 0
        return [r, c] if r < rows and c < cols else None
    if rows == 1:   # zoned: split the board left to right
        cx = rect[0] + rect[2] / 2
        return [0, min(cols - 1, int(cx / width * cols))]
    return None


def sequential_leds(dev, placed):
    """Compact boards and keypads: LEDs numbered left to right within each drawn row."""
    matrix = dev.get('matrix')
    out = {}
    if not matrix or matrix[0] < 2:
        return out
    rows, cols = matrix
    by_row = {}
    for k, rect in placed:
        by_row.setdefault(k[7], []).append((k, rect))
    for r, items in by_row.items():
        for c, (k, rect) in enumerate(sorted(items, key=lambda it: it[1][0])):
            if r < rows and c < cols:
                out[k[5]] = [r, c]
    return out


# ---- mice ---------------------------------------------------------------------------------

def _catmull(points):
    """Closed Catmull-Rom spline through `points` as an SVG path."""
    n = len(points)
    d = f'M{points[0][0]:.1f} {points[0][1]:.1f} '
    for i in range(n):
        p0, p1, p2, p3 = points[i - 1], points[i], points[(i + 1) % n], points[(i + 2) % n]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f'C{c1[0]:.1f} {c1[1]:.1f} {c2[0]:.1f} {c2[1]:.1f} {p2[0]:.1f} {p2[1]:.1f} '
    return d + 'Z'


MOUSE_W, MOUSE_H, MCX, MY0, MY1 = 680, 905, 340.0, 50.0, 870.0


def mouse_outline(ambi):
    """Outline points clockwise from the front centre; a right-handed mouse has a fuller thumb
    side (left) and a narrower right flank."""
    pts_r, pts_l = [], []
    for i in range(41):
        t = i / 40
        y = MY0 + (MY1 - MY0) * t
        base = 165 * math.sin(math.pi * min(max(t, 0.02), 0.98)) ** (0.32 if t < 0.5 else 0.5) * (0.80 + 0.25 * t)
        if ambi:
            wl = wr = base * (1 - 0.10 * math.exp(-((t - 0.5) / 0.12) ** 2))    # slight waist
        else:
            wl = base * (1 + 0.20 * math.exp(-((t - 0.62) / 0.16) ** 2))
            wr = base * 0.94
        pts_r.append((MCX + wr, y))
        pts_l.append((MCX - wl, y))
    return [(MCX, MY0)] + pts_r[1:-1] + [(MCX, MY1)] + pts_l[-2:0:-1]


def _side_at(outline, t, left):
    y = MY0 + (MY1 - MY0) * t
    xs = [p[0] for p in outline if abs(p[1] - y) < 12 and ((p[0] < MCX) == left)]
    return (min(xs) if left else max(xs)) if xs else MCX


def mouse_drawing(ambi):
    o = mouse_outline(ambi)
    seam = '#3a3a3a'
    p = [f'<path d="{_catmull(o)}" fill="{BODY_FILL}" stroke="{BODY_LINE}" stroke-width="4"/>\n']
    split = MY0 + 0.36 * (MY1 - MY0)
    p.append(f'<path d="M{MCX - 32} {MY0 + 6} L{MCX - 30} {split}" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    p.append(f'<path d="M{MCX + 32} {MY0 + 6} L{MCX + 30} {split}" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    lx, rx = _side_at(o, 0.38, True), _side_at(o, 0.38, False)
    p.append(f'<path d="M{lx + 8:.1f} {split + 40:.1f} Q{MCX} {split - 10:.1f} {rx - 8:.1f} {split + 40:.1f}" stroke="{seam}" stroke-width="3" fill="none"/>\n')
    wheel = (MCX - 20, MY0 + 80, 40, 110)
    p.append(rr(*wheel, 18, KEY_FILL, KEY_LINE, 3))
    for y in range(int(wheel[1]) + 12, int(wheel[1] + wheel[3]) - 6, 11):
        p.append(f'<line x1="{MCX - 13}" y1="{y}" x2="{MCX + 13}" y2="{y}" stroke="#2c2c2c" stroke-width="3"/>\n')
    dpi = (MCX - 13, MY0 + 210, 26, 34)
    p.append(rr(*dpi, 6, KEY_FILL, KEY_LINE, 2.5))
    sides = []
    for left in ([True, False] if ambi else [True]):
        for i, t in enumerate((0.30, 0.41)):
            x = _side_at(o, t, left)
            y = MY0 + (MY1 - MY0) * t
            bx = x + 10 if left else x - 32
            p.append(rr(bx, y - 34, 22, 66, 9, KEY_FILL, KEY_LINE, 2.5))
            sides.append((left, i, (bx + 11, y)))
    logo = (MCX, MY0 + 0.78 * (MY1 - MY0))
    p.append(f'<circle cx="{logo[0]}" cy="{logo[1]:.1f}" r="30" fill="none" stroke="#353535" stroke-width="3"/>\n')
    return svg(MOUSE_W, MOUSE_H, ''.join(p)), o, wheel, dpi, sides, logo


def mouse_pack(dev, drawing):
    _svg, o, wheel, dpi, sides, logo = drawing
    wx, wy = wheel[0] + wheel[2] / 2, wheel[1] + wheel[3] / 2
    hot = [('Left Click', 1, (MCX - 90, MY0 + 110), 'left'), ('Right Click', 2, (MCX + 90, MY0 + 110), 'right'),
           ('Scroll Up', 9, (wx, wheel[1] + 20), 'right'), ('Scroll Click', 3, (wx, wy), 'right'),
           ('Scroll Down', 10, (wx, wheel[1] + wheel[3] - 20), 'right'),
           ('Sensitivity Stage Up', 96, (MCX, dpi[1] + dpi[3] / 2), 'right')]
    for left, i, (x, y) in sides:
        hot.append((f'{"Left" if left else "Right"} side button {i + 1}' if len(sides) > 2 else f'Mouse Button {5 - i}',
                    (5 - i) if left else (7 - i), (x, y), 'left' if left else 'right'))
    zones = list(dev.get('leds', []))
    anchors = {'scroll_wheel': (wx, wy), 'logo': logo, 'backlight': (MCX, MY0 + 0.6 * (MY1 - MY0)),
               'left_side': (_side_at(o, 0.6, True) - 16, MY0 + 0.6 * (MY1 - MY0)),
               'right_side': (_side_at(o, 0.6, False) + 16, MY0 + 0.6 * (MY1 - MY0)),
               'underglow': (MCX, MY1 + 18)}
    names = {'scroll_wheel': 'scroll wheel', 'left_side': 'left side', 'right_side': 'right side'}
    leds = [{'name': names.get(z, z), 'anchor': [round(anchors[z][0], 1), round(anchors[z][1], 1)]} for z in zones if z in anchors]
    matrix = dev.get('matrix')
    if matrix and matrix[0] * matrix[1] > 1:
        # Per-LED mice: number the zones first, then spread the rest round the lower outline.
        n = matrix[1]
        for i, l in enumerate(leds[:n]):
            l['led'] = [0, i]
        rest = n - len(leds)
        if rest > 0:
            start = 0.7 if ('left_side' in zones or 'right_side' in zones) else 0.45
            lower = [pt for pt in o if pt[1] > MY0 + start * (MY1 - MY0)]
            for j in range(rest):
                x, y = lower[int(j * (len(lower) - 1) / max(rest - 1, 1))]
                dx = x - MCX
                leds.append({'name': f'LED {len(leds) + 1}', 'anchor': [round(x + (16 if dx > 0 else -16), 1), round(y, 1)],
                             'led': [0, len(leds)]})
    return {'hotspots': [{'name': n_, 'pos': pos, 'anchor': [round(a[0], 1), round(a[1], 1)], 'side': s_} for n_, pos, a, s_ in hot],
            'leds': leds}


# ---- strips, stands, docks, accessories ---------------------------------------------------

def _segments(path_pts, n, thick=14):
    """Cut a polyline into n LED boxes (axis-aligned rects round each piece's midpoint)."""
    if n <= 0:
        return []
    pieces, mids = split_even(path_pts, n, gap=8)
    rects = []
    for seg in pieces:
        xs, ys = [p[0] for p in seg], [p[1] for p in seg]
        x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        if x1 - x0 < thick:
            x0, x1 = (x0 + x1) / 2 - thick / 2, (x0 + x1) / 2 + thick / 2
        if y1 - y0 < thick:
            y0, y1 = (y0 + y1) / 2 - thick / 2, (y0 + y1) / 2 + thick / 2
        rects.append([round(x0, 1), round(y0, 1), round(x1 - x0, 1), round(y1 - y0, 1)])
    return rects


def _led_spots(rects, prefix='LED'):
    return [{'name': f'{prefix} {i + 1}', 'pos': None, 'led': [0, i], 'group': 'lighting', 'rect': r} for i, r in enumerate(rects)]


def _draw_rects(rects):
    return ''.join(rr(*r, 4, '#262626', '#4a4a4a', 2) for r in rects)


def mousemat_drawing():
    w, h = 1000, 760
    mat = (60, 60, 880, 640)
    body = rr(*mat, 26, '#131313', BODY_LINE, 3)
    body += rr(mat[0] + 30, mat[1] + 30, mat[2] - 60, mat[3] - 60, 16, 'none', '#222222', 2)
    return svg(w, h, body), mat


def mousemat_pack(dev, mat):
    x, y, w, h = mat
    path = [(x - 18, y + h - 20), (x - 18, y - 18), (x + w + 18, y - 18), (x + w + 18, y + h - 20)]
    n = (dev.get('matrix') or [1, 1])[1]
    rects = _segments(path, n) if n > 1 else [[x - 26, y - 26, w + 52, 16]]
    return _led_spots(rects, 'Edge LED'), rects


def headset_stand_drawing():
    w, h = 600, 820
    base = (90, 640, 420, 110)
    p = rr(*base, 50, BODY_FILL, BODY_LINE, 3)
    p += rr(265, 150, 70, 500, 20, BODY_FILL, BODY_LINE, 3)
    p += f'<path d="M190 175 Q300 70 410 175" stroke="{BODY_LINE}" stroke-width="22" fill="none" stroke-linecap="round"/>\n'
    p += f'<path d="M190 175 Q300 70 410 175" stroke="{BODY_FILL}" stroke-width="14" fill="none" stroke-linecap="round"/>\n'
    p += '<circle cx="300" cy="690" r="18" fill="none" stroke="#3c3c3c" stroke-width="3"/>\n'
    return svg(w, h, p), base


def headset_stand_pack(dev, base):
    x, y, w, h = base
    path = [(x - 16, y + 30), (x - 16, y + h + 16), (x + w + 16, y + h + 16), (x + w + 16, y + 30)]
    n = (dev.get('matrix') or [1, 1])[1]
    rects = _segments(path, n) if n > 1 else [[x, y + h + 10, w, 14]]
    return _led_spots(rects, 'Base LED'), rects


def mousedock_drawing():
    w, h = 600, 600
    p = '<circle cx="300" cy="300" r="230" fill="#141414" stroke="#4c4c4c" stroke-width="4"/>\n'
    p += '<circle cx="300" cy="300" r="150" fill="none" stroke="#2c2c2c" stroke-width="3"/>\n'
    p += rr(280, 90, 40, 26, 6, KEY_FILL, KEY_LINE, 2)
    return svg(w, h, p), (300, 300, 255)


def mousedock_pack(dev, ring):
    cx, cy, r = ring
    n = (dev.get('matrix') or [1, 1])[1]
    pts = [(cx + r * math.cos(a), cy + r * math.sin(a)) for a in [math.pi / 2 + 2 * math.pi * i / 96 for i in range(97)]]
    rects = _segments(pts, n) if n > 1 else [[cx - r, cy + r - 6, 2 * r, 14]]
    return _led_spots(rects, 'Ring LED'), rects


def accessory_drawing():
    w, h = 1000, 520
    box = (60, 60, 880, 400)
    p = rr(*box, 24, BODY_FILL, BODY_LINE, 3)
    p += rr(100, 100, 220, 60, 10, 'none', '#2c2c2c', 2)
    return svg(w, h, p), box


def accessory_pack(dev, box):
    x, y, w, h = box
    rows, cols = dev.get('matrix') or [1, 1]
    spots, rects = [], []
    if rows * cols <= 1:
        rects = [[x + 40, y + h - 70, w - 80, 24]]
        return _led_spots(rects, 'Lighting'), rects
    area = (x + 40, y + 140, w - 80, h - 180)
    cw, ch = area[2] / cols, min(area[3] / rows, 60)
    for r in range(rows):
        for c in range(cols):
            rect = [round(area[0] + c * cw + 1, 1), round(area[1] + r * ch + 1, 1), round(max(cw - 2, 1), 1), round(max(ch - 4, 2), 1)]
            rects.append(rect)
            spots.append({'name': f'LED {r + 1}-{c + 1}', 'pos': None, 'led': [r, c], 'group': 'lighting', 'rect': rect})
    return spots, rects


def headset_drawing():
    w, h = 700, 700
    p = f'<path d="M150 380 C150 120 550 120 550 380" stroke="{BODY_LINE}" stroke-width="40" fill="none" stroke-linecap="round"/>\n'
    p += f'<path d="M150 380 C150 120 550 120 550 380" stroke="{BODY_FILL}" stroke-width="30" fill="none" stroke-linecap="round"/>\n'
    for cx in (150, 550):
        p += f'<ellipse cx="{cx}" cy="460" rx="95" ry="130" fill="{BODY_FILL}" stroke="{BODY_LINE}" stroke-width="4"/>\n'
        p += f'<ellipse cx="{cx}" cy="460" rx="55" ry="80" fill="none" stroke="#333" stroke-width="3"/>\n'
    return svg(w, h, p), None


def headset_pack(dev):
    n = (dev.get('matrix') or [1, 0])[1]
    anchors = [(150, 460), (550, 460), (200, 230), (500, 230)]
    leds = [{'name': ['left earcup', 'right earcup', 'left ear', 'right ear'][i], 'anchor': list(anchors[i])} for i in range(min(n, 4))]
    for i, l in enumerate(leds):
        l['led'] = [0, i]
    return leds


# ---- writing ------------------------------------------------------------------------------

def load_database():
    import tomllib
    out = []
    for f in sorted(os.listdir(os.path.join(ROOT, 'data', 'devicedb'))):
        if f.endswith('.toml'):
            with open(os.path.join(ROOT, 'data', 'devicedb', f), 'rb') as fh:
                out.extend(tomllib.load(fh).get('device', []))
    return out


def _write_shared(name, content):
    d = os.path.join(OUT, LAYOUT_DIR)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, name), 'w') as f:
        f.write(content)


def _write_pack(pid, doc, image):
    d = os.path.join(OUT, pid)
    os.makedirs(d, exist_ok=True)
    link = os.path.join(d, 'top.svg')
    if os.path.lexists(link):
        os.remove(link)
    os.symlink(os.path.join('..', LAYOUT_DIR, image), link)
    with open(os.path.join(d, 'device.json'), 'w') as f:
        json.dump(doc, f, separators=(',', ':'))
        f.write('\n')


def generate_database_packs():
    verified = {'028d', '00aa'}
    devices = [d for d in load_database() if d['pid'] not in verified]
    drawings = {}

    def shared(key, make):
        if key not in drawings:
            drawings[key] = make()
            _write_shared(f'{key}.svg', drawings[key][0])
        return drawings[key]

    counts = {}
    for dev in devices:
        layout = dev.get('layout', 'accessory')
        kind = dev['kind']
        doc = {'kind': kind, 'name': dev['name'], 'layout': layout, 'grid': dev.get('matrix'), 'art': 'tsuba',
               'approximate': not dev.get('tested', False)}
        view = {}
        if layout in ('full', 'tkl', '75', '65', '60', 'keypad', 'laptop'):
            macro = bool(dev.get('dedicated_macro_keys')) and layout in ('full', 'tkl')
            key = layout + ('-macro' if macro else '')
            svg_, placed, body, w, h = shared(key, lambda: keyboard_drawing(layout, macro))
            seq = {} if layout in ('full', 'tkl') else sequential_leds(dev, placed)
            spots = []
            for k, rect in placed:
                name = k[5]
                led = seq.get(name) or keyboard_led(dev, k, rect, w)
                spots.append({'name': name, 'pos': STD_POS.get(name), 'led': led, 'group': k[6], 'rect': rect})
            view = {'image': 'top.svg', 'width': w, 'height': h, 'body': body, 'hotspots': spots}
        elif layout in ('mouse-right', 'mouse-ambi'):
            key = layout
            dr = shared(key, lambda: mouse_drawing(layout == 'mouse-ambi'))
            view = {'image': 'top.svg', 'width': MOUSE_W, 'height': MOUSE_H, **mouse_pack(dev, dr)}
        elif layout == 'headset':
            key = layout
            shared(key, headset_drawing)
            view = {'image': 'top.svg', 'width': 700, 'height': 700, 'hotspots': [], 'leds': headset_pack(dev)}
        else:
            maker, packer, size = {
                'mousemat-strip': (mousemat_drawing, mousemat_pack, (1000, 760)),
                'headset-stand': (headset_stand_drawing, headset_stand_pack, (600, 820)),
                'mousedock': (mousedock_drawing, mousedock_pack, (600, 600)),
            }.get(layout, (accessory_drawing, accessory_pack, (1000, 520)))
            key = layout if layout in ('mousemat-strip', 'headset-stand', 'mousedock') else 'accessory'
            _svg, geo = shared(key, maker)
            spots, _rects = packer(dev, geo)
            view = {'image': 'top.svg', 'width': size[0], 'height': size[1], 'hotspots': spots}
        doc['views'] = {'top': view}
        _write_pack(dev['pid'], doc, f'{key}.svg')
        counts[key] = counts.get(key, 0) + 1
    print(f'{len(devices)} database packs, {len(drawings)} shared drawings: ' + ', '.join(f'{k} ×{v}' for k, v in sorted(counts.items())))

def write(pid, doc, files):
    d = os.path.join(OUT, pid)
    os.makedirs(d, exist_ok=True)
    for name, content in files.items():
        with open(os.path.join(d, name), 'w') as f:
            f.write(content)
    with open(os.path.join(d, 'device.json'), 'w') as f:
        json.dump(doc, f, indent=1)
        f.write('\n')
    print(f'{pid}: {len(doc["views"]["top"]["hotspots"])} hotspots → {d}')


if __name__ == '__main__':
    write('028d', *blackwidow_v4_pro())
    write('00aa', *basilisk_v3_pro())
    generate_database_packs()
