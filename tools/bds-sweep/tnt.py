"""Round 4: TNT and explosion families for the `scenarios` job.

Explosions reach farther than a circuit cell, so these cells stand 16 blocks
apart with a 13 x 10 x 13 box refilled after each wave (`LAYOUT`); the
redstone witness gives each wave's update parity.

- `tnt_prime`: TNT primed by a redstone block (both parities) or a
  neighbouring explosion; the TNT block, the primed actor's path and the
  tick it explodes.
- `tnt_blast_<material>`: TNT in the middle of a 9 x 9 x 9 cube of one
  material, primed; after the explosion every cube position is read and the
  items left in the cell are counted. Many cells per material, compared by
  distribution.
- `tnt_actors`: armour stands and iron golems at distances 1-7 from a TNT,
  some behind a wall; their health and motion every tick (the golems
  wander: compare from their recorded positions).
- `tnt_hits`: iron golems and pigs held still with slowness, at known
  spots, alone and behind walls: damage and knockback.
"""

from __future__ import annotations

from circuits import RB, at, key, put, timeline, wait

LAYOUT = {'origin': [12, -60, 24], 'spacing': [16, 16], 'columns': 4, 'wave_size': 16, 'center': [0, 0],
          'clear_box': [[-6, -3, -6], [6, 6, 6]], 'witness': [2, -60, 18]}
PROPERTIES = {'view_distance': 10, 'tick_distance': 8}
TNT = 'minecraft:tnt'
BOX = {'box': [[-6, -3, -6], [6, 6, 6]]}
MATERIALS = ('dirt', 'oak_planks', 'cobblestone', 'glass')
SAMPLES = 64


def item(fam: str, name, build: list, ticks: int, events: list, watch: list, actors: bool = True,
         settle: int = 4) -> dict:
    steps = [['build', [[o, b, st, True] for o, b, st in build]], wait(settle), timeline(ticks, events, watch)]
    if actors:
        steps[-1].append(BOX)
    return {'key': key(fam, name), 'family': fam, 'steps': steps}


def tnt_prime() -> list[dict]:
    """TNT at the cell origin primed by a redstone block beside it set at
    t0 (both parities), eight cells each (the primed actor's first motion is
    random); two TNT side by side, the second primed by the first's
    explosion."""
    fam = 'tnt_prime'
    items = []
    watch = [at(0), at(1)]
    for t0 in (4, 5):
        for n in range(8):
            items.append(item(fam, ['redstone', t0, n], [(at(0), TNT, {'explode_bit': False})], 110,
                              [[t0, put(at(1), RB)]], watch))
    for n in range(8):
        build = [(at(0), TNT, {'explode_bit': False}), (at(-3), TNT, {'explode_bit': False})]
        items.append(item(fam, ['chain', n], build, 150, [[4, put(at(1), RB)]], watch + [at(-3)]))
    return items


def blast(material: str) -> list[dict]:
    """`SAMPLES` cells: a 9 x 9 x 9 cube of `material` centred one block
    above the floor (the floor's grass and dirt replaced), TNT in the
    middle, primed by a redstone block set in the cube's top face above a
    column of the material (the column carries no power: the block is set
    beside the TNT through a hole)."""
    fam = f'tnt_blast_{material}'
    items = []
    name = f'minecraft:{material}'
    cube = [['fill', at(-4, -3, -4), at(4, 5, 4), name], ['set', at(0, 1, 0), TNT, {'explode_bit': False}, False]]
    watch = [at(x, y, z) for x in range(-4, 5) for y in range(-3, 6) for z in range(-4, 5)]
    for n in range(SAMPLES):
        # The redstone block replaces the material beside the TNT.
        steps = [['build', []], *cube, wait(4),
                 timeline(110, [[4, put(at(1, 1, 0), RB)]], [at(0, 1, 0)]), ['read', 'after', watch]]
        items.append({'key': key(fam, n), 'family': fam, 'steps': steps})
    return items


