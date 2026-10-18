#!/usr/bin/env python3
"""Regenerate Tsuba's device database (data/devicedb/*.toml) from a local OpenRazer checkout.

The OpenRazer project (GPL-2.0) documents which USB product ids belong to which Razer devices
and what each one supports. This script reads those *facts* only — it parses the daemon's
hardware class definitions with Python's `ast` module and scans the kernel driver sources as
plain text. Nothing from the checkout is imported or executed, and no code or comments are
copied: the output is Tsuba's own TOML format.

Usage:
    python3 -I tools/import-openrazer.py <path to openrazer checkout>     (from the repo root)

Devices Tsuba has been verified on (`VERIFIED` below) keep Tsuba's own measured values.
Headsets (not protocol 2.5) are listed with `supported = false` and research notes.
"""
import ast
import os
import re
import sys
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'data', 'devicedb')

# ------------------------------------------------------------------------------------------
# Tsuba's own, hardware-verified values. These win over anything imported.
VERIFIED = {
    0x028d: dict(name='BlackWidow V4 Pro', kind='keyboard', iface=3, event_iface=1, txid=0x1f,
                 matrix=[9, 24], layout='full', leds=['backlight', 'underglow'], dial=True, macro_keys=5, tested=True),
    0x00aa: dict(name='Basilisk V3 Pro (wired)', kind='mouse', iface=0, event_iface=1, txid=0x1f,
                 matrix=[1, 13], layout='mouse-right',
                 leds=['logo', 'scroll_wheel', 'underglow'], tested=True),
}

# Family defaults (what a device gets when the database leaves a field out). The control
# interface is the USB interface whose feature reports carry protocol commands; the event
# interface carries the vendor input reports (Razer keys, media, power).
FAMILIES = {
    'keyboards': dict(iface=1, event_iface=1, txid=0xff, kinds=('keyboard', 'laptop_keyboard')),
    'keypads': dict(iface=1, event_iface=1, txid=0xff, kinds=('keypad',)),
    'mice': dict(iface=0, event_iface=1, txid=0xff, kinds=('mouse', 'receiver')),
    'mousemats': dict(iface=0, event_iface=0, txid=0x1f, kinds=('mousemat',)),
    'accessories': dict(iface=0, event_iface=0, txid=0x1f,
                        kinds=('mousedock', 'headset_stand', 'speaker', 'accessory', 'egpu', 'monitor')),
    'headsets': dict(iface=3, event_iface=3, txid=0xff, kinds=('headset',)),
}

# Headsets are not protocol-2.5 devices (OpenRazer drives most of them with a separate
# "kraken" driver that pokes lighting registers). They are listed for completeness with
# `supported = false`; Tsuba has no headset code. HeadsetControl (GPL-3) lists no Razer models.
KRAKEN_NOTE = ('OpenRazer drives this headset with its separate Kraken driver (lighting registers '
               'only: {fx}). Not protocol 2.5; battery, sidetone, mic and EQ are not documented by '
               'OpenRazer or HeadsetControl.')
ACCESSORY_HEADSET_NOTE = ('Lighting only, through protocol-2.5 style matrix reports on interface 0 '
                          '(OpenRazer accessory driver). No battery/sidetone/mic/EQ documented.')

HW = 'daemon/openrazer_daemon/hardware'
FILES = ['keyboards.py', 'mouse.py', 'mouse_mat.py', 'accessory.py', 'core.py', 'monitor.py', 'headsets.py']


# ------------------------------------------------------------------------------------------
# Daemon class definitions (parsed, never imported)

def literal(node):
    try:
        return ast.literal_eval(node)
    except Exception:
        return None


