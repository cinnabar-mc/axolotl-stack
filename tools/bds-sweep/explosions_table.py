"""Round-2 TNT sweeps (`tnt.ROUND2` families) to measured explosion tables.

    python tools/bds-sweep/explosions_table.py SWEEP_DIR[:FAMILY,...] [...]

A sweep directory may name the families taken from it (a later sweep that
re-measured a family replaces an earlier one).

Each family becomes `crates/unastar-data/data/measured/explosions/<family>.json`
(one structured row per cell, `method` in the header) and the `explosions`
manifest entry keeps its earlier files. Positions are cell offsets (the
superflat grass is y = -1). The scripted player's motions come from the
sweep's client capture (run with `--client-capture`), cut per item at the
teleport that moves it into its cell; `bds-sweep-dump` must be built.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import summarize  # noqa: E402
import tnt  # noqa: E402

DUMP = summarize.REPO / 'target' / 'debug' / 'bds-sweep-dump.exe'
SCRIPTED_PLAYER_RUNTIME_ID = 1
# Where the scripted player waits between items (`sweep.ARENA['player']`).
WAIT_X, WAIT_Z = 0.5, 0.5
TELEPORT = re.compile(r'MovePlayerPacket.*?runtime_id: (\d+).*?position: Vec3 \{ x: ([-\d.e]+), y: [-\d.e]+, z: ([-\d.e]+) \}')
MOTION = re.compile(r'actor_runtime_id: (\d+) \}, motion: Vec3 \{ x: ([-\d.e]+), y: ([-\d.e]+), z: ([-\d.e]+) \}')

METHOD = (
    'sweep.py scenarios --family {family} (tools/bds-sweep/tnt.py, round 2): cells 16 blocks apart on the '
    'superflat grass, a TNT at the cell origin primed by a redstone block (or lit by the scripted player), '
    'targets held still with Slowness 255 and Health Boost; per cell the explosions BDS reported (`tick` from '
    'the timeline start, `pos` the exploding actor\'s feet or the exploding block, `blocks` the cells it '
    'removes in BDS\'s order, air included), every damage event in the cell (`damage` before armour, '
    '`cause`, the `attacker` and the `damaging` actor), every knockback (`knockback`: the velocity change), '
    'and for each target its feet the tick before the explosion and its position change over the tick after '
    '(`moved`). Offsets are relative to the cell origin.'
)


def items_of(sweep: Path) -> dict:
    job = json.loads((sweep / 'job.json').read_text(encoding='utf-8'))
    return {item['key']: item for item in job['items']}


def rounded(values, places: int = 4) -> list:
    return [round(v, places) for v in values]


def last_tnt(row: dict, before: int) -> list | None:
    """The primed TNT's feet at its last read before tick `before`."""
    feet = None
    for t, actors in row.get('actor_traces', []):
        if t >= before:
            break
        for actor in actors:
            if actor[1] == 'minecraft:tnt':
                feet = actor[2:5]
    return feet


def targets(row: dict, tick: int) -> list[dict]:
    """Every non-TNT, non-item actor: its feet at the last read before the
    explosion shows (the explosion of timeline tick t happens after that
    tick's read, so the read of t + 1 is the last before the push moves
    anything) and its position change over the next read."""
    seen: dict = {}
    for t, actors in row.get('actor_traces', []):
        for actor in actors:
            if actor[1] in ('minecraft:tnt', 'minecraft:item'):
                continue
            entry = seen.setdefault(actor[0], {'type': summarize.short(actor[1]), 'before': None, 'after': None})
            if t <= tick + 1:
                entry['before'] = actor[2:5]
            elif entry['after'] is None:
                entry['after'] = actor[2:5]
    out = []
    for entry in seen.values():
        if entry['before'] is None:
            continue
        moved = None if entry['after'] is None else rounded(
            [a - b for a, b in zip(entry['after'], entry['before'])], 3)
        out.append({'type': entry['type'], 'feet': entry['before'], 'moved': moved})
    return sorted(out, key=lambda e: e['type'])


def damage_events(row: dict) -> list[dict]:
    return [{'tick': e['tick'], 'type': summarize.short(e['type']), 'damage': round(e['damage'], 4),
             'cause': e['cause'], 'attacker': summarize.short((e['actor'] or {}).get('type') or '') or None,
             'damaging': summarize.short((e['damaging'] or {}).get('type') or '') or None}
            for e in row.get('damage_events', [])]


def knockbacks(row: dict) -> list[dict]:
    return [{'tick': k['tick'], 'type': summarize.short(k['type']), 'knockback': rounded(k['knockback']),
             'from': k['source']} for k in row.get('knockbacks', [])]


def explosions(row: dict, with_blocks: bool) -> list[dict]:
    out = []
    for e in row.get('explosions', []):
        entry = {'tick': e['tick'], 'pos': rounded(e['pos']),
                 'source': summarize.short((e['source'] or {}).get('type') or (e['source'] or {}).get('block')
                                           or '')}
        if with_blocks:
            entry['blocks'] = e['blocks']
        else:
            entry['broken'] = len(e['blocks'])
        out.append(entry)
    return out