def tnt_actors() -> list[dict]:
    """Armour stands and iron golems at distance d (1-7 blocks east of a
    TNT on the floor), alone and behind a one-block glass or stone wall;
    their health and motion every tick from priming to well after the
    explosion."""
    fam = 'tnt_actors'
    items = []
    for mob in ('armor_stand', 'iron_golem'):
        for d in range(1, 8):
            for wall in (None, 'glass', 'stone'):
                if wall and d < 3:
                    continue
                build = [(at(0), TNT, {'explode_bit': False})]
                if wall:
                    build += [(at(d - 1, y), f'minecraft:{wall}', {}) for y in range(0, 3)]
                events = [[1, ['cmd', f'summon minecraft:{mob} {{cx}} {{y}} {{cz}}', at(d)]],
                          [4, put(at(0, 0, -1), RB)]]
                items.append(item(fam, [mob, d, wall], build, 110, events, [at(0)]))
    return items


def tnt_hits() -> list[dict]:
    """Explosion damage and knockback on mobs held still with slowness:
    iron golems (100 health, no knockback) and pigs (knockback) at
    distances 1.5-7.5 east and on a diagonal, alone and behind a glass or
    stone wall half or fully covering them."""
    fam = 'tnt_hits'
    items = []
    spots = [(d, 0) for d in range(1, 8)] + [(d, d) for d in range(1, 5)] + [(d, 0, 'raised') for d in (2, 4)]
    for mob in ('iron_golem', 'pig'):
        for spot in spots:
            dx, dz = spot[0], spot[1]
            raised = len(spot) > 2
            for wall in (None, 'glass', 'stone', 'low_stone'):
                if wall and (dx < 3 or dz):
                    continue
                build = [(at(0), TNT, {'explode_bit': False})]
                if raised:
                    build.append((at(dx, 0, dz), 'minecraft:stone', {}))
                if wall == 'low_stone':
                    build.append((at(dx - 1, 0), 'minecraft:stone', {}))
                elif wall:
                    build += [(at(dx - 1, y), f'minecraft:{wall}', {}) for y in range(0, 4)]
                y = 1 if raised else 0
                events = [[1, ['cmd', f'summon minecraft:{mob} {{cx}} {{y}} {{cz}}', at(dx, y, dz)]],
                          [2, ['cmd', f'effect @e[type={mob},x={{cx}},y={{y}},z={{cz}},r=1] slowness 30 255 true',
                               at(dx, y, dz)]],
                          [4, put(at(0, 0, -1), RB)]]
                items.append(item(fam, [mob, list(spot), wall], build, 100, events, [at(0)]))
    return items


FAMILIES = {'tnt_prime': tnt_prime, 'tnt_actors': tnt_actors, 'tnt_hits': tnt_hits}
FAMILIES.update({f'tnt_blast_{m}': (lambda m=m: blast(m)) for m in MATERIALS})


# ---------------------------------------------------------------- round 2
# The timelines record every damage event, knockback and explosion in the
# cell's actor box (`damage_events`, `knockbacks`, `explosions`: the
# explosion's centre and the blocks BDS removes, in its order) and the
# scripted player when it stands in the box.

HERE = at(0)
UNDEAD = {'husk'}
NETHERITE = ('helmet', 'chestplate', 'leggings', 'boots')
ARMOR_SLOTS = ('head', 'chest', 'legs', 'feet')


def sel(mob: str) -> str:
    return f'@e[type={mob},x={{cx}},y={{y}},z={{cz}},r=1]'


def held(mob: str, offset, armour: int = 0) -> list:
    """Events summoning `mob` at `offset` at tick 1 and holding it there:
    Slowness 255, Health Boost XXV and a full heal (the undead heal with
    Instant Damage), and `armour` pieces of netherite (head first)."""
    heal = 'instant_damage' if mob in UNDEAD else 'instant_health'
    events = [[1, ['cmd', f'summon minecraft:{mob} {{cx}} {{y}} {{cz}}', offset]],
              [2, ['cmd', f'effect {sel(mob)} slowness 60 255 true', offset]],
              [2, ['cmd', f'effect {sel(mob)} health_boost 60 24 true', offset]],
              [3, ['cmd', f'effect {sel(mob)} {heal} 1 24 true', offset]]]
    for slot, piece in list(zip(ARMOR_SLOTS, NETHERITE))[:armour]:
        events.append([2, ['cmd', f'replaceitem entity {sel(mob)} slot.armor.{slot} 0 netherite_{piece}', offset]])
    return events


