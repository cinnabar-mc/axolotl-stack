"""Distil `sweep.py --client-capture` mob runs into a committed mob fixture.

Two kinds of run, two fixture shapes:

    python tools/bds-sweep/mob_fixture.py tracked [--only KIND,...] [--events-only KIND,...] OUT.json SWEEP_DIR [SWEEP_DIR ...]
    python tools/bds-sweep/mob_fixture.py events OUT.json [--skip PREFIX,...] LABEL=SWEEP_DIR [LABEL=SWEEP_DIR ...]

Both decode each run's `client/packets.jsonl` with `bds-extractor --decode`
(cargo build -p bds-extractor first; `BDS_EXTRACTOR` overrides the binary)
and put every packet on the tick of the `bds-sweep:mark` before it. Only
observed values are kept: no packet bytes, no login data.

`tracked`: the generic per-tick job with `track` (`bat`, `ghast`,
`polar-bear`, `tadpole`, `vex`, `goat`), e.g.
`crates/unastar/src/entity/mob/ai/testdata/bds-1.26.51-polar_bear.json`. An
item of a later sweep replaces one of the same name; `--only` keeps the
items whose name (without the trailing `-<n>`) is listed; `--events-only`
keeps the items of those kinds without their actors' per-tick rows (a long
run that only its damage, flags, events and sounds are wanted from). Per
item it keeps the item's spec (where the player stood, the setup commands
and the actions) and:

- the player's feet every tick it moved (`t`, x, y, z);
- per tracked actor, in order of first appearance: its type, the first tick
  it was sampled and every tick (`t`, feet, body yaw, pitch, velocity,
  ground contact, health);
- every damage event (tick, actor type, amount, cause);
- per tracked actor, its actor flags at `AddActor` (the high word signed,
  as sent) and every flag that changed afterwards, every `ActorEvent` and
  every `LevelSoundEvent` it made.

`events`: the one-mob jobs (`spider`, `enderman`), e.g.
`crates/unastar/src/entity/mob/ai/testdata/bds-1.26.51-enderman.json`.
`LABEL` names the run (its difficulty, say: `normal`); items are keyed
`<label>/<item>` and a later run's item replaces one of the same key.
`--skip` leaves out the items whose name starts with one of the prefixes.
Per item it keeps the spec (where the mob and the player stood, whether the
mob was frozen, the setup commands and the timed actions), the mob's feet
and ground contact on every tick it moved, the player's feet whenever they
changed, and the observed events, each with its tick: the mob's `TARGET`
(whether it is the player), every change of its flag bits and of its other
actor data, its sounds and actor events, its `RemoveActor`, the player's
`MobEffect`s, the block updates, the items that appeared, the level events
and the damage the job recorded; also the mob's `MobEquipment` (the
enderman's carried block), its forced moves (a `MoveActorDelta` with
`force_move`: a teleport), the generic level events (the teleport trail; not
the sleep counter, 9801) and the other actors added and removed (arrows).
The player's timed `tp` actions are also kept as structured turns (feet,
yaw, pitch).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
EXTRACTOR = Path(os.environ.get('BDS_EXTRACTOR') or REPO / 'target' / 'debug' / (
    'bds-extractor.exe' if sys.platform == 'win32' else 'bds-extractor'))
PACKETS = ['TextPacket', 'SetActorDataPacket', 'AddActorPacket', 'LevelSoundEventPacket', 'LevelEventPacket',
           'RemoveActorPacket', 'MobEffectPacket', 'ActorEventPacket', 'UpdateBlockPacket', 'AddItemActorPacket',
           'MobEquipmentPacket', 'LevelEventGenericPacket', 'MoveActorDeltaPacket']
# The two actor-flag words of the actor data.
FLAG_WORDS = (0, 92)
TARGET = 6
# Actor data that changes on its own every tick or two and says nothing in
# an `events` fixture: the flag words (kept as bit changes), the air supply
# and the target.
SKIPPED_DATA = {0, 7, TARGET, 92}


def decoded(capture: Path) -> list[dict]:
    out = capture.with_name('decoded-mob.jsonl')
    subprocess.run([str(EXTRACTOR), '--decode', str(capture), '--only', ','.join(PACKETS), '--output', str(out)],
                   check=True, capture_output=True)
    return [json.loads(line) for line in out.read_text(encoding='utf-8').splitlines() if line.strip()]


def marked(records: list[dict]) -> dict[str, list[tuple[int, dict]]]:
    """Every record after a chat mark, grouped by the mark's item."""
    out: dict[str, list[tuple[int, dict]]] = {}
    key, t = None, None
    for record in records:
        if record['name'] == 'TextPacket':
            message = json.dumps(record['packet'])
            if 'bds-sweep:mark ' in message:
                rest = message[message.index('bds-sweep:mark ') + len('bds-sweep:mark '):].split('"')[0]
                key, tick = rest.rsplit(' ', 1)
                t = int(tick)
            continue
        if key is not None:
            out.setdefault(key, []).append((t, record))
    return out


