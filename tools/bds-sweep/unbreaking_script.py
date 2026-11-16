"""Print tools/bds-sweep/scripts/unbreaking.txt: wear with Unbreaking 0-III
sampled on BDS, for `unbreaking_table.py`:

    python tools/bds-sweep/unbreaking_script.py > tools/bds-sweep/scripts/unbreaking.txt
    python tools/bds-sweep/script.py tools/bds-sweep/scripts/unbreaking.txt --out DIR --timeout 1800

`--kinds armour,use` prints only those sections (the committed table's armour
and item use come from such a run; its mining from the full script, whose
mining section is the same).

Three kinds of wear, each at every level:

- mining: a netherite pickaxe with Efficiency V and Haste II breaks stone with
  every input, so the player keeps breaking one position and the stone is put
  back every other tick (tile drops off); the pickaxe's slot is resent when
  it wears;
- armour: four diamond pieces with the same level, each equipped from its own
  hotbar slot, take `/damage @s 8 entity_attack` (2 points of wear each) once
  every 11 ticks, outside the 10 invulnerable ticks; BDS sends the worn
  pieces' damage (`PlayerArmorDamage`);
- item use: flint and steel lights fire on netherrack, which is cleared after
  each use; BDS resends the inventory after each. A fresh one, with its
  marker, every 40 uses (it lasts 64).

The scripted client does not apply the inventory change `/enchant` makes,
so its next item use claims the old stack and BDS refuses it, resending the
inventory: each enchanted armour piece is used twice (the second use equips
it) and each flint and steel once in the air before its uses.

Per-event sections (`scripts/unbreaking-events.txt` is
`--kinds attack,spear,sword,elytra,threshold`), at levels 0 and III:

- attack: a netherite pickaxe hits a named pig (2 points a hit); spear: an
  iron spear does (1 point, its `damage_chance` 0-100), 26 inputs apart for
  its cooldown. BDS resends the inventory after every hit;
- sword: a netherite sword keeps breaking cobweb (2 points a block);
- elytra: glides from y=300, worn every 20 ticks (1 point each);
- threshold: a netherite pickaxe at damage 2030 and 2031 (of 2031) breaks
  stone, a diamond helmet at 362 and 363 (of 363) takes hits of 4 (1 point
  each), whether the item survives.

Each section begins with a `/say unbreaking KIND [PART] LEVEL` marker, which
`unbreaking_table.py` reads back from the capture."""
import argparse
import sys

sys.stdout.reconfigure(newline='\n')
ARMOUR = ['diamond_helmet', 'diamond_chestplate', 'diamond_leggings', 'diamond_boots']
ARMOUR_SLOTS = ['slot.armor.head', 'slot.armor.chest', 'slot.armor.legs', 'slot.armor.feet']

parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--breaks', type=int, default=800, help='mining samples per level I-III (level 0: an eighth)')
parser.add_argument('--hits', type=int, default=250, help='armour hits per level I-III (four pieces each)')
parser.add_argument('--uses', type=int, default=400, help='flint and steel uses per level I-III')
parser.add_argument('--events', type=int, default=400,
                    help='Unbreaking III pickaxe hits (spear: half; sword: inputs breaking cobweb, four times)')
parser.add_argument('--glides', type=int, default=10, help='Unbreaking III elytra glides from y=300')
parser.add_argument('--kinds', default='mining,armour,use',
                    help='the sections to print: mining, armour, use; attack, spear, sword, elytra, threshold')
args = parser.parse_args()
kinds = args.kinds.split(',')

lines = [
    '# Unbreaking sampled on BDS 1.26.51; printed by tools/bds-sweep/unbreaking_script.py (see there).',
    '# The player (feet (0.5,-60,0.5), facing east) breaks the west face (4) of (1,-59,0).',
]
tick = 20


def at(order, gap=1):
    global tick
    lines.append(f'{tick} {order}')
    tick += gap


for order in ['/gamerule sendcommandfeedback false', '/gamerule domobspawning false',
              '/gamerule dodaylightcycle false', '/gamerule randomtickspeed 0', '/gamerule dotiledrops false',
              '/time set 6000', '/fill -4 -61 -4 4 -61 4 stone', '/fill -4 -60 -4 4 -55 4 air',
              '/gamemode survival @s', '/clear @s', '/tp @s 0.5 -60 0.5 -90 0',
              '/effect @s resistance 100000 3 true', '/effect @s regeneration 100000 4 true',
              '/effect @s haste 100000 1 true']:
    at(order)
tick += 10

