"""`candles_lit` sweeps (with `--client-capture`) to
`crates/unastar-data/data/measured/candles/candles_lit.json`.

    python tools/bds-sweep/candles_table.py SWEEP_DIR

Per click: the block and its states before, the item, the block BDS read
afterwards, and the sounds and level events the clicking player got (cut
from the capture at each teleport into the cell; positions relative to the
clicked block). Needs `bds-sweep-dump` built.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import summarize  # noqa: E402

DUMP = summarize.REPO / 'target' / 'debug' / 'bds-sweep-dump.exe'
# Where the scripted player waits between items (`sweep.ARENA['player']`).
WAIT_X, WAIT_Z = 0.5, 0.5
TELEPORT = re.compile(r'MovePlayerPacket.*?position: Vec3 \{ x: ([-\d.]+), y: [-\d.]+, z: ([-\d.]+) \}.*?Teleport')
SOUND = re.compile(r'sound_event: "([^"]*)", position: Vec3 \{ x: ([-\d.]+), y: ([-\d.]+), z: ([-\d.]+) \}, '
                   r'data: ([-\d]+)')
EVENT = re.compile(r'LevelEventPacket \{ event_id: (\d+), position: Vec3 \{ x: ([-\d.]+), y: ([-\d.]+), '
                   r'z: ([-\d.]+) \}, data: ([-\d]+)')
METHOD = (
    'sweep.py scenarios --family candles_lit --client-capture (tools/bds-sweep/candles.py): the scripted '
    'player (creative) two blocks south of a candle or candle cake set at the cell origin clicks its top face '
    'with flint and steel or a fire charge; `after` is the block read afterwards, `sounds` and `events` the '
    'LevelSoundEvent and LevelEvent packets it got ([name or id, [dx, dy, dz] from the block\'s corner, data]).'
)


def segments(capture: Path) -> list[list]:
    dump = subprocess.run([str(DUMP), str(capture), '--dir', 's2c', '--name', 'MovePlayer,LevelSoundEvent,LevelEvent'],
                          capture_output=True, text=True, check=True).stdout
    out: list[list] = []
    current = None
    for line in dump.splitlines():
        teleport = TELEPORT.search(line)
        if teleport:
            away = (float(teleport.group(1)), float(teleport.group(2))) != (WAIT_X, WAIT_Z)
            current = [] if away else None
            if away:
                out.append(current)
            continue
        if current is None:
            continue
        for kind, pattern in (('sound', SOUND), ('event', EVENT)):
            match = pattern.search(line)
            if match:
                name = match.group(1)
                current.append([kind, name if kind == 'sound' else int(name),
                                [float(v) for v in match.group(2, 3, 4)], int(match.group(5))])
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('sweep', type=Path)
    args = parser.parse_args()
    job = json.loads((args.sweep / 'job.json').read_text(encoding='utf-8'))
    base = job['arena']['scenarios']['origin']
    rows = [json.loads(line) for line in (args.sweep / 'results.jsonl').read_text(encoding='utf-8').splitlines()
            if line.strip()]
    cut = segments(args.sweep / 'client-capture' / 'packets.jsonl')
    if len(cut) != len(rows):
        raise SystemExit(f'{len(cut)} teleports for {len(rows)} clicks')
    out = []
    for row, packets in zip(rows, cut):
        block, item, count = [json.loads(p) for p in row['key'].split('|')[1:]]
        trace = row['traces']['trace']
        rel = lambda pos: [round(pos[i] - base[i], 3) for i in range(3)]  # noqa: E731
        out.append({
            'block': summarize.short(block), 'item': item, 'variant': count,
            'before': trace[0][3], 'after': [summarize.short(trace[-1][2]), trace[-1][3]],
            'sounds': [[name, rel(pos), data] for kind, name, pos, data in packets if kind == 'sound'],
            'events': [[name, rel(pos), data] for kind, name, pos, data in packets if kind == 'event'],
        })
    out.sort(key=lambda r: json.dumps([r['block'], r['item'], r['variant']]))
    header = {'family': 'candles_lit', 'method': METHOD, 'row': '{block, item, variant, before, after, sounds, events}'}
    entry = summarize.write_rows('candles/candles_lit.json', header, out)
    summarize.update_manifest('candles', summarize.merge_manifest_files('candles', {
        **summarize.provenance('scenarios', [args.sweep]), 'files': [entry], 'method': METHOD}))
    print(entry['path'], entry['rows'], entry['sha256'][:12])
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
