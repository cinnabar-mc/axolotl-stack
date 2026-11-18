"""Mutation check of the movement differential: does the corpus catch a slightly wrong simulation?

    python tools/movement-diff/mutate.py [--only NAME ...] [--out mutations.json]

Each mutation changes `unastar_rules::movement` a little (a constant one f32
ULP off, two operations swapped, a threshold moved), rebuilds `movement-diff`
into its own target directory (`target/mutation`, so a BDS run's binaries are
never replaced), plays the corpus and compares every scenario with the
committed BDS traces. A mutation is caught when a scenario differs from BDS
earlier with it than without it: one that matches BDS without it, or a known
gap whose first difference it brings forward. The source file is always
restored.

The result (`--out`, default `tools/movement-diff/mutations.json`) lists, per
mutation, the scenarios that caught it; one caught by none is a blind spot of
the corpus.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
import compare  # noqa: E402

SOURCE = REPO / 'crates' / 'unastar-rules' / 'src' / 'movement' / 'mod.rs'
MATH = REPO / 'crates' / 'unastar-rules' / 'src' / 'math.rs'
BDS = REPO / 'crates' / 'bds-sweep' / 'testdata' / 'bds-1.26.51-movement-diff.json'
CORPUS = HERE / 'corpus.json'
TARGET = REPO / 'target' / 'mutation'

# (name, what it does, the exact text, its replacement[, the file, `SOURCE` if not given]).
# Each text must occur once.
MUTATIONS = [
    ('gravity_ulp', 'gravity 0.08 one ULP up', 'pub const GRAVITY: f32 = 0.08;', 'pub const GRAVITY: f32 = 0.080000006;'),
    ('vertical_drag_ulp', 'vertical drag 0.98 one ULP down', 'const VERTICAL_DRAG: f32 = 0.98;', 'const VERTICAL_DRAG: f32 = 0.97999996;'),
    ('air_friction_ulp', 'air friction 0.91 one ULP up', 'const AIR_FRICTION: f32 = 0.91;', 'const AIR_FRICTION: f32 = 0.91000006;'),
    ('ground_friction_ulp', 'the block friction one ULP up', 'friction *= ground.friction;',
     'friction *= f32::from_bits(ground.friction.to_bits() + 1);'),
    ('air_speed_ulp', 'air acceleration 0.02 one ULP up', 'const AIR_SPEED: f32 = 0.02;', 'const AIR_SPEED: f32 = 0.020000001;'),
    ('sprint_air_speed_ulp', 'sprinting air acceleration 0.025999999 one ULP up (to 0.026)',
     'const AIR_SPEED_SPRINTING: f32 = 0.025_999_999;', 'const AIR_SPEED_SPRINTING: f32 = 0.026;'),
    ('jump_ulp', 'jump 0.42 one ULP up', 'const JUMP_VELOCITY: f32 = 0.42;', 'const JUMP_VELOCITY: f32 = 0.42000002;'),
    ('jump_boost_ulp', 'jump boost step 0.1 one ULP up', 'const JUMP_BOOST_STEP: f32 = 0.1;', 'const JUMP_BOOST_STEP: f32 = 0.10000001;'),
    ('sprint_jump_ulp', 'sprint jump push 0.2 one ULP up', 'const SPRINT_JUMP: f32 = 0.2;', 'const SPRINT_JUMP: f32 = 0.20000002;'),
    ('impulse_ulp', 'move vector scale 0.98 one ULP down', 'const IMPULSE_SCALE: f32 = 0.98;', 'const IMPULSE_SCALE: f32 = 0.97999996;'),
    ('sneak_ulp', 'sneak factor 0.3 one ULP up', 'const SNEAK_IMPULSE: f32 = 0.3;', 'const SNEAK_IMPULSE: f32 = 0.30000004;'),
    ('acceleration_order', 'ground acceleration rounded as ((speed r) r) r',
     'speed * (ratio * ratio * ratio)', 'speed * ratio * ratio * ratio'),
    ('gravity_order', 'gravity and drag as v drag - g drag',
     'body.velocity[1] = (body.velocity[1] - gravity) * VERTICAL_DRAG;',
     'body.velocity[1] = body.velocity[1] * VERTICAL_DRAG - gravity * VERTICAL_DRAG;'),
    ('walk_table_sine', 'the walk turned by the sine table',
     'let (sin, cos) = (sin(yaw), cos(yaw));', 'let (sin, cos) = (table_sin(yaw), table_cos(yaw));'),
    ('sprint_jump_exact_sine', 'the sprint jump turned by the sine, not the table',
     'body.velocity[0] -= table_sin(yaw) * SPRINT_JUMP;', 'body.velocity[0] -= sin(yaw) * SPRINT_JUMP;'),
    ('runtime_sine', "the sine correctly rounded, without the C runtime's exceptions",
     'let value = adjust(SIN_EXCEPTIONS, a.to_bits(), rounded_sin(a));', 'let value = rounded_sin(a);', MATH),
    ('runtime_cosine', "the cosine correctly rounded, without the C runtime's exceptions",
     'adjust(COS_EXCEPTIONS, a.to_bits(), rounded_cos(a))', 'rounded_cos(a)', MATH),
    ('table_angle', 'sine table entry i at i x 2pi / 65536, not i / 10430.378',
     'sin((index & (SAMPLES - 1)) as f32 / INDEX_SCALE)',
     'sin((index & (SAMPLES - 1)) as f32 * std::f32::consts::PI * 2.0 / 65536.0)', MATH),
    ('glide_yaw_wrap', "the glide's look vector at the yaw as sent, not wrapped",
     'let heading = -controls.look_yaw() * DEGREES', 'let heading = -controls.yaw * DEGREES'),
    ('step_height', 'step height 0.5625 up to 0.6', 'pub const STEP_HEIGHT: f32 = 0.5625;', 'pub const STEP_HEIGHT: f32 = 0.6;'),
    ('slide_stop', 'a slide stops at twice f32::EPSILON', 'if velocity.abs() <= f32::EPSILON {',
     'if velocity.abs() <= 2.0 * f32::EPSILON {'),
    ('collision_epsilon', 'touching tolerance 1e-6 down to 1e-7', 'const EPSILON: f32 = 1.0e-6;', 'const EPSILON: f32 = 1.0e-7;'),
    ('sneak_edge_step', 'sneak edge probe step 0.05 down to 0.04', 'const STEP: f32 = 0.05;', 'const STEP: f32 = 0.04;'),
    ('water_drag_ulp', 'water drag 0.8 one ULP up', 'const WATER_DRAG: f32 = 0.8;', 'const WATER_DRAG: f32 = 0.80000007;'),
    ('water_sink_ulp', 'water sink 0.005 one ULP up', 'const WATER_SINK: f32 = 0.005;', 'const WATER_SINK: f32 = 0.0050000004;'),
    # One ULP of 0.085 or of 0.01 moves no product past an f32 rounding step at the speeds these
    # act on, so no observation can catch it; four and eight ULPs are the smallest that show.
    ('swim_rate', 'swim steering 0.085 four ULPs up', 'let rate = if look_y < -0.2 { 0.085 } else { 0.06 };',
     'let rate = if look_y < -0.2 { 0.08500003 } else { 0.06 };'),
    ('lava_drag', 'lava drag 0.5 one ULP up', 'const LAVA_DRAG: f32 = 0.5;', 'const LAVA_DRAG: f32 = 0.50000006;'),
    ('glide_drag', 'glide horizontal drag 0.99 one ULP down', '    v[0] *= 0.99;\n    v[1] *= 0.98;\n    v[2] *= 0.99;',
     '    v[0] *= 0.98999995;\n    v[1] *= 0.98;\n    v[2] *= 0.98999995;'),
    ('levitation_lift', 'levitation lift 0.01 one ULP up', 'const LEVITATION_LIFT: f32 = 0.01;', 'const LEVITATION_LIFT: f32 = 0.010000001;'),
    ('slow_falling_ulp', 'slow falling gravity 0.01 eight ULPs up', 'pub const SLOW_FALLING_GRAVITY: f32 = 0.01;',
     'pub const SLOW_FALLING_GRAVITY: f32 = 0.01000001;'),
    ('climb_speed', 'climb speed 0.2 one ULP up', 'const CLIMB_SPEED: f32 = 0.2;', 'const CLIMB_SPEED: f32 = 0.20000002;'),
    ('soul_sand', 'soul sand factor 1.225 one ULP up', 'const SOUL_SAND_ACCELERATION: f32 = 1.225;',
     'const SOUL_SAND_ACCELERATION: f32 = 1.2250001;'),
    ('honey_jump', 'honey jump 0.6 one ULP up', 'const HONEY_JUMP: f32 = 0.6;', 'const HONEY_JUMP: f32 = 0.6000001;'),
    ('slime_min_bounce', 'slowest bouncing fall 0.08000012 down to 0.0784', 'const SLIME_MIN_BOUNCE: f32 = 0.080_000_12;',
     'const SLIME_MIN_BOUNCE: f32 = 0.0784;'),
    ('cobweb_factor', 'cobweb horizontal factor 0.25 one ULP up', 'return Some([0.25, 0.05, 0.25]);',
     'return Some([0.25000003, 0.05, 0.25000003]);'),
    ('bed_bounce', 'bed bounce 0.66 one ULP up', 'BlockKind::Bed => (-0.66 * before[1]).min(1.0),',
     'BlockKind::Bed => (-0.66000005 * before[1]).min(1.0),'),
    ('powder_snow_factor', 'powder snow factor 0.9 one ULP up', 'return Some([0.9, 1.5, 0.9]);',
     'return Some([0.90000004, 1.5, 0.90000004]);'),
    ('sneak_release', 'a released sneak no longer slows its input',
     'body.sneak_slowed = body.sneaking || controls.sneak || body.crouched;',
     'body.sneak_slowed = controls.sneak || body.crouched;'),
    ('crouch_release_lag', 'the input that stands a crouched body up is not slowed',
     'body.sneak_slowed = body.sneaking || controls.sneak || body.crouched;',
     'body.sneak_slowed = body.sneaking || controls.sneak || crouched;'),
    ('stand_push', 'a body standing up is pushed out of blocks up to 0.011 deep, not 0.01',
     'const STAND_PUSH: f32 = 0.01;', 'const STAND_PUSH: f32 = 0.011;'),
    ('bubble_up_ulp', "an upward bubble column's push 0.06 one ULP up", 'const BUBBLE_UP: f32 = 0.06;',
     'const BUBBLE_UP: f32 = 0.060000002;'),
    ('bubble_top_ulp', "an upward bubble column's top push 0.1 one ULP up", 'const BUBBLE_UP_TOP: f32 = 0.1;',
     'const BUBBLE_UP_TOP: f32 = 0.10000001;'),
    # A downward column's push hits its -0.3 limit within a few ticks; one ULP of 0.03 rounds
    # away before then, four show.
    ('bubble_down_ulp', "a downward bubble column's push 0.03 four ULPs up", 'const BUBBLE_DOWN: f32 = 0.03;',
     'const BUBBLE_DOWN: f32 = 0.030000007;'),
    ('current_ulp', "a current's push 0.014 one ULP up", 'const CURRENT: f32 = 0.014;', 'const CURRENT: f32 = 0.014000001;'),
    ('sprint_tap', 'double-tap window 7 inputs down to 6', 'const SPRINT_TAP: u8 = 7;', 'const SPRINT_TAP: u8 = 6;'),
    ('sprint_tap_air', 'double tap also in the air', '&& !body.sprinting && body.on_ground {', '&& !body.sprinting {'),
    ('sprint_wall_angle', 'a wall ends a sprint from 40 degrees, not 45',
     'const SPRINT_WALL_ANGLE: f32 = std::f32::consts::FRAC_PI_4;', 'const SPRINT_WALL_ANGLE: f32 = 0.6981317;'),
    ('sprint_forward', 'a sprint lasts while the move vector is forward at all',
     'controls.forward >= SPRINT_START_FORWARD && !body.sneak_slowed && !body.sprint_blocked;',
     'controls.forward > 0.0 && !body.sneak_slowed && !body.sprint_blocked;'),
    ('honey_beside', 'the horizontal slowdown beside honey 0.4 one ULP up', 'const HONEY_BESIDE: f32 = 0.4;',
     'const HONEY_BESIDE: f32 = 0.40000004;'),
    ('web_touch', 'a web the box only touches holds it',
     'if any_block_inside(terrain, body, |b| b.kind == BlockKind::Cobweb) {',
     'if any_block(terrain, body, |b| b.kind == BlockKind::Cobweb) {'),
    ('network_offset', 'wire offset 1.62001 one ULP up', 'pub const NETWORK_OFFSET: f32 = 1.620_01;',
     'pub const NETWORK_OFFSET: f32 = 1.6200101;'),
]


def build_and_simulate(out: Path) -> dict:
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_BUILD_JOBS=os.environ.get('CARGO_BUILD_JOBS', '4'))
    subprocess.run(['cargo', 'build', '-q', '-p', 'bds-sweep', '--bin', 'movement-diff'], cwd=REPO, env=env, check=True)
    binary = TARGET / 'debug' / ('movement-diff.exe' if os.name == 'nt' else 'movement-diff')
    subprocess.run([str(binary), 'simulate', '--corpus', str(CORPUS), '--out', str(out)], cwd=REPO, check=True)
    return json.loads(out.read_text(encoding='utf-8'))['scenarios']


def first_differences(bds: dict, ours: dict) -> dict[str, float]:
    """The first tick each scenario differs from BDS's trace at (infinity when it matches)."""
    out = {}
    for name, trace in ours.items():
        if name in bds:
            found = compare.differences(trace['ticks'], bds[name]['ticks'])[0]
            out[name] = min((d.tick for d in found), default=float('inf'))
    return out