def parse_classes(path):
    tree = ast.parse(open(path, encoding='utf-8').read(), path)
    out = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        c = {'bases': [b.id if isinstance(b, ast.Name) else getattr(b, 'attr', '') for b in node.bases],
             'doc': ast.get_docstring(node) or '', 'attrs': {}, 'file': os.path.basename(path)}
        for st in node.body:
            if isinstance(st, ast.Assign) and len(st.targets) == 1 and isinstance(st.targets[0], ast.Name):
                name = st.targets[0].id
                v = st.value
                if name == 'METHODS':
                    c['attrs'][name] = methods_expr(v)
                elif name == 'EVENT_FILE_REGEX':
                    if isinstance(v, ast.Call) and v.args:
                        c['attrs'][name] = literal(v.args[0])
                else:
                    lv = literal(v)
                    if lv is not None:
                        c['attrs'][name] = lv
        out[node.name] = c
    return out


def methods_expr(v):
    """METHODS is a list literal or `Parent.METHODS + [...]`. Returns ('list', [...]) or
    ('extend', parent, [...])."""
    if isinstance(v, ast.List):
        return ('list', [literal(e) for e in v.elts if isinstance(literal(e), str)])
    if isinstance(v, ast.BinOp) and isinstance(v.op, ast.Add):
        left, right = v.left, v.right
        if isinstance(left, ast.Attribute) and isinstance(left.value, ast.Name):
            return ('extend', left.value.id, methods_expr(right)[1] if methods_expr(right) else [])
        if isinstance(left, ast.BinOp):
            inner = methods_expr(left)
            extra = methods_expr(right)
            if inner and extra:
                if inner[0] == 'extend':
                    return ('extend', inner[1], inner[2] + extra[1])
                return ('list', inner[1] + extra[1])
    return ('list', [])


def resolve(classes, name, attr, seen=()):
    c = classes.get(name)
    if c is None or name in seen:
        return None
    if attr in c['attrs']:
        v = c['attrs'][attr]
        if attr == 'METHODS':
            if v[0] == 'extend':
                return (resolve(classes, v[1], 'METHODS', seen + (name,)) or []) + v[2]
            return v[1]
        return v
    for b in c['bases']:
        r = resolve(classes, b, attr, seen + (name,))
        if r is not None:
            return r
    return None


# ------------------------------------------------------------------------------------------
# Kernel driver facts (text scan)

def driver_pids(drv):
    """USB_DEVICE_ID_RAZER_* macro name → pid, and pid → driver file stem."""
    names, owner = {}, {}
    for f in os.listdir(drv):
        if not f.endswith('.h'):
            continue
        for m in re.finditer(r'#define\s+(USB_DEVICE_ID_RAZER_\w+)\s+(0x[0-9A-Fa-f]+)', open(os.path.join(drv, f)).read()):
            pid = int(m.group(2), 16)
            names[m.group(1)] = pid
            owner.setdefault(pid, f[:-2])
    return names, owner


def matching_brace(s, i):
    depth = 0
    for j in range(i, len(s)):
        if s[j] == '{':
            depth += 1
        elif s[j] == '}':
            depth -= 1
            if depth == 0:
                return j
    return len(s) - 1


def switch_groups(src):
    """Yield (labels, body, is_default) for every case group of every switch on the USB pid."""
    for m in re.finditer(r'switch\s*\(\s*[\w>.\-]*(usb_pid|idProduct)\s*\)\s*\{', src):
        start = m.end() - 1
        body = src[start + 1:matching_brace(src, start)]
        parts = re.split(r'(\bcase\s+\w+\s*:|\bdefault\s*:)', body)
        labels, text = [], ''
        for p in parts:
            lab = re.match(r'case\s+(\w+)\s*:|default\s*:', p)
            if lab:
                if text.strip():
                    yield labels, text
                    labels, text = [], ''
                labels.append(lab.group(1) or 'default')
            else:
                text += p
        if labels:
            yield labels, text