def player_motions(sweep: Path, rows: list[dict]) -> dict[str, list]:
    """The scripted player's `SetActorMotion`s per solo item, in the order
    the items ran: the capture is cut at each teleport away from where the
    player waits between items (`sweep.ARENA['player']`)."""
    capture = sweep / 'client-capture' / 'packets.jsonl'
    if not capture.exists():
        return {}
    dump = subprocess.run([str(DUMP), str(capture), '--dir', 's2c', '--name', 'SetActorMotion,MovePlayer'],
                          capture_output=True, text=True, check=True).stdout
    segments: list[list] = []
    for line in dump.splitlines():
        teleport = TELEPORT.search(line)
        if teleport:
            if int(teleport.group(1)) == SCRIPTED_PLAYER_RUNTIME_ID and (
                    float(teleport.group(2)), float(teleport.group(3))) != (WAIT_X, WAIT_Z):
                segments.append([])
        elif segments:
            match = MOTION.search(line)
            if match and int(match.group(1)) == SCRIPTED_PLAYER_RUNTIME_ID:
                segments[-1].append(rounded(float(v) for v in match.group(2, 3, 4)))
    solo = [row['key'] for row in rows if row.get('solo')]
    if len(segments) != len(solo):
        raise SystemExit(f'{sweep}: {len(segments)} teleports into cells for {len(solo)} solo items')
    return dict(zip(solo, segments))


def cell_reads(item: dict, row: dict, tick: int) -> dict:
    """The timeline's watched cells as they stood at the end of tick
    `tick` (the trace keeps the first read and every change)."""
    timeline = next(step for step in item['steps'] if step[0] == 'timeline')
    watch = timeline[4]
    state: dict = {}
    for t, index, block, states in row['traces'][timeline[2]]:
        if t <= tick:
            state[tuple(watch[index])] = summarize.short(block)
    return state


def row_of(family: str, item: dict, row: dict, motions: dict) -> dict:
    name = [json.loads(part) for part in row['key'].split('|')[1:]]
    out: dict = {'name': name}
    blocks = family in ('tnt_logged', 'tnt_drops', 'tnt_decay', 'tnt_anchor')
    out['explosions'] = explosions(row, blocks)
    if family == 'tnt_chain':
        ticks = [e['tick'] for e in out['explosions']]
        out['fuse'] = ticks[1] - ticks[0] if len(ticks) == 2 else None
        return out
    first = out['explosions'][0]['tick'] if out['explosions'] else None
    if first is not None:
        out['tnt'] = last_tnt(row, first + 1)
        out['targets'] = targets(row, first)
    out['damage_events'] = damage_events(row)
    out['knockbacks'] = knockbacks(row)
    if row['key'] in motions:
        out['player_motions'] = motions[row['key']]
    if family in ('tnt_logged', 'tnt_drops', 'tnt_decay'):
        watch = next(step for step in item['steps'] if step[0] == 'read')[2]
        out['destroyed'] = [o for o, (b, _) in zip(watch, row['reads']['after']) if b == 'minecraft:air']
        out['items'] = [[summarize.short(i), n] for i, _, n in row.get('items', [])]
        if first is not None:
            out['first_stacks'] = sorted(
                [[a[9], rounded(a[2:5], 3)] for t, actors in row.get('actor_traces', []) if t == first + 1
                 for a in actors if a[1] == 'minecraft:item'], reverse=True)
    if family == 'tnt_anchor':
        if name[0] == 'charge':
            # The anchor's states at each change (air once it exploded).
            out['anchor'] = [[t, st] for t, _, b, st in row['traces']['trace']]
        if first is not None and name[0] != 'charge':
            before = cell_reads(item, row, first)
            after = cell_reads(item, row, first + 1)
            out['solid_before'] = sorted([list(p), b] for p, b in before.items() if b != 'air')
            out['fire_after'] = sorted(list(p) for p, b in after.items() if b == 'fire')
        out['items'] = [[summarize.short(i), n] for i, _, n in row.get('items', [])]
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('sweeps', nargs='+')
    args = parser.parse_args()
    by_family: dict[str, list] = defaultdict(list)
    sweeps = []
    for spec in args.sweeps:
        # The families follow the last colon (a Windows path has its own).
        path, _, names = spec.rpartition(':')
        if not path or '/' in names or '\\' in names:
            path, names = spec, ''
        wanted = set(names.split(',')) if names else set(tnt.ROUND2)
        sweeps.append((Path(path), wanted))
    for sweep, wanted in sweeps:
        items = items_of(sweep)
        rows = [json.loads(line) for line in (sweep / 'results.jsonl').read_text(encoding='utf-8').splitlines()
                if line.strip()]
        rows = [{**row, 'solo': items[row['key']].get('solo', False)} for row in rows if row['key'] in items]
        motions = player_motions(sweep, rows)
        for row in rows:
            family = row['key'].split('|', 1)[0]
            if family not in tnt.ROUND2 or family not in wanted:
                continue
            by_family[family].append(row_of(family, items[row['key']], row, motions))
    files = []
    for family, rows in sorted(by_family.items()):
        rows.sort(key=lambda r: json.dumps(r['name'], sort_keys=True))
        header = {'family': family, 'method': METHOD.format(family=family),
                  'row': '{name, explosions, tnt, targets, damage_events, knockbacks, ...}'}
        files.append(summarize.write_rows(f'explosions/{family}.json', header, rows))
    entry = {**summarize.provenance('scenarios', [sweep for sweep, _ in sweeps]), 'files': files,
             'method': 'round-2 TNT families: ' + METHOD.format(family='<family>')}
    summarize.update_manifest('explosions', summarize.merge_manifest_files('explosions', entry))
    for f in files:
        print(f['path'], f['rows'], f['sha256'][:12])
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
