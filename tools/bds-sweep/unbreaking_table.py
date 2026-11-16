"""The wear BDS applied with Unbreaking 0-III, from a capture of
`scripts/unbreaking.txt` or `scripts/unbreaking-events.txt` (printed by
`unbreaking_script.py`) played with `script.py`:

    target/debug/bds-sweep-dump <out>/client-capture/packets.jsonl \\
        --name Text,LevelEvent,InventorySlot,InventoryContent,PlayerArmorDamage,InventoryTransaction \\
        | python tools/bds-sweep/unbreaking_table.py --script SCRIPT > table.json

Each section begins at its `/say unbreaking KIND [PART] LEVEL` marker (the
`Text` packet BDS sends back); a `use` section's markers also mark a fresh
item.

- `mining`: per level, the breaks (destroy particles, 2001) and the points of
  wear they cost (the `Damage` of the pickaxe's last slot update; BDS sends
  one only when it wears);
- `armour`: per level and piece, how many hits wore it by 0, 1 or 2 points
  (each hit wears every piece by 2 before Unbreaking): `PlayerArmorDamage`
  carries the pieces' new damage after each hit that wore one, so the hits
  that wore none are the script's hit count less those;
- `use`: per level, the flint and steel uses (the script's) and the points of
  wear (each fresh item's highest `Damage` before the next marker; the
  `/clear` before a fresh one resends it empty);
- `attack`, `spear`: per level, the hits the client sent (its
  `ItemUseOnActor` transactions) and how many wore 0, 1 or 2 points (each
  rise of the held item's `Damage` in the inventory BDS resends after every
  hit is one hit's wear);
- `sword`: the same for broken cobweb (2001), from the sword's slot updates;
- `elytra`: per level, the script's glides and the points they wore (the
  chest piece's last `PlayerArmorDamage`);
- `threshold`: per part (`mining`, `armour`) and starting damage, the
  damage after each wear event, `null` once the slot is sent empty (the
  item broke).

Output: `{"mining": {LEVEL: {"samples", "worn"}}, "armour": {LEVEL: {"hits",
"pieces": {PIECE: [hits worn 0, 1, 2]}}}, "use": {LEVEL: {"samples",
"worn"}}, "attack"|"spear"|"sword": {LEVEL: {"events", "worn": [0, 1, 2]}},
"elytra": {LEVEL: {"glides", "points"}}, "threshold": {PART: {DAMAGE:
[damage or null, ...]}}}`, each kind only when the capture has it."""
import argparse
import json
import re
import sys
from pathlib import Path

MARKER = re.compile(r'unbreaking (mining|armour|use|attack|spear|sword|elytra|threshold) (?:(mining|armour) )?(\d+)')
ARMOUR_PAIR = re.compile(r'armor_slot: (\w+), damage: (\d+)')
USER_DATA = re.compile(r'user_data_buffer: b"((?:[^"\\]|\\.)*)"')
ESCAPES = {'0': 0, 'n': 10, 't': 9, 'r': 13, '\\': 92, '"': 34, "'": 39}


def unescape(text):
    """The bytes of a Rust byte-string literal's contents."""
    out = bytearray()
    index = 0
    while index < len(text):
        char = text[index]
        if char != '\\':
            out += char.encode('latin-1')
            index += 1
        elif text[index + 1] == 'x':
            out.append(int(text[index + 2:index + 4], 16))
            index += 4
        else:
            out.append(ESCAPES[text[index + 1]])
            index += 2
    return bytes(out)


def damage(body):
    """The `Damage` (int tag) of the first stack in `body` that has one, or 0
    when none has (the tested item is the only one that wears)."""
    for match in USER_DATA.finditer(body):
        data = unescape(match.group(1))
        at = data.find(b'\x03\x06\x00Damage')
        if at >= 0:
            return int.from_bytes(data[at + 9:at + 13], 'little', signed=True)
    return 0


def broken(body):
    """Whether an `InventorySlot` carries an empty stack."""
    return re.search(r'item: \w+ \{ id: 0,', body) is not None


def script_counts(script):
    """From the script: hits per armour level, uses per use marker and
    glides per elytra level."""
    hits, uses, glides = {}, {}, {}
    kind = level = None
    for line in Path(script).read_text(encoding='utf-8').splitlines():
        parts = line.split(maxsplit=1)
        if len(parts) < 2 or parts[0].startswith('#'):
            continue
        marker = MARKER.search(parts[1])
        if parts[1].startswith('/say') and marker:
            kind, level = marker.group(1), marker.group(3)
            if kind == 'use':
                uses.setdefault(level, []).append(0)
        elif kind == 'armour' and parts[1].startswith('/damage'):
            hits[level] = hits.get(level, 0) + 1
        elif kind == 'use' and parts[1].startswith('use-on'):
            uses[level][-1] += 1
        elif kind == 'elytra' and parts[1].startswith('/tp @s 0.5 300'):
            counts = glides.setdefault(kind, {})
            counts[level] = counts.get(level, 0) + 1
    return hits, uses, glides