for level in range(4 if 'mining' in kinds else 0):
    breaks = args.breaks if level else max(args.breaks // 8, 1)
    at('/clear @s')
    at('/replaceitem entity @s slot.hotbar 0 netherite_pickaxe', 3)
    at('/enchant @s efficiency 5', 3)
    if level:
        at(f'/enchant @s unbreaking {level}', 3)
    at(f'/say unbreaking mining {level}', 3)
    at('/setblock 1 -59 0 stone', 2)
    at('mine 1 -59 0 4 0', 2)
    for _ in range(breaks - 1):
        at('/setblock 1 -59 0 stone', 2)
    at('stop', 5)

for level in range(1, 4 if 'armour' in kinds else 1):
    at('/clear @s')
    for slot in ARMOUR_SLOTS:
        at(f'/replaceitem entity @s {slot} 0 air')
    for index, piece in enumerate(ARMOUR):
        at(f'/replaceitem entity @s slot.hotbar {index + 1} {piece}', 2)
    for index in range(len(ARMOUR)):
        at(f'select {index + 1}', 3)
        at(f'/enchant @s unbreaking {level}', 3)
        at('use', 5)
        at('use', 5)
    at('select 0', 3)
    at(f'/say unbreaking armour {level}', 3)
    for _ in range(args.hits):
        at('/damage @s 8 entity_attack', 11)
    tick += 5

at('/clear @s')
for slot in ARMOUR_SLOTS:
    at(f'/replaceitem entity @s {slot} 0 air')
at('/setblock 2 -60 0 netherrack', 2)
for level in range(1, 4 if 'use' in kinds else 1):
    for use in range(args.uses):
        # A fresh one every 40 uses: flint and steel lasts 64.
        if use % 40 == 0:
            at('/clear @s')
            at('/replaceitem entity @s slot.hotbar 0 flint_and_steel', 3)
            at(f'/enchant @s unbreaking {level}', 3)
            at('use', 3)
            at(f'/say unbreaking use {level}', 3)
        at('use-on 2 -60 0 1', 2)
        at('/setblock 2 -59 0 air', 2)
    tick += 5

def fresh_held(item, level, *extra):
    """A fresh `item` in hotbar slot 0 with Unbreaking `level`, used once in
    the air so BDS resends the inventory (the scripted client does not apply
    `/enchant` itself)."""
    at('/clear @s')
    at(f'/replaceitem entity @s slot.hotbar 0 {item}', 3)
    for enchantment in extra:
        at(f'/enchant @s {enchantment}', 3)
    if level:
        at(f'/enchant @s unbreaking {level}', 3)
    at('use', 5)


pig = 0


def hits(kind, item, levels, gap):
    """Hits per (level, count) on a fresh named pig 2 blocks east, standing
    at eye height on a stone walk, each killed after its hit (BDS resends
    the inventory after every hit)."""
    global pig
    at('/gamerule domobloot false')
    at('/fill 1 -60 0 9 -60 1 stone', 2)
    for level, samples in levels:
        fresh_held(item, level)
        at(f'/say unbreaking {kind} {level}', 3)
        for _ in range(samples):
            pig += 1
            at(f'/summon pig 2.5 -59 0.5 0 0 minecraft:spawn_adult p{pig}', 5)
            at(f'attack-named p{pig}', 4)
            at('/kill @e[type=pig]', gap - 9)
    at('/fill 1 -60 0 9 -60 1 air', 5)


if 'attack' in kinds:
    hits('attack', 'netherite_pickaxe', [(0, args.events // 16), (3, args.events)], 14)

if 'spear' in kinds:
    # A spear's attack cooldown is 19 inputs (scripts/spears.txt).
    hits('spear', 'iron_spear', [(0, args.events // 16), (3, args.events // 2)], 26)

if 'sword' in kinds:
    # The sword keeps breaking cobweb at the mining position; a `keep`
    # setblock puts it back once broken.
    for level, inputs in [(0, args.events // 4), (3, args.events * 4)]:
        fresh_held('netherite_sword', level)
        at(f'/say unbreaking sword {level}', 3)
        at('/setblock 1 -59 0 web', 2)
        at('mine 1 -59 0 4 0', 2)
        for _ in range(inputs // 2):
            at('/setblock 1 -59 0 web keep', 2)
        at('stop', 5)
        at('/setblock 1 -59 0 air', 3)

if 'elytra' in kinds:
    # Glides from y=300 (30 wear events each at level 0); BDS reports each
    # wear with `PlayerArmorDamage`.
    at('/gamerule falldamage false')
    for level, glides in [(0, 2), (3, args.glides)]:
        at('/clear @s')
        at('/replaceitem entity @s slot.armor.chest 0 air', 3)
        at('/replaceitem entity @s slot.hotbar 0 elytra', 3)
        if level:
            at(f'/enchant @s unbreaking {level}', 3)
        # The first use is refused (the client's stack is stale), the
        # second puts the elytra on.
        at('use', 5)
        at('use', 5)
        at(f'/say unbreaking elytra {level}', 3)
        for _ in range(glides):
            at('/tp @s 0.5 300 0.5 -90 0', 5)
            at('flags Jumpdown,Jumppressedraw,Jumpcurrentraw', 5)
            at('held-flags Jumping,Jumpcurrentraw', 10)
            at('held-flags none', 600)
            at('/tp @s 0.5 -60 0.5 -90 0', 10)
    at('/replaceitem entity @s slot.armor.chest 0 air', 3)

if 'threshold' in kinds:
    # A netherite pickaxe (2031 durability) one short of its maximum damage
    # and at it, breaking stone twice; a diamond helmet (363) the same,
    # taking two hits of 4 (1 point of wear each).
    for damage in (2030, 2031):
        fresh_held(f'netherite_pickaxe 1 {damage}', 0, 'efficiency 5')
        at(f'/say unbreaking threshold mining {damage}', 3)
        for _ in range(2):
            at('/setblock 1 -59 0 stone', 2)
            at('mine 1 -59 0 4 0', 4)
            at('stop', 4)
        at('/setblock 1 -59 0 air', 3)
    for damage in (362, 363):
        at('/clear @s')
        at(f'/replaceitem entity @s slot.armor.head 0 diamond_helmet 1 {damage}', 3)
        at(f'/say unbreaking threshold armour {damage}', 3)
        at('/damage @s 4 entity_attack', 15)
        at('/damage @s 4 entity_attack', 15)
        at('/replaceitem entity @s slot.armor.head 0 air', 3)

at('exit')
print('\n'.join(lines))