def driver_facts(drv, macro_pid):
    txids = defaultdict(Counter)
    proto = defaultdict(Counter)
    report_index = {}
    for f in sorted(os.listdir(drv)):
        if not f.endswith('.c'):
            continue
        src = re.sub(r'/\*.*?\*/', '', open(os.path.join(drv, f)).read(), flags=re.S)
        src = re.sub(r'//[^\n]*', '', src)
        for labels, body in switch_groups(src):
            pids = [macro_pid[l] for l in labels if l in macro_pid]
            if not pids:
                continue
            for t in re.findall(r'transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)', body):
                for p in pids:
                    txids[p][int(t, 16)] += 1
            for kind in re.findall(r'razer_chroma_(standard|extended_matrix|mouse_extended_matrix)_\w+\(', body):
                for p in pids:
                    proto[p]['extended' if 'extended' in kind else 'standard'] += 1
            ri = re.search(r'report_index\s*=\s*(0x[0-9A-Fa-f]+)', body)
            if ri:
                for p in pids:
                    report_index.setdefault(p, int(ri.group(1), 16))
            # Mouse driver: a few receivers/wired mice use another interface index.
            mi = re.search(r'\bindex\s*=\s*(0x[0-9A-Fa-f]+)\s*;', body)
            if mi and f.startswith('razermouse'):
                for p in pids:
                    report_index.setdefault(p, int(mi.group(1), 16))
    return txids, proto, report_index


# ------------------------------------------------------------------------------------------
# Derivations

EFFECTS = [  # (method fragment, effect)
    ('_none', 'off'), ('static', 'static'), ('breath', 'breathing'), ('spectrum', 'spectrum'),
    ('wave', 'wave'), ('reactive', 'reactive'), ('starlight', 'starlight'), ('ripple', 'ripple'),
    ('wheel', 'wheel'), ('custom', 'custom_frame'), ('blinking', 'blinking'), ('pulsate', 'pulsate'),
]
ZONES = [('logo', 'logo'), ('scroll', 'scroll_wheel'), ('left', 'left_side'), ('right', 'right_side'),
         ('backlight', 'backlight'), ('charging', 'charging'), ('fast_charging', 'fast_charging'),
         ('fully_charged', 'fully_charged'), ('channel', 'channels')]


def effects_of(methods):
    out = []
    for frag, eff in EFFECTS:
        if any(m.startswith('set_') and frag in m for m in methods if 'brightness' not in m and 'poll' not in m):
            out.append(eff)
    if 'set_key_row' in methods and 'custom_frame' not in out:
        out.append('custom_frame')
    return out


def zones_of(methods):
    out = []
    for frag, zone in ZONES:
        pat = re.compile(rf'^(set|get)_{frag}_')
        if any(pat.match(m) for m in methods):
            if zone == 'charging' and not any(re.match(r'^set_charging_', m) for m in methods):
                continue
            out.append(zone)
    return out


AMBI = ('lancehead', 'viper', 'orochi', 'atheris', 'abyssus', 'diamondback', 'taipan', 'krait',
        'ouroboros', 'pro click mini', 'cobra', 'salmosa', 'boomslang', 'mamba wireless')


def mouse_layout(name):
    n = name.lower()
    return 'mouse-ambi' if any(a in n for a in AMBI) else 'mouse-right'


def keyboard_layout(name, kind, matrix):
    n = name.lower()
    if kind == 'keypad':
        return 'keypad'
    if kind == 'laptop_keyboard':
        return 'laptop'
    if '75%' in n or '75 %' in n:
        return '75'
    if 'mini' in n:
        return '65' if 'blackwidow' in n else '60'
    if 'tenkeyless' in n or 'tkl' in n or 'tournament' in n or ' te' in n or 'tartarus' in n:
        return 'tkl'
    if 'joro' in n:
        return '75'
    if 'tk' in n.split() or ' tk ' in f' {n} ':
        return 'tkl'
    if not matrix or matrix[0] < 5:
        return 'full'
    cols = matrix[1]
    return 'full' if cols >= 21 else 'tkl' if cols >= 17 else '65' if cols >= 15 else '60'


def accessory_kind(name, file):
    n = name.lower()
    if file == 'core.py':
        return 'egpu', 'accessory'
    if file == 'monitor.py':
        return 'monitor', 'accessory'
    if 'mouse dock' in n or 'bungee' in n:
        return 'mousedock', 'mousedock'
    if 'base station' in n:
        return 'headset_stand', 'headset-stand'
    if 'nommo' in n:
        return 'speaker', 'accessory'
    return 'accessory', 'accessory'


