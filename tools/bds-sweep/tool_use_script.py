"""Print tools/bds-sweep/scripts/tool-use.txt: tools used on blocks and
shears on sheep, captured on BDS 1.26.51 for `tool_use_fixture.py`:

    python tools/bds-sweep/tool_use_script.py > tools/bds-sweep/scripts/tool-use.txt
    python tools/bds-sweep/script.py tools/bds-sweep/scripts/tool-use.txt --out DIR --timeout 2400

The player stands at (0.5, -60, 0.5) facing east on a stone floor; each case
puts its block at (2, -60, 0) (or floating at (2, -58, 0) for a click from
below) and clicks it once with the held item (`use-on`). Every case begins
with a `/say tooluse case NAME` marker and a fresh item in hotbar slot 0
(used once in the air first, so the client holds the stack BDS last sent).

- cases: a hoe, a shovel and an axe on every block they convert or refuse
  (faces, a block above, a block from below), lit and unlit campfires,
  copper stages and waxed copper, honeycomb on copper, shears on a pumpkin,
  and each in creative;
- wear: `/say tooluse wear KIND LEVEL` and then repeated clicks with a
  netherite tool (Unbreaking 0 or III) that each convert a block put back
  before the click: an axe stripping oak logs, a hoe tilling dirt, a shovel
  flattening grass; and shears shearing a fresh named sheep each time
  (`interact-named`);
- drops: husks given an iron spear (`/replaceitem`), hurt to 1 health and
  killed by the player's sword: the spears they drop (`AddItemActor`).

`tool_use_fixture.py` cuts the packets of each marker's window into the
committed fixture."""
import argparse
import sys

sys.stdout.reconfigure(newline='\n')

parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--uses', type=int, default=300, help='Unbreaking III clicks per tool (level 0: a tenth)')
parser.add_argument('--shears', type=int, default=200, help='Unbreaking III sheep shorn (level 0: a tenth)')
parser.add_argument('--husks', type=int, default=160, help='husks holding an iron spear killed by the player')
parser.add_argument('--parts', default='cases,wear,drops', help='cases, wear, drops')
args = parser.parse_args()
parts = args.parts.split(',')

lines = [
    '# Tools used on blocks on BDS 1.26.51; printed by tools/bds-sweep/tool_use_script.py (see there).',
    '# The player (feet (0.5,-60,0.5), facing east) clicks the block at (2,-60,0).',
]
tick = 20


def at(order, gap=1):
    global tick
    lines.append(f'{tick} {order}')
    tick += gap


for order in ['/gamerule sendcommandfeedback false', '/gamerule domobspawning false',
              '/gamerule dodaylightcycle false', '/gamerule randomtickspeed 0', '/gamerule dofiretick false',
              '/time set 6000', '/fill -4 -61 -4 4 -61 4 stone', '/fill -4 -60 -4 4 -55 4 air',
              '/gamemode survival @s', '/clear @s', '/tp @s 0.5 -60 0.5 -90 0']:
    at(order)
tick += 10

BLOCK = '2 -60 0'
ABOVE = '2 -59 0'
FLOAT = '2 -58 0'


def fresh(item, level=0, mode='survival'):
    at('/clear @s')
    at(f'/gamemode {mode} @s', 2)
    at(f'/replaceitem entity @s slot.hotbar 0 {item}', 3)
    if level:
        at(f'/enchant @s unbreaking {level}', 3)
    at('use', 5)


def case(name, item, block, face=1, above=None, floating=False, mode='survival'):
    """One click of `item` on `block`: `face` of the block at (2,-60,0), or
    the bottom of one at (2,-58,0) when `floating`; `above` goes on top."""
    at(f'/fill 1 -60 -1 3 -56 1 air', 2)
    at('/kill @e[type=item]', 2)
    fresh(item, mode=mode)
    pos = FLOAT if floating else BLOCK
    at(f'/setblock {pos} {block}', 2)
    if above:
        at(f'/setblock {ABOVE} {above}', 2)
    at(f'/say tooluse case {name}', 3)
    at(f'use-on {pos} {0 if floating else face}', 12)