def data_items(entries: list[dict]) -> dict[int, object]:
    out = {}
    for entry in entries:
        (_, payload), = entry['payload'].items()
        out[entry['id']] = payload.get('value', payload)
    return out


def flags_of(items: dict[int, object], previous: int | None) -> int | None:
    """The 128 actor flags after `items`, a word missing from them taken
    from `previous`; the high word keeps its sign, as sent."""
    if not any(word in items for word in FLAG_WORDS):
        return previous
    low = items.get(0, (previous or 0) & 0xFFFFFFFFFFFFFFFF)
    high = items.get(92, (previous or 0) >> 64)
    return (int(high) << 64) | (int(low) & 0xFFFFFFFFFFFFFFFF)


def flag_word(items: dict[int, object]) -> int | None:
    """The 128 actor flags `items` set, unsigned (a missing word is 0)."""
    flags = flags_of(items, None)
    return None if flags is None else flags & (2**128 - 1)


def rounded(values, digits=4) -> list[float]:
    return [round(v, digits) for v in values]


# --- tracked ----------------------------------------------------------------


def added_flags(records: list[dict]) -> dict[int, int | None]:
    """The actor flags of every AddActor of a capture, by runtime id: an
    item's AddActor can arrive before its first mark, after the previous
    item's last one."""
    return {record['packet']['target_runtime_id']['actor_runtime_id']:
            flags_of(data_items(record['packet']['actor_data']['data']), None)
            for record in records if record['name'] == 'AddActorPacket'}


def actor_packets(rows: list[tuple[int, dict]], runtime_ids: set[int],
                  adds: dict[int, int | None]) -> dict[int, dict]:
    """Per tracked runtime id: flags at AddActor, flag changes, actor events
    and sounds."""
    out: dict[int, dict] = {rid: {'flags': adds.get(rid), 'flag_changes': [], 'events': [], 'sounds': []}
                            for rid in runtime_ids}
    unique: dict[int, int] = {}
    for t, record in rows:
        packet, name = record['packet'], record['name']
        if name == 'AddActorPacket':
            rid = packet['target_runtime_id']['actor_runtime_id']
            if rid in out:
                unique[packet['target_actor_id']['actor_unique_id']] = rid
                out[rid]['flags'] = flags_of(data_items(packet['actor_data']['data']), None)
                out[rid]['added'] = t
        elif name == 'SetActorDataPacket':
            rid = packet['target_runtime_id']['actor_runtime_id']
            if rid not in out:
                continue
            entry = out[rid]
            previous = entry.get('current', entry['flags'])
            now = flags_of(data_items(packet['actor_data']['data']), previous)
            if previous is not None and now is not None and now != previous:
                changed = previous ^ now
                for bit in range(128):
                    if changed >> bit & 1:
                        entry['flag_changes'].append({'t': t, 'flag': bit, 'set': bool(now >> bit & 1)})
            entry['current'] = now
        elif name == 'ActorEventPacket':
            rid = packet['target_runtime_id']['actor_runtime_id']
            if rid in out:
                out[rid]['events'].append({'t': t, 'event': packet['event_id'], 'data': packet['data']})
        elif name == 'LevelSoundEventPacket':
            rid = unique.get(packet['actor_unique_id'])
            if rid is not None:
                out[rid]['sounds'].append({'t': t, 'sound': packet['sound_event']})
    for entry in out.values():
        entry.pop('current', None)
        if entry['flags'] is not None:
            entry['flags'] = str(entry['flags'])
    return out