class Events:
    """Per-event wear of one item: the points each rise of its `Damage`
    cost, the events that cost none being the events less the rises."""

    def __init__(self, events=0):
        self.events = events
        self.last = 0
        self.changes = []

    def saw(self, value):
        # Less damage is the next section's fresh item (or the `/clear`
        # before it), not wear.
        if value > self.last:
            self.changes.append(value - self.last)
        self.last = value

    def row(self):
        worn = [self.events - len(self.changes), 0, 0]
        for points in self.changes:
            worn[points] += 1
        return {'events': self.events, 'worn': worn}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--script', default=Path(__file__).parent / 'scripts' / 'unbreaking.txt')
    args = parser.parse_args()
    sys.stdout.reconfigure(newline='\n')
    hits, uses, glides = script_counts(args.script)
    mining, armour, use = {}, {}, {}
    events, elytra, threshold = {}, {}, {}
    kind = level = None
    last = {}
    use_items = []
    for line in sys.stdin:
        parts = line.split(' ', 4)
        if len(parts) < 5:
            continue
        # `bds-sweep-dump` names packets with their `Packet` suffix.
        name, body = parts[3].removesuffix('Packet'), parts[4]
        if parts[2] == 'c2s':
            if kind in ('attack', 'spear') and name == 'InventoryTransaction' and 'ItemUseOnActor' in body:
                events[kind][level].events += 1
            continue
        marker = MARKER.search(body) if name == 'Text' else None
        if marker:
            kind, level = marker.group(1), marker.group(3)
            if kind == 'mining':
                mining[level] = {'samples': 0, 'worn': 0}
            elif kind == 'armour':
                armour[level] = {'hits': hits[level], 'pieces': {}}
                last = {}
            elif kind == 'use':
                use_items.append((level, 0))
            elif kind in ('attack', 'spear', 'sword'):
                events.setdefault(kind, {})[level] = Events()
            elif kind == 'elytra':
                elytra[level] = {'glides': glides['elytra'][level], 'points': 0}
            else:
                kind = (kind, marker.group(2))
                threshold.setdefault(marker.group(2), {})[level] = []
            continue
        if kind == 'mining' and name == 'LevelEvent' and 'event_id: 2001,' in body:
            mining[level]['samples'] += 1
        elif kind == 'mining' and name == 'InventorySlot' and 'container_id: 0, slot: 0,' in body:
            mining[level]['worn'] = damage(body)
        elif kind == 'armour' and name == 'PlayerArmorDamage':
            pieces = armour[level]['pieces']
            for piece, total in ARMOUR_PAIR.findall(body):
                worn = int(total) - last.get(piece, 0)
                last[piece] = int(total)
                pieces.setdefault(piece, [0, 0, 0])[worn] += 1
        elif kind == 'use' and name == 'InventoryContent' and 'container_id: 0,' in body:
            use_items[-1] = (level, max(use_items[-1][1], damage(body)))
        elif kind in ('attack', 'spear') and name == 'InventoryContent' and 'container_id: 0,' in body:
            events[kind][level].saw(damage(body))
        elif kind == 'sword' and name == 'LevelEvent' and 'event_id: 2001,' in body:
            events[kind][level].events += 1
        elif kind == 'sword' and name == 'InventorySlot' and 'container_id: 0, slot: 0,' in body:
            events[kind][level].saw(damage(body))
        elif kind == 'elytra' and name == 'PlayerArmorDamage':
            elytra[level]['points'] = max(int(total) for _, total in ARMOUR_PAIR.findall(body))
        elif isinstance(kind, tuple) and name == 'InventorySlot' and (
                (kind[1] == 'mining' and 'container_id: 0, slot: 0,' in body)
                or (kind[1] == 'armour' and 'container_id: 120, slot: 0,' in body)):
            seen = threshold[kind[1]][level]
            if None not in seen:
                seen.append(None if broken(body) else damage(body))
        elif isinstance(kind, tuple) and kind[1] == 'armour' and name == 'PlayerArmorDamage':
            seen = threshold['armour'][level]
            if None not in seen:
                seen += [int(total) for _, total in ARMOUR_PAIR.findall(body)]
    for table in armour.values():
        for counts in table['pieces'].values():
            counts[0] = table['hits'] - counts[1] - counts[2]
    for item_level, worn in use_items:
        row = use.setdefault(item_level, {'samples': sum(uses[item_level]), 'worn': 0})
        row['worn'] += worn
    table = {'mining': mining, 'armour': armour, 'use': use, 'elytra': elytra, 'threshold': threshold}
    table.update({kind: {level: row.row() for level, row in rows.items()} for kind, rows in events.items()})
    print(json.dumps({kind: rows for kind, rows in table.items() if rows}, indent=1))


if __name__ == '__main__':
    main()