if 'cases' in parts:
    for block in ['grass_block', 'dirt', 'coarse_dirt', 'dirt_with_roots', 'grass_path', 'podzol', 'mycelium',
                  'farmland', 'rooted_dirt', 'mud', 'snow']:
        case(f'hoe_{block}', 'iron_hoe', block)
    case('hoe_dirt_side', 'iron_hoe', 'dirt', face=4)
    case('hoe_dirt_covered', 'iron_hoe', 'dirt', above='stone')
    case('hoe_dirt_under_grass', 'iron_hoe', 'dirt', above='short_grass')
    case('hoe_dirt_below', 'iron_hoe', 'dirt', floating=True)
    case('hoe_dirt_creative', 'iron_hoe', 'dirt', mode='creative')

    for block in ['grass_block', 'dirt', 'coarse_dirt', 'dirt_with_roots', 'podzol', 'mycelium', 'farmland',
                  'grass_path', 'mud', 'snow_layer', 'campfire', 'soul_campfire',
                  'campfire ["extinguished"=true]']:
        case(f'shovel_{block.split()[0]}{"_out" if "extinguished" in block else ""}', 'iron_shovel', block)
    case('shovel_dirt_side', 'iron_shovel', 'dirt', face=4)
    case('shovel_dirt_covered', 'iron_shovel', 'dirt', above='stone')
    case('shovel_dirt_under_grass', 'iron_shovel', 'dirt', above='short_grass')
    case('shovel_dirt_below', 'iron_shovel', 'dirt', floating=True)
    case('shovel_grass_creative', 'iron_shovel', 'grass_block', mode='creative')

    for block in ['oak_log ["pillar_axis"="x"]', 'oak_wood', 'stripped_oak_log', 'spruce_log', 'crimson_stem',
                  'warped_hyphae', 'bamboo_block', 'cherry_log', 'mangrove_wood', 'pale_oak_log',
                  'copper_block', 'exposed_copper', 'weathered_cut_copper_stairs ["weirdo_direction"=2]',
                  'oxidized_copper', 'waxed_copper', 'waxed_oxidized_cut_copper_slab',
                  'exposed_copper_bulb', 'weathered_copper_door', 'oxidized_copper_trapdoor', 'exposed_copper_grate',
                  'weathered_chiseled_copper', 'exposed_lightning_rod', 'waxed_exposed_copper_chest',
                  'oxidized_copper_lantern', 'stone', 'pumpkin']:
        name = block.split()[0]
        case(f'axe_{name}', 'iron_axe', block)
    case('axe_oak_log_side', 'iron_axe', 'oak_log', face=4)
    case('axe_oak_log_below', 'iron_axe', 'oak_log', floating=True)
    case('axe_oak_log_creative', 'iron_axe', 'oak_log', mode='creative')
    case('axe_exposed_copper_creative', 'iron_axe', 'exposed_copper', mode='creative')

    for block in ['copper_block', 'exposed_cut_copper_stairs', 'waxed_copper', 'oxidized_copper_bulb', 'stone']:
        case(f'honeycomb_{block}', 'honeycomb 4', block)
    case('honeycomb_copper_creative', 'honeycomb 4', 'copper_block', mode='creative')
    case('shears_pumpkin_west', 'shears', 'pumpkin', face=4)
    case('shears_pumpkin_top', 'shears', 'pumpkin', face=1)
    case('shears_carved_pumpkin', 'shears', 'carved_pumpkin', face=4)
    at('/fill 1 -60 -1 3 -56 1 air', 2)
    at('/gamemode survival @s', 3)

if 'wear' in parts:
    for kind, item, block in [('axe', 'netherite_axe', 'oak_log'), ('hoe', 'netherite_hoe', 'dirt'),
                              ('shovel', 'netherite_shovel', 'grass_block')]:
        for level, uses in [(0, args.uses // 10), (3, args.uses)]:
            at('/kill @e[type=item]', 2)
            fresh(item, level)
            at(f'/say tooluse wear {kind} {level}', 3)
            for _ in range(uses):
                at(f'/setblock {BLOCK} {block}', 2)
                at(f'use-on {BLOCK} 1', 2)
            at(f'/setblock {BLOCK} air', 5)
    sheep = 0
    for level, shorn in [(0, args.shears // 10), (3, args.shears)]:
        fresh('shears', level)
        at(f'/say tooluse wear shears {level}', 3)
        for _ in range(shorn):
            sheep += 1
            at(f'/summon sheep 2.5 -60 0.5 0 0 spawn_adult s{sheep}', 5)
            at(f'interact-named s{sheep}', 4)
            at('/kill @e[type=sheep]', 2)
            at('/kill @e[type=item]', 3)

if 'drops' in parts:
    # A husk (it does not burn by day) holding an iron spear, brought to 1
    # health and killed by the player's hit: whether the spear drops and
    # the wear it keeps (`AddItemActor`).
    at('/difficulty easy', 2)
    at('/effect @s resistance 100000 4 true', 2)
    fresh('netherite_sword')
    at('/say tooluse drops husk 0', 3)
    for husk in range(args.husks):
        at(f'/summon husk 2.5 -60 0.5 0 0 minecraft:spawn_adult h{husk}', 3)
        at('/replaceitem entity @e[type=husk] slot.weapon.mainhand 0 iron_spear', 2)
        at('/effect @e[type=husk] slowness 30 255 true', 2)
        at('/damage @e[type=husk] 19', 12)
        at(f'attack-named h{husk}', 30)
        at('/kill @e[type=husk]', 2)
        at('/kill @e[type=item]', 3)
    at('/difficulty peaceful', 2)

at('exit')
print('\n'.join(lines))