def clean_name(doc, cls):
    m = re.search(r'Class for (?:the )?(.*)', doc)
    if m:
        name = m.group(1)
    else:  # no docstring: split the class name's words
        name = re.sub(r'(?<=[a-z0-9])(?=[A-Z])', ' ', cls)
    name = name.strip().rstrip('.')
    name = re.sub(r'^Razer\s+', '', name)
    return name


def event_iface(regex):
    if not regex:
        return None
    m = re.search(r'if0\(?(\d)', regex)
    if m:
        return int(m.group(1))
    m = re.search(r'event-if0?(\d)', regex)
    return int(m.group(1)) if m else None


def family_of(kind):
    for fam, d in FAMILIES.items():
        if kind in d['kinds']:
            return fam
    return 'accessories'


def base_key(name):
    n = re.sub(r'\((wired|wireless|receiver|bluetooth)[^)]*\)', '', name, flags=re.I)
    n = re.sub(r'\b(wired|wireless|receiver)\b', '', n, flags=re.I)
    return re.sub(r'\s+', ' ', n).strip().lower()


def connection(name):
    n = name.lower()
    if 'receiver' in n or 'dongle' in n:
        return 'receiver'
    if 'bluetooth' in n:
        return 'bluetooth'
    if 'wireless' in n:
        return 'wireless'
    if 'wired' in n:
        return 'wired'
    return None


