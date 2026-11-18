"""Tables from captures of `scripts/items-6-{food,shield,totem}.txt` (printed by
`items6_script.py`, played with `script.py`):

    target/debug/bds-sweep-dump <out>/client-capture/packets.jsonl --input-tick \\
        --name Text,MobEffect,CompletedUsingItem \\
        | python tools/bds-sweep/items6_table.py food \\
        > crates/unastar/src/server/game/item_use/testdata/bds-food-chances.json

    python tools/bds-sweep/items6_table.py cases shield <out>/client-capture/packets.jsonl SCRIPT \
        crates/unastar/src/server/game/item_use/testdata/bds-shield

`food`: per food section (`/say items6 food NAME`), the eats BDS finished
(`CompletedUsingItem`) and how many `MobEffect` adds each effect id got
(not the one-tick instant health of the next section's heal, which comes
before its marker): `{NAME: {"eats", "effects": {EFFECT_ID: adds}}}`.

`cases PART`: the `shield` or `totem` part of a capture for a replay, cut
from where its first case starts to where its last ends
(`combat_fixture.cut`, the resent inventory, level sounds, effects,
cooldowns, attributes and actor events kept): `PREFIX.jsonl`, and
`PREFIX-cases.json` with each case's `name` and input window `[from, to)`.
A shield case starts at the script's `/tp` back to the start, a totem case
at its `/clear @s`; a case ends where the next starts (the last 60 inputs
after its marker). The shield part stops before the vindicators, the totem
part before `void`, where the player dies."""
import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import combat_fixture  # noqa: E402

MARKER = re.compile(r'items6 (\w+) (\w+)')
EFFECT = re.compile(r'event_id: Add, effect_id: (\d+),.*effect_duration_ticks: (\d+)')


def food(dump):
    out = {}
    section = None
    for line in dump:
        if 'TextPacket' in line and (match := MARKER.search(line)):
            section = match.group(2) if match.group(1) == 'food' else None
            if section:
                out[section] = {'eats': 0, 'effects': {}}
            continue
        if section is None:
            continue
        if 'CompletedUsingItemPacket' in line:
            out[section]['eats'] += 1
        elif 'MobEffectPacket' in line and (match := EFFECT.search(line)) and int(match.group(2)) > 1:
            effects = out[section]['effects']
            effects[match.group(1)] = effects.get(match.group(1), 0) + 1
    return out


LINE = re.compile(r'^(\d+) (.*)$')
OPENERS = {'shield': '/tp @s 0.5 -60 0.5 -90 0', 'totem': '/clear @s'}
# The first case left out of each part's replay: the vindicators (their AI
# picks the hit ticks) and the totem cases where the player dies.
STOP = {'shield': 'vindicator_blocking', 'totem': 'void'}


def cases(script, part):
    """The cases of `part` in `script`: name and input window."""
    lines = []
    with open(script, encoding='utf-8') as f:
        for line in f:
            if match := LINE.match(line.strip()):
                lines.append((int(match.group(1)), match.group(2)))
    out = []
    opener = None
    for tick, order in lines:
        if order == OPENERS[part]:
            opener = tick
        elif (match := MARKER.search(order)) and order.startswith('/say') and match.group(1) == part:
            if match.group(2) == STOP[part]:
                if out:
                    out[-1]['inputs'][1] = opener
                break
            if out:
                out[-1]['inputs'][1] = opener
            out.append({'name': match.group(2), 'inputs': [opener, tick + 60]})
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('food')
    cut = sub.add_parser('cases')
    cut.add_argument('part', choices=sorted(OPENERS))
    cut.add_argument('source')
    cut.add_argument('script')
    cut.add_argument('prefix')
    args = parser.parse_args()
    if args.command == 'food':
        sys.stdout.reconfigure(newline='\n')
        print(json.dumps(food(sys.stdin), indent=1, sort_keys=True))
        return
    found = cases(args.script, args.part)
    rows = [json.loads(line) for line in open(args.source, encoding='utf-8') if line.strip()]
    window = [(found[0]['inputs'][0], found[-1]['inputs'][1])]
    out = combat_fixture.cut(rows, window, also={'InventoryContentPacket', 'CommandOutputPacket', 'UpdateAttributesPacket'},
                             also_c2s={'CommandRequestPacket'})
    with open(args.prefix + '.jsonl', 'w', encoding='utf-8', newline='\n') as f:
        for record in out:
            f.write(json.dumps(record, separators=(',', ':')) + '\n')
    with open(args.prefix + '-cases.json', 'w', encoding='utf-8', newline='\n') as f:
        json.dump(found, f, indent=1)
        f.write('\n')
    print(len(found), 'cases,', len(out), 'records')


if __name__ == '__main__':
    main()
