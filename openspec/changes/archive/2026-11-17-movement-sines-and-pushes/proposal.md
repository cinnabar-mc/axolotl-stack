# Proposal: BDS's sines, a crouch under a ceiling, bubble columns and currents

## Why

Round 6 left 19 of the 226 corpus scenarios and 23 of 230 random seeds off BDS 1.26.51. Several
were the same few ULP from the trigonometry, and two were a sneak let go under a ceiling. Bubble
columns and water currents were trusted instead of simulated, so a client beside them could move
as it liked within a tick's reach. PvP and the anticheat need the server's copy to move as BDS's
does, there too.

Prior work:
- [decision 0067](../../../../docs/decisions/0067-find-movement-rules-from-random-seeds-and-probes.md)
  and `docs/solutions/tooling/find-movement-rules-from-random-seeds-then-probe-them.md`: the
  method this change follows (probes played on BDS, rules fitted offline in `f32`);
- [decision 0054](../../../../docs/decisions/0054-measure-bds-movement-through-a-far-reporting-client.md):
  how the harness reads BDS's own copy;
- `docs/quirks/bds.md`: the rows on movement rounding, sneaking and the glide heading this change
  extends.

## What Changes

- **Trigonometry** (`unastar_rules::math`, [decision 0068](../../../../docs/decisions/0068-take-the-windows-runtime-sine-on-every-platform.md)): the sines and cosines are the Windows C runtime's,
  which BDS runs on, on every platform: libm's sine rounded to `f32`, with the runtime's
  one-ULP exceptions shipped as data. The sine table's entry `i` is `sin(i / 10430.378)`. There
  is one table, shared by movement, the drop direction and the sweep client. The glide's look
  vector wraps the yaw into -180..180.
- **Sneaking under a ceiling:** a sneak let go where a standing box does not fit stays crouched
  and slow. A standing box less than 0.01 into a block is pushed out to its face.
- **Bubble columns:** simulated (a push for each column block touched, after the drag), and
  judged instead of trusted.
- **Water currents:** the average flow of the water blocks touched, times 0.014, before the move.
  Flowing water is judged instead of trusted; falling water and flowing lava stay trusted.
- **Rest after a teleport:** the input BDS rests still takes the pushes.
- **Water floor and glides:** a move in water that hits a floor stops the vertical velocity; a
  glide's gravity is rounded as `(0.75 cos² - 1) × -0.08`, and a glide that lands stops.
- **Corpus:** 353 scenarios (from 226): sine probes, glide pitches, bed heights, ceiling exits
  and pushes, currents, bubble columns, and four failing seeds of a fresh random batch.
- **Mutation check:** a known gap whose first difference a mutation brings forward also catches
  it.

## Impact

- Code: `crates/unastar-rules` (`math`, `movement`, build script, `data/sine-exceptions.bin`,
  the `sine_exceptions` example), `crates/bds-sweep` (sweep client, scenario rest),
  `crates/unastar` (the authority's rest and trusts).
- Spec: `server-authority`, the trusted situations (bubble columns and flowing water are judged).
- Records: quirk rows, the `player.movement` parity row, a solution note.