def build(openrazer):
    hw = os.path.join(openrazer, HW)
    drv = os.path.join(openrazer, 'driver')
    macro_pid, owner = driver_pids(drv)
    txids, proto, report_index = driver_facts(drv, macro_pid)

    devices = {}
    for f in FILES:
        classes = parse_classes(os.path.join(hw, f))
        for cls, c in classes.items():
            pid = resolve(classes, cls, 'USB_PID')
            if pid is None or cls.startswith('_') or 'USB_PID' not in c['attrs']:
                continue
            methods = resolve(classes, cls, 'METHODS') or []
            name = clean_name(c['doc'], cls)
            matrix = resolve(classes, cls, 'MATRIX_DIMS')
            has_matrix = resolve(classes, cls, 'HAS_MATRIX')
            d = {'pid': pid, 'name': name, 'methods': methods, 'file': f}
            if f == 'keyboards.py':
                if 'get_device_type_keypad' in methods:
                    d['kind'] = 'keypad'
                elif re.search(r'\bBlade\b|\bBook\b', name):
                    d['kind'] = 'laptop_keyboard'
                else:
                    d['kind'] = 'keyboard'
            elif f == 'mouse.py':
                d['kind'] = 'receiver' if 'dongle' in name.lower() else 'mouse'
            elif f == 'mouse_mat.py':
                d['kind'] = 'mousemat'
            elif f == 'headsets.py':
                d['kind'] = 'headset'
            else:
                d['kind'], d['layout'] = accessory_kind(name, f)
            if has_matrix and matrix and list(matrix) != [1, 1]:
                d['matrix'] = list(matrix)
            elif has_matrix and matrix:
                d['matrix'] = [1, 1]
            d['effects'] = effects_of(methods)
            zones = zones_of(methods)
            if d.get('matrix') and d['matrix'][0] * d['matrix'][1] > 1 and d['kind'] in ('keyboard', 'laptop_keyboard', 'keypad'):
                zones = ['backlight'] + [z for z in zones if z != 'backlight']
            d['leds'] = zones
            dpi = resolve(classes, cls, 'DPI_MAX')
            if dpi:
                d['max_dpi'] = dpi
            if 'set_poll_rate' in methods:
                d['polling_rates'] = list(resolve(classes, cls, 'POLL_RATES') or [125, 500, 1000])
            flags = {
                'dpi_stages': 'set_dpi_stages' in methods,
                'battery': 'get_battery' in methods,
                'scroll_modes': 'set_scroll_mode' in methods,
                'scroll_acceleration': 'set_scroll_acceleration' in methods,
                'smart_reel': 'set_scroll_smart_reel' in methods,
                'onboard_macros': 'add_macro' in methods,
                'game_mode': 'set_game_mode' in methods,
                'dedicated_macro_keys': bool(resolve(classes, cls, 'DEDICATED_MACRO_KEYS')),
                'keyswitch_optical': 'set_keyswitch_optimization' in methods,
                'idle_time': 'set_idle_time' in methods,
                'charging_lights': 'set_charge_effect' in methods or 'charging' in zones,
            }
            d.update({k: True for k, v in flags.items() if v})
            conn = connection(name)
            if conn in ('wireless', 'receiver', 'bluetooth') or (flags['battery'] and conn != 'wired'):
                d['wireless'] = True
            if conn:
                d['connection'] = conn
            ev = event_iface(resolve(classes, cls, 'EVENT_FILE_REGEX'))
            if ev is not None:
                d['event_iface'] = ev
            if pid in report_index:
                d['iface'] = report_index[pid]
            if txids.get(pid):
                common = txids[pid].most_common()
                # Prefer the id used for most commands; 0xff only wins when nothing else is used.
                best = next((t for t, _ in common if t != 0xff), common[0][0])
                d['txid'] = best
            if proto.get(pid):
                p = proto[pid]
                d['protocol'] = 'extended' if p['extended'] >= p['standard'] else 'standard'
            if d['kind'] == 'headset':
                d['supported'] = False
            if pid in devices:
                continue  # first class wins (alternates subclass the main one)
            devices[pid] = d

    # PIDs only the kernel driver knows (no daemon class): name from the macro.
    for macro, pid in macro_pid.items():
        if pid in devices:
            continue
        stem = owner.get(pid, '')
        name = macro.replace('USB_DEVICE_ID_RAZER_', '').replace('_', ' ').title()
        name = re.sub(r'\bV(\d)', r'V\1', name)
        kind = {'razerkbd_driver': 'keyboard', 'razermouse_driver': 'mouse', 'razeraccessory_driver': 'accessory',
                'razerkraken_driver': 'headset'}.get(stem, 'accessory')
        d = {'pid': pid, 'name': name, 'kind': kind, 'methods': [], 'effects': [], 'leds': [],
             'notes': 'Known to the OpenRazer kernel driver only; capabilities unknown.'}
        if kind == 'headset':
            d['supported'] = False
        if pid in txids:
            d['txid'] = next((t for t, _ in txids[pid].most_common() if t != 0xff), txids[pid].most_common()[0][0])
        if pid in report_index:
            d['iface'] = report_index[pid]
        devices[pid] = d

    # Headset notes.
    for d in devices.values():
        if d['kind'] != 'headset':
            continue
        d['supported'] = False
        if owner.get(d['pid']) == 'razeraccessory_driver':
            d['iface'], d['event_iface'] = 0, 0
            d['notes'] = ACCESSORY_HEADSET_NOTE
        else:
            d['notes'] = KRAKEN_NOTE.format(fx=', '.join(d['effects']) or 'none')

    # Layouts.
    for d in devices.values():
        if 'layout' in d:
            continue
        k = d['kind']
        if k in ('keyboard', 'laptop_keyboard', 'keypad'):
            d['layout'] = keyboard_layout(d['name'], k, d.get('matrix'))
        elif k in ('mouse',):
            d['layout'] = mouse_layout(d['name'])
        elif k == 'receiver':
            d['layout'] = 'accessory'
        elif k == 'mousemat':
            d['layout'] = 'mousemat-strip'
        elif k == 'headset':
            d['layout'] = 'headset'
        else:
            d['layout'] = 'accessory'

    # Wired ↔ wireless/receiver pairs (same model name apart from the connection).
    groups = defaultdict(list)
    for d in devices.values():
        if d.get('connection'):
            groups[(d['kind'] if d['kind'] != 'receiver' else 'mouse', base_key(d['name']))].append(d)
    for members in groups.values():
        wired = [m for m in members if m['connection'] == 'wired']
        other = [m for m in members if m['connection'] != 'wired']
        if len(wired) == 1 and other:
            for o in other:
                o['pair'] = wired[0]['pid']
            wired[0]['pair'] = other[0]['pid']

    # Verified values last.
    for pid, v in VERIFIED.items():
        devices.setdefault(pid, {'pid': pid, 'methods': [], 'effects': [], 'leds': []}).update(v)
    for d in devices.values():
        d.setdefault('tested', False)
    return devices


