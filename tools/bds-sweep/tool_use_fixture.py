"""Cut the tool-use fixtures out of a capture of `scripts/tool-use.txt`
(printed by `tool_use_script.py`, played with `script.py`):

    python tools/bds-sweep/tool_use_fixture.py cases <out>/client-capture/packets.jsonl \\
        tools/bds-sweep/scripts/tool-use.txt crates/unastar/src/server/game/item_use/testdata/bds-tool-use
    target/debug/bds-sweep-dump <out>/client-capture/packets.jsonl --input-tick \\
        --name Text,InventoryContent,AddItemActor,InventoryTransaction \\
        | python tools/bds-sweep/tool_use_fixture.py table tools/bds-sweep/scripts/tool-use.txt \\
        > crates/unastar/src/server/game/item_use/testdata/bds-tool-use-wear.json

`cases` writes `PREFIX.jsonl`, the packets of every case's window (from its
`/say tooluse case NAME` marker to 15 inputs later; `combat_fixture.cut`
with the inventory BDS resends and level events kept too), and
`PREFIX-cases.json`, each case as the script set it up: `name`, the held
`item` and `count`, the game `mode`, the `block` (`pos`, `name`, `states`),
the block put `above` it, the clicked `face` of `pos`, and the window's
`inputs` `[from, to)`.

`table` reads the decoded packets and writes, per wear section
(`/say tooluse wear KIND LEVEL`), the clicks or sheep the script made
(`uses`) and how many of them wore the item by 0, 1 or 2 points (`worn`:
each rise of the held item's `Damage` in the inventory BDS resends after a
use is one use's wear); and for `drops`, the husks killed and the
`[network id, Damage]` of every item they dropped (`AddItemActor`)."""
import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import combat_fixture  # noqa: E402
from unbreaking_table import damage  # noqa: E402

LINE = re.compile(r'^(\d+) (.*)$')
SETBLOCK = re.compile(r'^/setblock (-?\d+) (-?\d+) (-?\d+) (\w+)(?: \[(.*)\])?$')
MARKER = re.compile(r'tooluse (case|wear|drops) (\w+)(?: (\d+))?')
WINDOW = 15
# Script cases whose block is not a Bedrock identifier (BDS refused the
# `/setblock`; Bedrock's rooted dirt is `dirt_with_roots`, also scripted).
REFUSED_BLOCKS = {'minecraft:rooted_dirt'}


def script_lines(path):
    with open(path, encoding='utf-8') as lines:
        for line in lines:
            match = LINE.match(line.strip())
            if match:
                yield int(match.group(1)), match.group(2)


def states(text):
    """`"a"="x","b"=2` as a JSON object."""
    return json.loads('{' + (text or '').replace('=', ':') + '}')


def cases(script):
    out = []
    setup = {}
    for tick, order in script_lines(script):
        if order.startswith('/fill 1 -60 -1 3 -56 1 air'):
            setup = {'mode': 'survival', 'above': None}
        elif order.startswith('/gamemode '):
            setup['mode'] = order.split()[1]
        elif order.startswith('/replaceitem entity @s slot.hotbar 0 '):
            words = order.split()[5:]
            setup['item'] = 'minecraft:' + words[0]
            setup['count'] = int(words[1]) if len(words) > 1 else 1
        elif (match := SETBLOCK.match(order)) and 'item' in setup:
            pos = [int(v) for v in match.group(1, 2, 3)]
            block = {'pos': pos, 'name': 'minecraft:' + match.group(4), 'states': states(match.group(5))}
            if 'block' in setup and pos[1] == setup['block']['pos'][1] + 1:
                setup['above'] = block
            else:
                setup['block'] = block
        elif (match := MARKER.search(order)) and match.group(1) == 'case':
            setup['name'] = match.group(2)
            setup['inputs'] = [tick, tick + WINDOW]
        elif order.startswith('use-on ') and 'name' in setup:
            setup['face'] = int(order.split()[4])
            if setup['block']['name'] not in REFUSED_BLOCKS:
                out.append(setup)
            setup = {'mode': setup['mode'], 'above': None}
    return out


def table(script, dump):
    uses = {}
    section = None
    for _, order in script_lines(script):
        if (match := MARKER.search(order)) and match.group(1) != 'case':
            section = (match.group(1), match.group(2), match.group(3) or '0')
            uses[section] = 0
        elif section and (order.startswith('use-on ') or order.startswith('interact-named ')
                          or order.startswith('attack-named ')):
            uses[section] += 1
    wear = {}
    drops = {'husks': 0, 'items': []}
    section = None
    last = 0
    for line in dump:
        if 'TextPacket' in line:
            match = MARKER.search(line)
            section = (match.group(1), match.group(2), match.group(3) or '0') if match else section
            if match and match.group(1) == 'case':
                section = None
            if section and section[0] == 'wear':
                wear.setdefault(section[1], {})[section[2]] = {'uses': uses[section], 'worn': [0, 0, 0]}
                # Every section starts with a fresh item.
                last = 0
            elif section and section[0] == 'drops':
                drops['husks'] = uses[section]
            continue
        if section is None:
            continue
        if section[0] == 'wear' and 'InventoryContentPacket { container_id: 0,' in line:
            now = damage(line)
            if now >= last:
                row = wear[section[1]][section[2]]
                row['worn'][now - last] += 1
            last = now
        elif section[0] == 'drops' and 'AddItemActorPacket' in line:
            item = re.search(r'item: CerealizerNetworkItemStackDescriptorSerializedData \{ id: (\d+)', line)
            drops['items'].append([int(item.group(1)), damage(line)])
    # Uses that sent no wear (a resend with the same damage counts as 0, a
    # missing one would not): the remainder of the script's uses.
    for kind in wear.values():
        for row in kind.values():
            row['worn'][0] = row['uses'] - row['worn'][1] - row['worn'][2]
    return {'wear': wear, 'drops': drops}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    cut = sub.add_parser('cases')
    cut.add_argument('source')
    cut.add_argument('script')
    cut.add_argument('prefix')
    tab = sub.add_parser('table')
    tab.add_argument('script')
    args = parser.parse_args()
    if args.command == 'cases':
        found = cases(args.script)
        rows = [json.loads(line) for line in open(args.source, encoding='utf-8') if line.strip()]
        windows = [tuple(case['inputs']) for case in found]
        out = combat_fixture.cut(rows, windows, also={'InventoryContentPacket', 'LevelEventPacket'})
        with open(args.prefix + '.jsonl', 'w', encoding='utf-8', newline='\n') as f:
            for record in out:
                f.write(json.dumps(record, separators=(',', ':')) + '\n')
        with open(args.prefix + '-cases.json', 'w', encoding='utf-8', newline='\n') as f:
            json.dump(found, f, indent=1)
            f.write('\n')
        print(len(found), 'cases,', len(out), 'records')
    else:
        sys.stdout.reconfigure(newline='\n')
        print(json.dumps(table(args.script, sys.stdin), indent=1))


if __name__ == '__main__':
    main()