def distil_tracked(result: dict, rows: list[tuple[int, dict]], adds: dict[int, int | None]) -> dict:
    spec = result['spec']
    actors: dict[int, dict] = {}
    for sample in result['samples']:
        for actor in sample.get('tracked', []):
            entry = actors.setdefault(actor['id'], {'type': actor['type'], 'first': sample['t'], 'ticks': []})
            entry['ticks'].append([sample['t'], *rounded(actor['pos']), round(actor['rot'][1], 3),
                                   round(actor['rot'][0], 3), *rounded(actor['vel']), int(bool(actor['ground'])),
                                   actor.get('health')])
    packets = actor_packets(rows, set(actors), adds)
    player, last = [], None
    for sample in result['samples']:
        feet = rounded(sample['player']['pos'])
        if feet != last:
            player.append([sample['t'], *feet])
            last = feet
    return {
        'key': result['key'].split('|', 1)[1],
        'player': rounded(spec['player']),
        'setup': spec['setup'],
        'actions': spec['actions'],
        'ticks': spec['ticks'],
        'player_ticks': player,
        'damage': [{'t': d[0] - result['first_tick'], 'type': d[1], 'amount': d[2], 'cause': d[3]}
                   for d in result['damage']],
        'actors': [{**actors[rid], **packets[rid]} for rid in sorted(actors, key=lambda r: actors[r]['first'])],
    }


def run_tracked(args: list[str]) -> int:
    only, events_only = None, set()
    while args[:1] in (['--only'], ['--events-only']) and len(args) > 1:
        kinds = set(args[1].split(','))
        if args[0] == '--only':
            only = kinds
        else:
            events_only = kinds
        args = args[2:]
    if len(args) < 2:
        print(__doc__)
        return 2
    out = Path(args[0])
    items: dict[str, dict] = {}
    sources = []
    for directory in map(Path, args[1:]):
        records = decoded(directory / 'client' / 'packets.jsonl')
        rows, adds = marked(records), added_flags(records)
        job = json.loads((directory / 'job.json').read_text(encoding='utf-8'))
        sources.append(job['items'][0]['key'].split('|', 1)[0])
        for line in (directory / 'results.jsonl').read_text(encoding='utf-8').splitlines():
            result = json.loads(line)
            kind = result['key'].split('|', 1)[1].rsplit('-', 1)[0]
            if only is not None and kind not in only and kind not in events_only:
                continue
            item = distil_tracked(result, rows.get(result['key'], []), adds)
            if kind in events_only:
                for actor in item['actors']:
                    actor['ticks'] = []
            items[result['key']] = item
    kinds = ', '.join(sorted(set(sources)))
    fixture = {
        'source': f'BDS 1.26.51, sweep.py {kinds} --client-capture (tools/bds-sweep/mob_fixture.py)',
        'columns': ['t', 'x', 'y', 'z', 'yaw', 'pitch', 'vx', 'vy', 'vz', 'ground', 'health'],
        'items': sorted(items.values(), key=lambda item: item['key']),
    }
    out.write_text(json.dumps(fixture, separators=(',', ':')).replace('{"key"', '\n{"key"') + '\n',
                   encoding='utf-8', newline='\n')
    print(f'{len(items)} items -> {out}')
    return 0


# --- events -----------------------------------------------------------------