def caught_by(baseline: dict[str, float], mutated: dict[str, float]) -> list[str]:
    """The scenarios that differ from BDS earlier with the mutation than without it: those that
    match BDS without it, and the known gaps whose first difference the mutation brings forward."""
    return sorted(name for name, tick in mutated.items() if tick < baseline.get(name, float('inf')))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--only', nargs='*')
    parser.add_argument('--out', type=Path, default=HERE / 'mutations.json')
    args = parser.parse_args()
    bds = json.loads(BDS.read_text(encoding='utf-8'))['scenarios']
    scratch = TARGET / 'traces'
    scratch.mkdir(parents=True, exist_ok=True)
    originals = {path: path.read_bytes() for path in (SOURCE, MATH)}
    baseline = first_differences(bds, build_and_simulate(scratch / 'baseline.json'))
    results = []
    try:
        for name, what, old, new, *where in MUTATIONS:
            if args.only and name not in args.only:
                continue
            path = where[0] if where else SOURCE
            text = originals[path].decode('utf-8')
            if text.count(old) != 1:
                raise SystemExit(f'mutation {name}: its text occurs {text.count(old)} times')
            path.write_bytes(text.replace(old, new).encode('utf-8'))
            try:
                caught = caught_by(baseline, first_differences(bds, build_and_simulate(scratch / f'{name}.json')))
            finally:
                path.write_bytes(originals[path])
            results.append({'mutation': name, 'change': what, 'caught_by': caught})
            print(f'{name}: caught by {len(caught)} scenario(s)' if caught else f'{name}: MISSED', flush=True)
    finally:
        for path, original in originals.items():
            path.write_bytes(original)
    summary = {
        'baseline_matching': sum(1 for tick in baseline.values() if tick == float('inf')),
        'mutations': len(results),
        'caught': sum(1 for r in results if r['caught_by']),
        'results': results,
    }
    args.out.write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f"{summary['caught']} of {summary['mutations']} mutations caught; output {args.out}")
    return 0


if __name__ == '__main__':
    sys.exit(main())
