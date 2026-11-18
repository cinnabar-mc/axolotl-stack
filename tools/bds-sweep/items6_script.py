"""Print tools/bds-sweep/scripts/items-6-{food,shield,totem}.txt (one per
`--parts`): food effect chances, the totem
of undying and shields, captured on BDS 1.26.51 for `items6_table.py`:

    python tools/bds-sweep/items6_script.py --parts shield > tools/bds-sweep/scripts/items-6-shield.txt
    python tools/bds-sweep/script.py tools/bds-sweep/scripts/items-6-shield.txt --out DIR --timeout 2400

The player stands at (0.5, -60, 0.5) facing east on a stone floor (peaceful,
where every food can be eaten when full). Every section begins with a
`/say items6 SECTION NAME` marker.

- food: chicken, rotten flesh and poisonous potato eaten `--eats` times each
  (a fresh one put in hotbar slot 0 each time, effects cleared after); the
  `MobEffect` adds after each eat give the chance of its effect;
- shield: a shield in the off hand (or the main hand), a pig 2 blocks in
  front of (east) or behind (west) the player, and `/damage @s N
  entity_attack entity` the pig while the player sneaks (or stands), and
  some ticks after the sneak starts: the health, the shield's wear, sounds
  and actor events BDS sends;
- shield (easy difficulty, as mob damage is 0 in peaceful): a vindicator
  attacking a player blocking with a shield, and one not blocking;
- totem: a totem of undying in the off hand, the main hand or both, and a
  lethal `/damage`, a hit it survives anyway, a pop with speed and poison
  held; then `void` damage and `/kill` with a totem held and a totem in an
  unselected hotbar slot (the player dies there, so these come last)."""
import argparse
import sys

sys.stdout.reconfigure(newline='\n')
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--eats', type=int, default=150, help='eats of each chance food')
parser.add_argument('--parts', default='food,shield,totem')
args = parser.parse_args()
parts = args.parts.split(',')

lines = [
    '# Food chances, shields and totems on BDS 1.26.51; printed by tools/bds-sweep/items6_script.py (see there).',
    '# The player (feet (0.5,-60,0.5), facing east).',
]
tick = 20


def at(order, gap=1):
    global tick
    lines.append(f'{tick} {order}')
    tick += gap


for order in ['/gamerule sendcommandfeedback false', '/gamerule domobspawning false',
              '/gamerule dodaylightcycle false', '/gamerule randomtickspeed 0', '/gamerule naturalregeneration false',
              '/time set 6000', '/fill -4 -61 -4 4 -61 4 stone', '/fill -4 -60 -4 4 -55 4 air',
              '/gamemode survival @s', '/clear @s', '/tp @s 0.5 -60 0.5 -90 0']:
    at(order)
tick += 10

if 'food' in parts:
    for food in ['chicken', 'rotten_flesh', 'poisonous_potato']:
        at(f'/say items6 food {food}', 3)
        for _ in range(args.eats):
            at(f'/replaceitem entity @s slot.hotbar 0 {food}', 3)
            at('use', 36)
            at('/effect @s clear', 3)

SNEAK_ON = ['flags Startsneaking,Sneaking,Sneakdown', 'held-flags Sneaking,Sneakdown']
SNEAK_OFF = ['flags Stopsneaking', 'held-flags none']


def heal():
    at('/effect @s clear', 1)
    at('/effect @s instant_health 1 10 true', 4)
    at('/effect @s clear', 2)