def distil_events(result: dict, rows: list[tuple[int, dict]], label: str) -> dict:
    runtime_id = result['runtime_id']
    player_id = result['player_runtime_id']
    unique_id = None
    # Other actors (arrows) seen during the item: unique id -> type.
    others: dict[int, str] = {}
    events: list[dict] = []
    flags = None
    data: dict[int, object] = {}
    for t, record in rows:
        packet, name = record['packet'], record['name']
        if name == 'AddActorPacket' and packet['target_runtime_id']['actor_runtime_id'] == runtime_id:
            unique_id = packet['target_actor_id']['actor_unique_id']
            items = data_items(packet['actor_data']['data'])
            flags = flag_word(items)
            data = {k: v for k, v in items.items() if k not in SKIPPED_DATA}
            events.append({'t': t, 'event': 'add'})
        elif name == 'AddActorPacket' and packet['actor_type'] != result['mob']:
            others[packet['target_actor_id']['actor_unique_id']] = packet['actor_type']
            events.append({'t': t, 'event': 'actor', 'type': packet['actor_type'],
                           'at': rounded(packet['position'].values()), 'velocity': rounded(packet['velocity'].values())})
        elif name == 'RemoveActorPacket' and packet['target_actor_id']['actor_unique_id'] in others:
            events.append({'t': t, 'event': 'actor_removed',
                           'type': others[packet['target_actor_id']['actor_unique_id']]})
        elif name == 'MobEquipmentPacket' and packet['target_runtime_id']['actor_runtime_id'] == runtime_id:
            item = packet['item']
            events.append({'t': t, 'event': 'equipment', 'container': packet['container_id'], 'slot': packet['slot'],
                           'item': item['id'], 'count': item['stacksize'], 'block': item['block_runtime_id']})
        elif name == 'MoveActorDeltaPacket' and packet['move_data']['actor_runtime_id']['actor_runtime_id'] == runtime_id:
            move = packet['move_data']
            if move['force_move']:
                events.append({'t': t, 'event': 'forced_move', 'ticks': move['ticks'],
                               'on_ground': move['is_on_ground']})
        elif name == 'LevelEventGenericPacket' and packet['event_id'] != 9801:
            events.append({'t': t, 'event': 'level_event_generic', 'id': packet['event_id'],
                           'data': {k: round(v, 4) if isinstance(v, float) else v
                                    for k, v in packet['__ctd__'].items()}})
        elif name == 'SetActorDataPacket' and packet['target_runtime_id']['actor_runtime_id'] == runtime_id:
            items = data_items(packet['actor_data']['data'])
            if TARGET in items:
                events.append({'t': t, 'event': 'target', 'set': items[TARGET] != 0})
            word = flag_word(items)
            if word is not None and flags is not None:
                for bit in range(128):
                    old, new = flags >> bit & 1, word >> bit & 1
                    if old != new:
                        events.append({'t': t, 'event': 'flag', 'bit': bit, 'set': bool(new)})
                flags = word
            for key, value in items.items():
                if key not in SKIPPED_DATA and data.get(key) != value:
                    events.append({'t': t, 'event': 'data', 'id': key, 'value': value})
                    data[key] = value
        elif name == 'LevelSoundEventPacket' and unique_id is not None and packet['actor_unique_id'] == unique_id:
            events.append({'t': t, 'event': 'sound', 'sound': packet['sound_event'],
                           'at': rounded(packet['position'].values())})
        elif name == 'ActorEventPacket' and packet['target_runtime_id']['actor_runtime_id'] == runtime_id:
            events.append({'t': t, 'event': 'actor_event', 'id': packet['event_id'], 'data': packet['data']})
        elif name == 'RemoveActorPacket' and packet['target_actor_id']['actor_unique_id'] == unique_id:
            events.append({'t': t, 'event': 'removed'})
        elif name == 'MobEffectPacket' and packet['target_runtime_id']['actor_runtime_id'] == player_id:
            events.append({'t': t, 'event': 'effect', 'effect': packet['effect_id'], 'op': packet['event_id'],
                           'amplifier': packet['effect_amplifier'], 'duration': packet['effect_duration_ticks']})
        elif name == 'UpdateBlockPacket':
            pos = packet['block_position']
            events.append({'t': t, 'event': 'block', 'pos': [pos['x'], pos['y'], pos['z']],
                           'network_id': packet['block_runtime_id'], 'layer': packet['layer']})
        elif name == 'AddItemActorPacket':
            item = packet['item']
            events.append({'t': t, 'event': 'item', 'network_id': item.get('network_id'),
                           'count': item.get('count'), 'at': rounded(packet['position'].values(), 3)})
        elif name == 'LevelEventPacket':
            events.append({'t': t, 'event': 'level_event', 'id': packet['event_id'], 'data': packet['data'],
                           'at': rounded(packet['position'].values(), 3)})
    spec = result['spec']
    moves, last = [], None
    feet, last_feet = [], None
    for sample in result['samples']:
        mob = sample.get('mob')
        if mob is not None:
            here = rounded(mob['pos'], 5) + [mob['ground']]
            if here[:3] != (last or [None] * 3)[:3] or sample['t'] == 0:
                moves.append([sample['t']] + here)
            last = here
        player = rounded(sample['player']['pos'], 4)
        if player != last_feet:
            feet.append([sample['t']] + player)
            last_feet = player
    first = result['first_tick']
    # The sweep moves and turns the player with `tp "<player>" x y z yaw pitch`.
    turns = []
    for action in spec.get('actions', []):
        for command in action['commands']:
            words = command.split()
            if len(words) == 7 and words[0] == 'tp' and words[1].startswith('"'):
                turns.append({'t': action['t'], 'feet': [float(w) for w in words[2:5]],
                              'yaw': float(words[5]), 'pitch': float(words[6])})
    return {
        'key': f'{label}/{result["key"].split("|")[1]}',
        'mob': result['mob'],
        'at': spec['at'],
        'player': spec['player'],
        'frozen': spec['frozen'],
        'setup': spec.get('setup', []),
        'actions': [{'t': a['t'], 'commands': a['commands'], 'order': a.get('order')}
                    for a in spec.get('actions', [])],
        'ticks': spec['ticks'],
        # [t, x, y, z, on_ground] on every tick the mob moved.
        'moves': moves,
        # [t, x, y, z] of the player's feet whenever they changed.
        'player_feet': feet,
        # Every timed `tp` of the player: where it stood and looked.
        'player_turns': turns,
        'damage': [{'t': d[0] - first, 'actor': d[1], 'amount': round(d[2], 4), 'cause': d[3]}
                   for d in result['damage']],
        'events': events,
    }