def tnt_knock() -> list[dict]:
    """Knockback and damage on mobs held still (twice each): pigs east at
    2-6 blocks, on the diagonal, raised one block on stone; husks at 3
    blocks with 0, 2 and 4 pieces of netherite armour (knockback
    resistance); an iron golem (full resistance)."""
    fam = 'tnt_knock'
    targets = [('pig', at(d), 0) for d in (2, 3, 4, 5, 6)] + [('pig', at(3, 0, 3), 0), ('pig', at(3, 1), 0)]
    targets += [('husk', at(3), n) for n in (0, 2, 4)] + [('iron_golem', at(3), 0)]
    items = []
    for mob, offset, armour in targets:
        for n in range(2):
            build = [(at(0), TNT, {'explode_bit': False})]
            if offset[1] == 1:
                build.append((at(offset[0], 0, offset[2]), 'minecraft:stone', {}))
            events = held(mob, offset, armour) + [[4, put(at(0, 0, -1), RB)]]
            row = item(fam, [mob, offset, armour, n], build, 110, events, [at(0)])
            if mob in UNDEAD:
                # Peaceful (the scenarios' difficulty) refuses monsters.
                row['steps'].insert(0, ['cmd', 'difficulty easy', HERE])
            items.append(row)
    return items


def player_events(offset, extra: list | None = None, yaw: int = 90) -> list:
    """The scripted player in survival at `offset` (facing west, towards
    the TNT) with Health Boost XXV and a full heal from tick 0; `player_end`
    puts it back in creative on the timeline's last tick."""
    return [[0, ['cmd', 'gamemode survival {player}', HERE]],
            [0, ['cmd', f'tp {{player}} {{cx}} {{y}} {{cz}} {yaw} 0', offset]],
            [1, ['cmd', 'effect {player} health_boost 60 24 true', HERE]],
            [2, ['cmd', 'effect {player} instant_health 1 24 true', HERE]]] + (extra or [])


def player_end(ticks: int) -> list:
    return [[ticks - 1, ['cmd', 'effect {player} clear', HERE]],
            [ticks - 1, ['cmd', 'gamemode creative {player}', HERE]]]


def tnt_player() -> list[dict]:
    """The scripted player (alone in its cell) east of a TNT at 2-6
    blocks at easy, and at 3 blocks at every difficulty: its health,
    position and knockback."""
    fam = 'tnt_player'
    items = []
    spots = [(d, 'easy') for d in (2, 3, 4, 6)] + [(3, level) for level in ('peaceful', 'normal', 'hard')]
    for d, level in spots:
        for n in range(2):
            build = [(at(0), TNT, {'explode_bit': False})]
            events = player_events(at(d)) + [[4, put(at(0, 0, -1), RB)]] + player_end(110)
            row = item(fam, [d, level, n], build, 110, events, [at(0)])
            row['steps'].insert(0, ['cmd', f'difficulty {level}', HERE])
            items.append({**row, 'solo': True})
    return items


def tnt_water() -> list[dict]:
    """TNT primed under water (a 9 x 4 x 9 pool of still sources): an iron
    golem and a pig held in the water, dirt and the floor around the TNT
    read every tick; four cells."""
    fam = 'tnt_water'
    items = []
    watch = [at(0, -1, 0), at(-2), at(0, 0, -3), at(2, 1, 0), at(0, 2, 0)]
    for n in range(4):
        events = held('iron_golem', at(3)) + held('pig', at(0, 0, 3)) + [[4, put(at(0, 0, -1), RB)]]
        steps = [['fill', at(-4, 0, -4), at(4, 3, 4), 'minecraft:water'],
                 ['build', [[at(0), TNT, {'explode_bit': False}, False], [at(-2), 'minecraft:dirt', {}, False],
                            [at(0, 0, -3), 'minecraft:dirt', {}, False]]],
                 wait(4), timeline(110, events, watch) + [BOX]]
        items.append({'key': key(fam, n), 'family': fam, 'steps': steps})
    return items