if 'shield' in parts:
    # Mob damage is 0 in peaceful (the script's world): easy halves it, plus 1.
    at('/difficulty easy', 2)
    def shield_case(name, damage, side='front', sneak=True, hand='offhand', delay=10, cause='entity_attack'):
        global tick
        x = '2.5' if side == 'front' else '-1.5'
        at('/kill @e[type=pig]', 2)
        # Knockback from the hits that land moves the player: back to the
        # start, facing east.
        at('/tp @s 0.5 -60 0.5 -90 0', 2)
        at('/clear @s', 1)
        at('/replaceitem entity @s slot.weapon.offhand 0 air', 1)
        if hand == 'offhand':
            at('/replaceitem entity @s slot.weapon.offhand 0 shield', 2)
        else:
            at('/replaceitem entity @s slot.hotbar 0 shield', 2)
        at(f'/summon pig {x} -60 0.5 0 0 minecraft:spawn_adult shieldpig', 3)
        heal()
        at(f'/say items6 shield {name}', 3)
        if sneak:
            for order in SNEAK_ON:
                at(order, 1)
            # The damage lands `delay` inputs after the sneak started.
            tick += max(delay - 2, 0)
        at(f'/damage @s {damage} {cause} entity @e[type=pig,c=1]', 8)
        if sneak:
            for order in SNEAK_OFF:
                at(order, 1)
        at('/kill @e[type=pig]', 4)

    for damage in [1, 2, 3, 4, 5, 8, 12]:
        shield_case(f'front_{damage}', damage)
    shield_case('behind_4', 4, side='behind')
    shield_case('standing_4', 4, sneak=False)
    shield_case('mainhand_4', 4, hand='mainhand')
    shield_case('projectile_4', 4, cause='projectile')
    shield_case('fire_4', 4, cause='fire')
    for delay in [1, 2, 3, 4, 5, 6, 8]:
        shield_case(f'delay_{delay}', 4, delay=delay)
    # A vindicator (its iron axe) attacking a player blocking with a shield,
    # and one not blocking.
    for name, sneak in [('vindicator_blocking', True), ('vindicator_open', False)]:
        at('/kill @e[type=!player]', 2)
        at('/tp @s 0.5 -60 0.5 -90 0', 2)
        at('/clear @s', 1)
        at('/replaceitem entity @s slot.weapon.offhand 0 shield', 2)
        heal()
        at('/effect @s resistance 100 2 true', 2)
        at('/summon vindicator 2.5 -60 0.5 90 0 minecraft:spawn_adult axeman', 3)
        at(f'/say items6 shield {name}', 3)
        if sneak:
            for order in SNEAK_ON:
                at(order, 1)
        tick += 120
        if sneak:
            for order in SNEAK_OFF:
                at(order, 1)
        at('/kill @e[type=vindicator]', 4)
    at('/clear @s', 1)
    at('/replaceitem entity @s slot.weapon.offhand 0 air', 3)
    at('/difficulty peaceful', 2)

if 'totem' in parts:
    def totem_case(name, offhand, mainhand, damage='/damage @s 40 entity_attack'):
        at('/clear @s', 1)
        if name == 'hotbar_unselected':
            at('/replaceitem entity @s slot.hotbar 3 totem_of_undying', 2)
        at('/replaceitem entity @s slot.weapon.offhand 0 ' + ('totem_of_undying' if offhand else 'air'), 2)
        if mainhand:
            at('/replaceitem entity @s slot.hotbar 0 totem_of_undying', 2)
        if name != 'with_effects':
            heal()
        else:
            at('/effect @s instant_health 1 10 true', 4)
        at(f'/say items6 totem {name}', 3)
        at(damage, 30)

    totem_case('offhand', True, False)
    totem_case('mainhand', False, True)
    totem_case('both', True, True)
    totem_case('none_absorbed', True, False, '/damage @s 19 entity_attack')
    # Effects held before the totem (speed, poison): kept or cleared.
    at('/effect @s speed 60 0 true', 2)
    at('/effect @s poison 60 0 true', 2)
    totem_case('with_effects', True, False)
    totem_case('void', True, False, '/damage @s 40 void')
    tick += 60  # the client respawns by itself
    totem_case('kill', True, False, '/kill @s')
    tick += 60  # the client respawns by itself
    totem_case('hotbar_unselected', False, False)
    tick += 60

at('exit')
print('\n'.join(lines))
