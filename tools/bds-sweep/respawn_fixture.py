"""Distils a capture of `scripts/respawn.txt` into the committed respawn fixture.

    python tools/bds-sweep/respawn_fixture.py OUT.json DIR

DIR is a `script.py` run of `scripts/respawn.txt`. Each `/kill @s` of the
script is paired, in order, with the deaths in the capture; `SCENARIOS`
says what the script had set up by then (where the player died, its spawn
point, blocks put in the point's way, `doImmediateRespawn`), and the
capture gives the `Respawn` packets BDS sent: searching-for-spawn at the
death, then, once the client answered, searching-for-spawn and
ready-to-spawn. Needs `bds-sweep-dump` built (`cargo build -p bds-sweep`).
Raw captures stay outside the repository.
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
SCRIPT = REPO / 'tools' / 'bds-sweep' / 'scripts' / 'respawn.txt'
HOME = [3.5, -60.0, 2.5]

# Per `/kill @s` line: the player's feet when it died, its spawn point's
# block (`None`: the world spawn), stone put in that block and the one
# above, and `doImmediateRespawn`.
SCENARIOS = {
    40: {'feet': HOME, 'point': None, 'blocked': False, 'immediate': False},
    100: {'feet': HOME, 'point': [5, -60, -4], 'blocked': False, 'immediate': False},
    151: {'feet': [-6.5, -55.0, 6.5], 'point': [5, -60, -4], 'blocked': False, 'immediate': False},
    202: {'feet': HOME, 'point': [5, -60, -4], 'blocked': True, 'immediate': False},
    252: {'feet': HOME, 'point': None, 'blocked': False, 'immediate': False},
    302: {'feet': HOME, 'point': None, 'blocked': False, 'immediate': True},
}

LINE = re.compile(r'^(\d+) (\d+) (c2s|s2c) (\w+) (.*)$')
RESPAWN = re.compile(r'position: Vec3 \{ x: ([^,]+), y: ([^,]+), z: ([^ ]+) \}, state: (\w+)')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('out', type=Path)
    parser.add_argument('capture', type=Path)
    args = parser.parse_args()
    if not DUMP.exists():
        parser.error(f'{DUMP} missing: cargo build -p bds-sweep')
    out = subprocess.run([str(DUMP), str(args.capture / 'client-capture' / 'packets.jsonl'), '--input-tick'],
                         check=True, capture_output=True, text=True, encoding='utf-8').stdout
    rows = [m.groups() for m in map(LINE.match, out.splitlines()) if m]
    kills = [int(line.split()[0]) for line in SCRIPT.read_text(encoding='utf-8').splitlines()
             if line.split()[1:3] == ['/kill', '@s']]
    deaths = [i for i, (_, _, d, n, b) in enumerate(rows) if d == 's2c' and n == 'DeathInfoPacket']
    if len(kills) != len(deaths):
        raise SystemExit(f'{len(kills)} kills but {len(deaths)} deaths')
    cases = []
    for line, at in zip(kills, deaths):
        respawns = []
        for _, _, d, n, b in rows[at:]:
            if d == 's2c' and n == 'RespawnPacket':
                m = RESPAWN.search(b)
                respawns.append({'state': m[4], 'position': [float(m[1]), float(m[2]), float(m[3])]})
            if d == 's2c' and n == 'ActorEventPacket' and 'SpawnAlive' in b:
                break
        cases.append({'line': line, **SCENARIOS[line], 'respawns': respawns})
    fixture = {
        'source': 'BDS 1.26.51, plain, superflat, operator player: script.py capture of '
                  'tools/bds-sweep/scripts/respawn.txt (2026-11-18), distilled by tools/bds-sweep/respawn_fixture.py',
        'deaths': cases,
    }
    args.out.write_text(json.dumps(fixture, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(f'{len(cases)} deaths')
    return 0


if __name__ == '__main__':
    sys.exit(main())