# ------------------------------------------------------------------------------------------
# TOML output

ORDER = ['pid', 'name', 'kind', 'layout', 'tested', 'supported', 'txid', 'iface', 'event_iface', 'protocol',
         'connection', 'pair', 'matrix', 'leds', 'effects', 'max_dpi', 'polling_rates', 'dpi_stages',
         'battery', 'wireless', 'idle_time', 'charging_lights', 'scroll_modes', 'scroll_acceleration',
         'smart_reel', 'dial', 'macro_keys', 'dedicated_macro_keys', 'onboard_macros', 'game_mode',
         'keyswitch_optical', 'notes']


def tval(k, v):
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if k in ('txid',):
        return f'0x{v:02x}'
    if k in ('pid', 'pair'):
        return f'"{v:04x}"'
    if isinstance(v, int):
        return str(v)
    if isinstance(v, list):
        return '[' + ', '.join(tval('', x) for x in v) + ']'
    s = str(v).replace('\\', '\\\\').replace('"', '\\"')
    return f'"{s}"'


def write(devices, source_rev):
    os.makedirs(OUT, exist_ok=True)
    by_family = defaultdict(list)
    for d in devices.values():
        by_family[family_of(d['kind'])].append(d)
    total = 0
    for fam, defaults in FAMILIES.items():
        rows = sorted(by_family.get(fam, []), key=lambda d: d['pid'])
        lines = [
            f'# Tsuba device database — {fam}.',
            '# Generated by tools/import-openrazer.py from facts cross-checked against the OpenRazer',
            f'# project ({source_rev}); Tsuba-verified devices carry Tsuba\'s own measurements.',
            '# Fields left out take the family defaults below. `tested = true` only for devices',
            '# Tsuba has been verified on; everything else is read-only until the user opts in.',
            '',
            '[defaults]',
            f'iface = {defaults["iface"]}',
            f'event_iface = {defaults["event_iface"]}',
            f'txid = 0x{defaults["txid"]:02x}',
            '',
        ]
        for d in rows:
            lines.append('[[device]]')
            for k in ORDER:
                if k not in d:
                    continue
                v = d[k]
                if k in ('iface', 'event_iface', 'txid') and v == defaults[k]:
                    continue
                if v is False and k not in ('tested', 'supported'):
                    continue
                if v == [] or v is None:
                    continue
                lines.append(f'{k} = {tval(k, v)}')
            lines.append('')
        with open(os.path.join(OUT, f'{fam}.toml'), 'w') as f:
            f.write('\n'.join(lines))
        total += len(rows)
        print(f'{fam}: {len(rows)} devices')
    print(f'total: {total}')


def git_rev(path):
    head = os.path.join(path, '.git', 'HEAD')
    try:
        ref = open(head).read().strip()
        if ref.startswith('ref:'):
            p = os.path.join(path, '.git', ref.split()[1])
            if os.path.exists(p):
                return 'openrazer ' + open(p).read().strip()[:12]
            packed = open(os.path.join(path, '.git', 'packed-refs')).read()
            for line in packed.splitlines():
                if line.endswith(ref.split()[1]):
                    return 'openrazer ' + line[:12]
        return 'openrazer ' + ref[:12]
    except OSError:
        return 'openrazer checkout'


if __name__ == '__main__':
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    src = os.path.abspath(sys.argv[1])
    write(build(src), git_rev(src))