SLAB = ('minecraft:oak_slab', {'minecraft:vertical_half': 'bottom'})
SLAB_SPOTS = [at(-2, 1, 0), at(0, 1, 2), at(0, 1, -2), at(0, 3, 0), at(0, -1, 0), at(2, 1, 2)]
CUBE = [at(x, y, z) for x in range(-4, 5) for y in range(-3, 6) for z in range(-4, 5)]


def tnt_logged() -> list[dict]:
    """The blast cube of dirt (`blast`) with oak slabs at `SLAB_SPOTS`, dry
    or set into a still water source (waterlogged); 16 cells each."""
    fam = 'tnt_logged'
    items = []
    for logged in (False, True):
        sets = []
        for spot in SLAB_SPOTS:
            if logged:
                sets.append([spot, 'minecraft:water', {'liquid_depth': 0}, False])
            sets.append([spot, SLAB[0], SLAB[1], logged])
        sets.append([at(0, 1, 0), TNT, {'explode_bit': False}, False])
        for n in range(16):
            steps = [['fill', at(-4, -3, -4), at(4, 5, 4), 'minecraft:dirt'], ['build', sets], wait(4),
                     timeline(110, [[4, put(at(1, 1, 0), RB)]], [at(0, 1, 0)] + SLAB_SPOTS) + [BOX],
                     ['read', 'after', CUBE]]
            items.append({'key': key(fam, logged, n), 'family': fam, 'steps': steps})
    return items


def tnt_drops() -> list[dict]:
    """The blast cube of dirt or cobblestone with the item actors traced
    every tick (their stack sizes and where they appear); 16 cells each."""
    fam = 'tnt_drops'
    items = []
    for material in ('dirt', 'cobblestone'):
        cube = [['fill', at(-4, -3, -4), at(4, 5, 4), f'minecraft:{material}'],
                ['set', at(0, 1, 0), TNT, {'explode_bit': False}, False]]
        for n in range(16):
            steps = [*cube, wait(4), timeline(110, [[4, put(at(1, 1, 0), RB)]], [at(0, 1, 0)]) + [BOX],
                     ['read', 'after', CUBE]]
            items.append({'key': key(fam, material, n), 'family': fam, 'steps': steps})
    return items


def tnt_chain() -> list[dict]:
    """64 pairs: TNT at the cell origin primed by power and a second TNT 3
    blocks west primed by its explosion, caged in obsidian on every side but
    the first's (its push would carry it out of the actor box): the ticks
    between the two explosions."""
    fam = 'tnt_chain'
    items = []
    cage = [at(-4), at(-3, 1), at(-3, 0, -1), at(-3, 0, 1)]
    for n in range(64):
        build = [(at(0), TNT, {'explode_bit': False}), (at(-3), TNT, {'explode_bit': False})]
        build += [(p, 'minecraft:obsidian', {}) for p in cage]
        items.append(item(fam, [n], build, 150, [[4, put(at(1), RB)]], [at(0), at(-3)]))
    return items


def tnt_decay() -> list[dict]:
    """The dirt blast cube (`tnt_drops`) with the game rule
    `tntexplosiondropdecay` on: each broken block drops with a chance of
    one in the power; 16 cells."""
    fam = 'tnt_decay'
    items = []
    for n in range(16):
        steps = [['cmd', 'gamerule tntexplosiondropdecay true', HERE],
                 ['fill', at(-4, -3, -4), at(4, 5, 4), 'minecraft:dirt'],
                 ['set', at(0, 1, 0), TNT, {'explode_bit': False}, False], wait(4),
                 timeline(110, [[4, put(at(1, 1, 0), RB)]], [at(0, 1, 0)]) + [BOX],
                 ['read', 'after', CUBE], ['cmd', 'gamerule tntexplosiondropdecay false', HERE]]
        items.append({'key': key(fam, n), 'family': fam, 'steps': steps})
    return items


