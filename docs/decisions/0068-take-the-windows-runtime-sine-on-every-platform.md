---
status: proposed
date: 2026-11-16
decision-makers: [project owner]
---

# Take the Windows C runtime's sine on every platform, as rounded sines plus shipped exceptions

## Context and Problem Statement

BDS 1.26.51 runs on Windows and moves players with the Windows C runtime's `f32` sine and cosine.
On about one angle in 15,000 these are one ULP off the correctly rounded value. Sprint jumps and
walks probed at those angles took the runtime's value every time (movement round 7, the `sinx_*`
scenarios). A walk, a sprint jump or a glide that reads a different sine drifts from BDS's copy,
and the drift grows from tick to tick. unastar runs on Linux too. Rust's `f32::sin` there is
another libm, with other off-by-one angles.

## Decision Drivers

- Copy BDS exactly ([0013](0013-copy-bds-behaviour-exactly.md)); the server's copy must move the
  same on every platform it runs on.
- Black-box only: the values come from measurements and from the runtime itself, never from BDS's
  code.
- Small, reviewable data; no work per tick beyond a sine.

## Considered Options

1. Call `f32::sin`/`f32::cos`: BDS's values on Windows, other values elsewhere.
2. Ship every angle's value: about 4 GB per function for all `f32`s, gigabytes even for one turn.
3. Compute libm's `f64` sine rounded to `f32` (the correctly rounded value, the same on every
   platform), and ship only the angles where the Windows runtime differs, with the direction of
   its one-ULP step.
4. Implement a sine polynomial that reproduces the runtime's rounding: not derivable black-box
   with confidence, and any mismatch would be silent.

## Decision Outcome

Chosen: option 3. `unastar_rules::math::{sin, cos}` look the angle's bits up in the exceptions
(`crates/unastar-rules/data/sine-exceptions.bin`, delta-encoded, 340 KB, angles up to 256
radians). The `sine_exceptions` example writes the file on Windows and checks every angle
against the runtime. The 65536-entry movement table is computed from the same sine
(`sin(i / 10430.378)`, fitted on 152 entries read from BDS).

### Consequences

- Good: Windows and Linux servers judge players identically, and identically to BDS. A
  runtime update would show up in the `--check` run, not in players' movement.
- Good: one sine for movement, the drop direction and the sweep client.
- Bad: angles past 256 radians (yaws past about 14,600 degrees) take the rounded value. A client
  can send such a yaw, but vanilla clients wrap long before.
- Bad: the exceptions can only be regenerated on Windows.

### Confirmation

`crates/unastar-rules/src/math.rs` tests (`sine_is_the_windows_runtime_sine` on Windows),
`cargo run -p unastar-rules --example sine_exceptions -- --check`, and the `sinx_*`/`sinh_*`
scenarios in `crates/bds-sweep/tests/movement_diff.rs`.
