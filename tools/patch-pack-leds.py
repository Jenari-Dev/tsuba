#!/usr/bin/env python3
"""Put the media-key LEDs from tools/media-leds.json into an existing device.json picture pack.

The generator (gen-device-art.py) rewrites only Tsuba's built-in line-art pack. A personal picture
pack (~/.local/share/tsuba/devices/<pid>/device.json) keeps its own copy of the key -> LED table,
so this patches just the four media hotspots in it (a .bak copy is kept next to the file).

Usage: python3 tools/patch-pack-leds.py [path/to/device.json]
       (default: the personal BlackWidow V4 Pro pack, 028d)
"""
import json
import os
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAMES = {'Previous': 'previous', 'Play / Pause': 'play_pause', 'Next': 'next', 'Mute': 'mute'}


def main():
    default = os.path.join(os.environ.get('XDG_DATA_HOME') or os.path.expanduser('~/.local/share'), 'tsuba', 'devices', '028d', 'device.json')
    path = sys.argv[1] if len(sys.argv) > 1 else default
    with open(os.path.join(ROOT, 'tools', 'media-leds.json')) as f:
        leds = json.load(f)
    with open(path) as f:
        doc = json.load(f)
    changed = 0
    for h in doc['views']['top']['hotspots']:
        key = NAMES.get(h.get('name'))
        if key and h.get('led') != leds[key]:
            print(f"{h['name']}: {h.get('led')} -> {leds[key]}")
            h['led'] = list(leds[key])
            changed += 1
    if not changed:
        print('nothing to change:', path)
        return
    shutil.copyfile(path, path + '.bak')
    with open(path, 'w') as f:
        json.dump(doc, f, indent=1)
        f.write('\n')
    print(f'patched {changed} hotspot(s) in {path} (backup: {path}.bak)')


if __name__ == '__main__':
    main()