def tnt_lit() -> list[dict]:
    """The scripted player (survival, alone in its cell) lights a TNT with
    flint and steel or a fire charge from 2 blocks east: the damage events'
    attacker and cause on a held pig and on the player."""
    fam = 'tnt_lit'
    items = []
    for tool in ('flint_and_steel', 'fire_charge'):
        for n in range(2):
            build = [(at(0), TNT, {'explode_bit': False})]
            events = player_events(at(2), [
                [1, ['cmd', f'replaceitem entity {{player}} slot.hotbar 0 {tool} 1', HERE]],
                [12, ['use', at(0), 1, [500, 1000, 500]]]]) + held('pig', at(0, 0, 3)) + player_end(120)
            items.append({**item(fam, [tool, n], build, 120, events, [at(0)]), 'solo': True})
    return items


def tnt_anchor() -> list[dict]:
    """A charged respawn anchor clicked in the overworld (it explodes:
    power 5, fire, no source actor), at easy: dirt blocks 2 blocks out
    (their drops), the cells around read every tick (where fire is set),
    a held pig and the player 4 blocks away (the damage cause). Then
    glowstone (two in hand) clicked twice on an anchor of every charge, and
    a full anchor clicked at peaceful."""
    fam = 'tnt_anchor'
    items = []
    dirt = [at(x, 0, z) for x in (-2, 2) for z in (-2, -1, 0, 1, 2)]
    dirt += [at(x, 0, z) for x in (-1, 0, 1) for z in (-2, 2)]
    watch = [at(x, y, z) for x in range(-5, 6) for y in range(-2, 3) for z in range(-5, 6)]
    for n in range(12):
        build = [[at(0), 'minecraft:respawn_anchor', {'respawn_anchor_charge': 1}, True]]
        build += [[p, 'minecraft:dirt', {}, True] for p in dirt]
        events = player_events(at(4), [
            [1, ['cmd', 'replaceitem entity {player} slot.hotbar 0 stick 1', HERE]],
            [12, ['use', at(0), 1, [500, 1000, 500]]]]) + held('pig', at(0, 0, -4)) + player_end(60)
        steps = [['cmd', 'difficulty easy', HERE], ['build', build], wait(4),
                 timeline(60, events, watch) + [BOX]]
        items.append({'key': key(fam, n), 'family': fam, 'steps': steps, 'solo': True})
    for charge, level in [(c, 'easy') for c in range(5)] + [(4, 'peaceful')]:
        build = [[at(0), 'minecraft:respawn_anchor', {'respawn_anchor_charge': charge}, True]]
        events = player_events(at(4), [
            [1, ['cmd', 'replaceitem entity {player} slot.hotbar 0 glowstone 2', HERE]],
            [12, ['use', at(0), 1, [500, 1000, 500]]],
            [22, ['use', at(0), 1, [500, 1000, 500]]]]) + player_end(40)
        steps = [['cmd', f'difficulty {level}', HERE], ['build', build], wait(4),
                 timeline(40, events, [at(0)]) + [BOX]]
        items.append({'key': key(fam, 'charge', charge, level), 'family': fam, 'steps': steps, 'solo': True})
    return items


ROUND2 = {'tnt_knock': tnt_knock, 'tnt_player': tnt_player, 'tnt_water': tnt_water, 'tnt_logged': tnt_logged,
          'tnt_drops': tnt_drops, 'tnt_chain': tnt_chain, 'tnt_decay': tnt_decay, 'tnt_lit': tnt_lit, 'tnt_anchor': tnt_anchor}
FAMILIES.update(ROUND2)