def run_events(args: list[str]) -> int:
    if len(args) < 2:
        print(__doc__)
        return 2
    out = Path(args[0])
    args = args[1:]
    skip: tuple[str, ...] = ()
    if args[:1] == ['--skip']:
        skip, args = tuple(args[1].split(',')), args[2:]
    items: dict[str, dict] = {}
    for arg in args:
        label, sweep = arg.split('=', 1)
        sweep = Path(sweep)
        rows = marked(decoded(sweep / 'client' / 'packets.jsonl'))
        for line in (sweep / 'results.jsonl').read_text(encoding='utf-8').splitlines():
            result = json.loads(line)
            if result['key'].split('|')[1].startswith(skip):
                continue
            item = distil_events(result, rows.get(result['key'], []), label)
            items[item['key']] = item
    fixture = {'source': 'BDS 1.26.51, tools/bds-sweep/sweep.py --client-capture, distilled by '
                         'tools/bds-sweep/mob_fixture.py',
               'items': list(items.values())}
    out.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(fixture, separators=(',', ':'))
    text = text.replace('{"key"', '\n{"key"').replace('{"t"', '\n{"t"').replace('],[', '],\n[')
    out.write_text(text + '\n', encoding='utf-8', newline='\n')
    return 0


def main() -> int:
    modes = {'tracked': run_tracked, 'events': run_events}
    if len(sys.argv) < 2 or sys.argv[1] not in modes:
        print(__doc__)
        return 2
    return modes[sys.argv[1]](sys.argv[2:])


if __name__ == '__main__':
    sys.exit(main())
