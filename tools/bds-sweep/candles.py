"""Candle families for the `scenarios` sweep job (blocks lane, round 6).

`candles_lit`: the scripted player (creative) clicks the top of a candle or
candle cake with flint and steel or a fire charge, standing two blocks south
of it; the block is read every tick. Every candle colour and candle cake
with flint and steel, one to four candles, a fire charge, a candle that is
already lit and a waterlogged one. Each click runs alone (a `use` event);
run with `--client-capture` for the packets.
"""

from __future__ import annotations

from coral import HERE, cmd, key, set_

COLOURS = ('', 'white_', 'orange_', 'magenta_', 'light_blue_', 'yellow_', 'lime_', 'pink_', 'gray_',
           'light_gray_', 'cyan_', 'purple_', 'blue_', 'brown_', 'green_', 'red_', 'black_')
STAND = [0, 0, 2]


def click(block: str, states: dict, item: str, water: bool = False) -> list:
    """The steps: the block set at the cell origin (into a still water
    source when `water`: waterlogged), the player 2 blocks south holding
    `item` and looking at it, the top face clicked at tick 0."""
    first = [set_(HERE, 'minecraft:water', {'liquid_depth': 0}, False)] if water else []
    return first + [set_(HERE, block, states),
                    cmd(f'replaceitem entity {{player}} slot.hotbar 0 {item} 1'),
                    cmd('tp {player} {cx} {y} {cz} 180 45', STAND), ['wait', 10],
                    ['timeline', 20, 'trace', [[0, ['use', HERE, 1, [500, 1000, 500]]]], [HERE]]]


def candles_lit() -> list[dict]:
    fam = 'candles_lit'
    items = []
    for colour in COLOURS:
        for block in (f'minecraft:{colour}candle', f'minecraft:{colour}candle_cake'):
            items.append({'key': key(fam, block, 'flint_and_steel', 0), 'family': fam,
                          'steps': click(block, {}, 'flint_and_steel')})
    for count in (1, 2, 3):
        items.append({'key': key(fam, 'minecraft:candle', 'flint_and_steel', count), 'family': fam,
                      'steps': click('minecraft:candle', {'candles': count}, 'flint_and_steel')})
    for block in ('minecraft:candle', 'minecraft:candle_cake'):
        items.append({'key': key(fam, block, 'fire_charge', 0), 'family': fam,
                      'steps': click(block, {}, 'fire_charge')})
    items.append({'key': key(fam, 'minecraft:candle', 'flint_and_steel', 'lit'), 'family': fam,
                  'steps': click('minecraft:candle', {'lit': True}, 'flint_and_steel')})
    for item in ('flint_and_steel', 'fire_charge'):
        items.append({'key': key(fam, 'minecraft:candle', item, 'waterlogged'), 'family': fam,
                      'steps': click('minecraft:candle', {}, item, water=True)})
    return items


FAMILIES = {'candles_lit': candles_lit}
