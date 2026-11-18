"""Distils the sleeping captures into the committed sleeping fixture.

    python tools/bds-sweep/sleeping_fixture.py OUT.json \\
        sleeping=DIR sleeping-2=DIR sleeping-3=DIR sleeping-4=DIR \\
        sleeping-5=DIR

Each DIR is a `script.py` run of `scripts/<name>.txt` (`sleeping-2`,
`sleeping-3` and `sleeping-5` with `--observer
scripts/sleeping-2-observer.txt`). The bed
clicks of each script are paired, in order, with the `use-on` lines of the
script; `SCENARIOS` below says, for the clicks the fixture keeps, what the
script had set up by then (worked out from the script: clock, weather
levels, the player's feet, monsters), and the capture gives what BDS
answered: the respawn point it set, the `tile.bed.*` messages, the
sleeping-players event and whether the player lay down. Wakes keep where
BDS stood the player up; nights keep how long the sleepers lay and the
clock and weather after. Needs `bds-sweep-dump` built
(`cargo build -p bds-sweep`). Raw captures stay outside the repository.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DUMP = REPO / 'target' / 'debug' / 'bds-sweep-dump.exe'
HEAD = [2, -60, 0]
FOOT = [1, -60, 0]
BED_A = {'head': HEAD, 'direction': 3}
BED_C = {'head': [8, -60, 1], 'direction': 0}

# The context of each kept click: script -> {use-on line tick: context}.
# `clock`: the clock when BDS handled the click (the scripts set it one or
# two ticks before; only clicks whose answer cannot change within those
# ticks are kept while the daylight cycle runs). `rain`/`lightning`: the
# weather levels then, from the script's `/weather` ticks and BDS's fade of
# 0.01 a tick. `players`/`percentage`: online players and the
# `playersSleepingPercentage` rule. `zombie`: the zombie's feet (held in a
# one-block glass shaft). `walls`: stone set beside the bed. `mode`: the
# player's game mode. `bed`: the clicked bed's head and direction.
def ctx(clock, feet, *, clicked=HEAD, cycle=True, rain=0.0, lightning=0.0, players=1, percentage=100,
        zombie=None, walls=(), above=None, mode='survival', occupied=False, bed=BED_A):
    return {
        'bed': bed, 'clock': clock, 'daylight_cycle': cycle, 'rain': rain, 'lightning': lightning,
        'players': players, 'percentage': percentage, 'feet': feet, 'clicked': clicked,
        'zombie': zombie, 'walls': list(walls), 'block_on_head': above, 'mode': mode,
        'occupied_by_other': occupied,
    }


P = [0.5, -60.0, 0.5]
SCENARIOS = {
    'sleeping': {
        40: ctx(6015, P),
        46: ctx(6021, P, clicked=FOOT),
        52: ctx(6027, [2.5, -60.0, 2.5]),
        56: ctx(6031, [-0.5, -60.0, -1.5], clicked=FOOT),
        62: ctx(12002, P),
        212: ctx(13002, P, clicked=FOOT),
        # Thunder 10 ticks old: both levels 0.1.
        420: ctx(6008, P, rain=0.1, lightning=0.1),
        # Thunder at full level, cleared 5 ticks, then rain for 10: rain
        # back at 1.0, lightning faded to 0.85.
        620: ctx(6008, P, rain=1.0, lightning=0.85),
        692: ctx(14002, P),
        752: ctx(14002, [-1.5, -60.0, 0.5], clicked=FOOT),
        772: ctx(14022, [-2.5, -60.0, 0.5], clicked=FOOT),
        842: ctx(14092, P, above='minecraft:stone'),
        880: ctx(14009, P, zombie=[9.5, -60.0, 0.5]),
        942: ctx(14000, P, cycle=False),
        1112: ctx(14001, P, mode='creative'),
    },
    'sleeping-2': {
        82: ctx(14040, [0.5, -60.0, 3.5], clicked=[2, -60, 4], players=2, occupied=True,
                bed={'head': [2, -60, 4], 'direction': 3}),
        192: ctx(14002, P, players=2),
        352: ctx(14002, P, players=2, percentage=50),
        # 12540 and on with rain cleared 19 ticks before: the rain makes it
        # night (12540 is day in clear weather, sleeping-3).
        642: ctx(12540, P, cycle=False, players=2, rain=0.81),
        742: ctx(23460, P, cycle=False, players=2),
        732: ctx(23459, P, cycle=False, players=2),
        762: ctx(18000, [-1.0, -60.0, 0.5], clicked=FOOT, cycle=False, players=2),
        792: ctx(18000, [-0.5, -60.0, 0.5], cycle=False, players=2),
        812: ctx(18000, [2.5, -60.0, 3.0], cycle=False, players=2),
        822: ctx(18000, [2.5, -60.0, 3.5], cycle=False, players=2),
        834: ctx(18000, [0.5, -58.0, 0.5], clicked=FOOT, cycle=False, players=2),
        856: ctx(14005, P, players=2, zombie=[14.5, -60.0, 0.5]),
        916: ctx(14065, P, players=2, zombie=[2.5, -54.0, 0.5]),
    },
    'sleeping-3': {
        42: ctx(12540, P, cycle=False, players=2, percentage=50),
        52: ctx(12541, P, cycle=False, players=2, percentage=50),
        62: ctx(12542, P, cycle=False, players=2, percentage=50),
        72: ctx(12543, P, cycle=False, players=2, percentage=50),
        82: ctx(18000, [1.5, -60.0, -1.5], cycle=False, players=2, percentage=50),
        92: ctx(18000, P, cycle=False, players=2, percentage=50, walls=[[2, -60, 1], [2, -59, 1]]),
        122: ctx(18000, [-0.5, -60.0, 2.5], clicked=FOOT, cycle=False, players=2, percentage=50),
        132: ctx(18000, [-1.0, -60.0, 2.0], clicked=FOOT, cycle=False, players=2, percentage=50),
        142: ctx(18000, [-1.0, -60.0, 2.5], clicked=FOOT, cycle=False, players=2, percentage=50),
        153: ctx(18000, [0.5, -57.0, 0.5], clicked=FOOT, cycle=False, players=2, percentage=50),
        182: ctx(18000, [-1.0, -60.0, 0.5], clicked=FOOT, cycle=False, players=2, percentage=50),
        245: ctx(18000, P, cycle=False, players=2, percentage=50, zombie=[13.5, -60.0, 0.5]),
        257: ctx(18000, P, cycle=False, players=2, percentage=50, zombie=[-8.5, -60.0, 0.5]),
        281: ctx(18000, P, cycle=False, players=2, percentage=50, zombie=[2.5, -60.0, 11.5]),
        305: ctx(18000, P, cycle=False, players=2, percentage=50, zombie=[2.5, -55.0, 0.5]),
        330: ctx(14008, P, players=2, percentage=50, rain=0.08),
    },
    'sleeping-4': {
        42: ctx(18000, P, cycle=False, walls=[[2, -60, 1], [2, -59, 1]]),
        82: ctx(18000, P, cycle=False, walls=[[2, -60, 1], [2, -59, 1], [1, -60, 1], [1, -59, 1]]),
        122: ctx(18000, P, cycle=False, walls=[[2, -60, 1], [2, -59, 1], [1, -60, 1], [1, -59, 1],
                                               [1, -60, -1], [1, -59, -1], [2, -60, -1], [2, -59, -1]]),
        162: ctx(18000, P, cycle=False, walls=[[2, -60, 1]]),
        202: ctx(18000, [3.5, -60.0, 0.5], cycle=False),
        242: ctx(18000, [6.5, -60.0, 0.5], clicked=[8, -60, 1], cycle=False, bed=BED_C),
        272: ctx(18000, [9.5, -60.0, 0.5], clicked=[8, -60, 1], cycle=False, bed=BED_C),
        310: ctx(18000, P, cycle=False),
    },
    'sleeping-5': {
        42: ctx(14002, P, players=2, percentage=50),
        112: ctx(14002, P, players=2, percentage=50),
        172: ctx(14002, P, players=2, percentage=50),
    },
}

LINE = re.compile(r'^(\d+) (\d+) (c2s|s2c) (\w+) (.*)$')


def dump(capture: Path) -> list[tuple[int, str, str, str]]:
    out = subprocess.run([str(DUMP), str(capture / 'client-capture' / 'packets.jsonl'), '--input-tick'],
                         check=True, capture_output=True, text=True, encoding='utf-8').stdout
    rows = []
    for line in out.splitlines():
        match = LINE.match(line)
        if match:
            rows.append((int(match[1]), match[3], match[4], match[5]))
    return rows


def script_clicks(script: Path) -> list[int]:
    ticks = []
    for line in script.read_text(encoding='utf-8').splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0].isdigit() and parts[1] == 'use-on':
            ticks.append(int(parts[0]))
    return ticks


def ints(text: str) -> list[int]:
    return [int(v) for v in re.findall(r'-?\d+', text)]


def block_pos(body: str, field: str) -> list[int]:
    match = re.search(field + r': BlockPos \{ x: (-?\d+), y: (-?\d+), z: (-?\d+) \}', body)
    return [int(match[1]), int(match[2]), int(match[3])]


def vec3(body: str, field: str) -> list[float]:
    match = re.search(field + r': Vec3 \{ x: ([^,]+), y: ([^,]+), z: ([^ ]+) \}', body)
    return [float(match[1]), float(match[2]), float(match[3])]


def vec3_value(text: str) -> list[float]:
    match = re.search(r'x: ([^,]+), y: ([^,]+), z: ([^ ]+) \}', text)
    return [float(match[1]), float(match[2]), float(match[3])]


def sleeping_event(body: str) -> list[int]:
    """(needed, sleeping) of a sleeping-players event (zigzag ints)."""
    raw = re.search(r'NbtCompoundBody\(b"(.*)"\)', body)[1]
    data = raw.encode('latin-1').decode('unicode_escape').encode('latin-1')
    values = {}
    at = 0
    while at < len(data) and data[at] == 3:
        length = data[at + 1]
        name = data[at + 2:at + 2 + length].decode()
        value = data[at + 2 + length]
        values[name] = (value >> 1) ^ -(value & 1)
        at += 3 + length
    return [values['overworldPlayerCount'], values['sleepingPlayerCount']]


def self_id(rows) -> int:
    for _, _, name, body in rows:
        if name == 'StartGamePacket':
            return int(re.search(r'runtime_id: ActorRuntimeId \{ actor_runtime_id: (\d+)', body)[1])
    raise SystemExit('no StartGame in the capture')


def actor_rows(body: str) -> list:
    """The rows of a SetActorData as [key, type, value]."""
    rows = []
    for match in re.finditer(r'DataItemEntry \{ id: (\d+), payload: DataItem(\w+)Payload\(DataItem\w+Payload '
                             r'\{ type_: \w+, value: (.*?) \}\) \}', body):
        key, kind, value = int(match[1]), match[2].lower(), match[3]
        if kind == 'pos':
            value = ints(value)
        elif kind == 'vec3':
            value = vec3_value(value + ' }')
        elif kind == 'float':
            value = float(value)
        else:
            value = int(value)
        rows.append([key, kind, value])
    return rows


def distil(name: str, capture: Path, scenarios: dict) -> dict:
    rows = dump(capture)
    me = self_id(rows)
    clicks = [i for i, (_, d, n, b) in enumerate(rows)
              if d == 'c2s' and n == 'InventoryTransactionPacket' and 'action_type: Place' in b]
    lines = script_clicks(REPO / 'tools' / 'bds-sweep' / 'scripts' / f'{name}.txt')
    if len(lines) != len(clicks):
        raise SystemExit(f'{name}: {len(lines)} use-on lines but {len(clicks)} clicks')
    out = {'clicks': [], 'wakes': [], 'nights': [], 'actor_data': []}
    mine = f'actor_runtime_id: {me} '
    for line, at in zip(lines, clicks):
        tick = rows[at][0]
        answer = {'spawn': None, 'messages': [], 'event': None, 'slept': False}
        for t, d, n, b in rows[at + 1:]:
            if d == 'c2s' and n in ('InventoryTransactionPacket', 'PlayerActionPacket', 'CommandRequestPacket'):
                break
            if t > tick + 3:
                break
            if n == 'SetSpawnPositionPacket':
                answer['spawn'] = {'block': block_pos(b, 'block_position'), 'bed': block_pos(b, 'spawn_block_pos')}
            elif n == 'TextPacket' and 'tile.bed.' in b:
                answer['messages'].append(re.search(r'tile\.bed\.(\w+)', b)[1])
            elif n == 'LevelEventGenericPacket' and 'event_id: 9801' in b and answer['event'] is None:
                answer['event'] = sleeping_event(b)
            elif n == 'SetActorDataPacket' and mine in b and 'id: 26, payload' in b:
                answer['slept'] = any(k == 26 and v == 2 for k, _, v in actor_rows(b))
                if answer['slept'] and 'id: 133' in b:
                    out['actor_data'].append({'line': line, 'event': 'lie_down', 'rows': actor_rows(b)})
        if line not in scenarios:
            continue
        out['clicks'].append({'script': name, 'line': line, **scenarios[line], 'answer': answer})
        if not answer['slept']:
            continue
        # Until the wake: when, why, and where the player stood up.
        rest = rows[at + 1:]
        for i, (t, d, n, b) in enumerate(rest):
            if n == 'AnimatePacket' and 'Wakeup' in b and mine in b:
                before = [r for r in rest[max(0, i - 40):i] if r[1] == 'c2s' and r[0] >= t - 3]
                command = next((re.search(r'command: "([^"]*)"', r[3])[1] for r in before
                                if r[2] == 'CommandRequestPacket'), None)
                cause = 'left' if any(r[2] == 'PlayerActionPacket' and 'Stopsleeping' in r[3] for r in before) \
                    else 'command' if command else 'morning'
                after = rest[i:i + 40]
                # BDS's correction standing the player up, sent with the
                # wake or up to three inputs later; the scripts' `/tp` back
                # to the player's spot (x and z 0.5, which no stand-up spot
                # of this bed has) is not one. A wake without one keeps none.
                stood = next((vec3(r[3], 'pos') for r in after
                              if r[2] == 'CorrectPlayerMovePredictionPacket' and r[0] <= t + 3
                              and vec3(r[3], 'pos')[0::2] != [0.5, 0.5]), None)
                clock = next((ints(re.search(r'time: (-?\d+)', r[3])[0])[0] for r in after
                              if r[2] == 'SyncWorldClocksPacket' and r[0] <= t + 1), None)
                event = next((sleeping_event(r[3]) for r in after
                              if r[2] == 'LevelEventGenericPacket' and '9801' in r[3] and r[0] <= t), None)
                wake_rows = next((actor_rows(r[3]) for r in after
                                  if r[2] == 'SetActorDataPacket' and mine in r[3] and 'id: 26, payload' in r[3]), None)
                if cause == 'command' and command.startswith('/tp'):
                    break
                out['wakes'].append({'script': name, 'line': line, 'cause': cause, 'command': command,
                                     'slept_ticks': t - tick, 'stood_eye': stood, 'event': event})
                if wake_rows and not any(a['event'] == 'wake' for a in out['actor_data']):
                    out['actor_data'].append({'line': line, 'event': 'wake', 'rows': wake_rows})
                if cause == 'morning':
                    out['nights'].append({'script': name, 'line': line, 'slept_ticks': t - tick,
                                          'clock_after': clock})
                break
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('out', type=Path)
    parser.add_argument('runs', nargs='+', help='SCRIPT=DIR')
    args = parser.parse_args()
    if not DUMP.exists():
        parser.error(f'{DUMP} missing: cargo build -p bds-sweep')
    fixture = {
        'source': 'BDS 1.26.51, plain, superflat, operator player: script.py captures of '
                  'tools/bds-sweep/scripts/sleeping.txt to sleeping-5.txt '
                  '(2026-11-17), distilled by tools/bds-sweep/sleeping_fixture.py',
        # Bed A, which every script sets up first.
        'bed': {'foot': FOOT, 'head': HEAD, 'direction': 3},
        'clicks': [], 'wakes': [], 'nights': [], 'actor_data': [],
    }
    for run in args.runs:
        name, _, directory = run.partition('=')
        part = distil(name, Path(directory), SCENARIOS[name])
        for key in ('clicks', 'wakes', 'nights', 'actor_data'):
            fixture[key].extend({'script': name, **row} if 'script' not in row else row for row in part[key])
    args.out.write_text(json.dumps(fixture, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(f"{len(fixture['clicks'])} clicks, {len(fixture['wakes'])} wakes, {len(fixture['nights'])} nights")
    return 0


if __name__ == '__main__':
    sys.exit(main())
