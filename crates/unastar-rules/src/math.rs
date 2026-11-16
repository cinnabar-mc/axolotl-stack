//! Scalar Bedrock runtime math, independent of ECS and vector libraries.
//!
//! BDS 1.26.51 (Windows) takes its sines and cosines from the Windows C
//! runtime's `f32` functions, which differ from the correctly rounded ones
//! by one ULP on about one angle in 15,000. [`sin`] and [`cos`] give the C
//! runtime's values on every platform: libm's `f64` sine rounded to `f32`
//! ([`rounded_sin`]), with the one-ULP exceptions up to 256 radians
//! (`data/sine-exceptions.bin`, written by the `sine_exceptions` example
//! on Windows) applied.
//!
//! Bedrock also reads a 65536-entry sine table ([`table_sin`]): entry `i`
//! is `sin(i / 10430.378)` in `f32` (the lookup's own scale inverted, not
//! `i × 2pi / 65536`), indexed by the angle times 10430.378, truncated.
//!
//! Measured on BDS 1.26.51 (`tools/movement-diff`, the `sin_*` probes):
//! sprint jumps from rest at 160 yaws read 152 table entries, all equal to
//! `sin(i / 10430.378)` and 18 of them an angle ULP off `sin(i × 2pi /
//! 65536)`; 24 sprint jumps reading entries where the C runtime's sine is
//! an ULP off the correctly rounded one, and 23 walks at yaws where its
//! own is, take the C runtime's value every time.

/// The C runtime's exceptions: the bits of the positive angle, with bit 31
/// set where its value is one ULP above the rounded one (else below),
/// sorted (`build.rs` decodes `data/sine-exceptions.bin`).
const SIN_EXCEPTIONS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sin_exceptions.bin"));
const COS_EXCEPTIONS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cos_exceptions.bin"));
/// The largest angle the exceptions cover: 256 radians (yaws to about
/// 14,600 degrees either way).
const LAST_COVERED: u32 = 0x4380_0000;
const UP: u32 = 1 << 31;

/// The sine table's size and its lookup scale (`65536 / 2pi` in `f32`,
/// 10430.378).
const SAMPLES: i32 = 1 << 16;
const INDEX_SCALE: f32 = SAMPLES as f32 / std::f32::consts::TAU;

/// libm's `f64` sine of `x`, rounded to `f32`: the same on every platform.
pub fn rounded_sin(x: f32) -> f32 {
    libm::sin(f64::from(x)) as f32
}

/// libm's `f64` cosine of `x`, rounded to `f32`.
pub fn rounded_cos(x: f32) -> f32 {
    libm::cos(f64::from(x)) as f32
}

/// Whether the C runtime's value at the positive angle `bits` is one ULP
/// above (`Some(true)`) or below (`Some(false)`) the rounded one.
fn exception(list: &[u8], bits: u32) -> Option<bool> {
    let entry = |i: usize| {
        let at = i * 4;
        u32::from_le_bytes([list[at], list[at + 1], list[at + 2], list[at + 3]])
    };
    let (mut low, mut high) = (0, list.len() / 4);
    while low < high {
        let middle = (low + high) / 2;
        let value = entry(middle);
        match (value & !UP).cmp(&bits) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return Some(value & UP != 0),
        }
    }
    None
}

/// `value` moved one ULP up or down where the C runtime's differs at the
/// positive angle `bits`.
fn adjust(list: &[u8], bits: u32, value: f32) -> f32 {
    if bits > LAST_COVERED {
        // Beyond 256 radians the exceptions are not listed: the rounded value.
        return value;
    }
    match exception(list, bits) {
        Some(true) => value.next_up(),
        Some(false) => value.next_down(),
        None => value,
    }
}

/// The sine BDS 1.26.51 computes for `x` radians: the Windows C runtime's
/// `f32` sine (odd, so taken at `|x|`).
pub fn sin(x: f32) -> f32 {
    let a = x.abs();
    let value = adjust(SIN_EXCEPTIONS, a.to_bits(), rounded_sin(a));
    if x.is_sign_negative() { -value } else { value }
}

/// The cosine BDS 1.26.51 computes for `x` radians: the Windows C
/// runtime's `f32` cosine (even).
pub fn cos(x: f32) -> f32 {
    let a = x.abs();
    adjust(COS_EXCEPTIONS, a.to_bits(), rounded_cos(a))
}

/// The sine table's entry `index` (wrapped to the table).
fn table_entry(index: i32) -> f32 {
    sin((index & (SAMPLES - 1)) as f32 / INDEX_SCALE)
}

/// `sin` from Bedrock's sine table: entry `(radians × 10430.378) as i32`.
pub fn table_sin(radians: f32) -> f32 {
    table_entry((radians * INDEX_SCALE) as i32)
}

/// `cos` from the sine table: a quarter turn (16384 entries) on.
pub fn table_cos(radians: f32) -> f32 {
    table_entry((radians * INDEX_SCALE + (SAMPLES / 4) as f32) as i32)
}

/// Direction used by native player Drop, from finite yaw/pitch in degrees.
///
/// Yaw zero points along +Z; positive pitch points down. The sine table's
/// entries invert the rounded `f32` index scale before the sine; neither
/// `f64` sampling nor an algebraic TAU multiplication preserves the
/// captured negative-angle impulses. These stationary-drop captures do not
/// calibrate other actor look-vector paths.
#[inline]
pub fn player_throw_direction(yaw: f32, pitch: f32) -> [f32; 3] {
    let (yaw, pitch) = (yaw.to_radians(), pitch.to_radians());
    let cos_pitch = table_cos(pitch);
    [
        -table_sin(yaw) * cos_pitch,
        -table_sin(pitch),
        table_cos(yaw) * cos_pitch,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table entries the BDS probes read (`sin_*` in the movement
    /// corpus), including entries where the correctly rounded sine of
    /// `i × 2pi / 65536` is an ULP off.
    #[test]
    fn table_entries_invert_the_lookup_scale() {
        // Entry 27306 (cos at yaw 60): sin(27306 / 10430.378).
        assert_eq!(table_cos(60.0f32.to_radians()).to_bits(), 0x3f00_039e);
        assert_eq!(table_sin(0.0), 0.0);
        // Entry 32768 is sin(pi in f32), not 0.
        assert_eq!(table_entry(32768).to_bits(), 0xb3bb_bd2e);
    }

    #[test]
    fn exceptions_are_sorted_and_covered() {
        for list in [SIN_EXCEPTIONS, COS_EXCEPTIONS] {
            let bits: Vec<u32> = list
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) & !UP)
                .collect();
            assert!(bits.windows(2).all(|w| w[0] < w[1]));
            assert!(bits.last().is_some_and(|&b| b <= LAST_COVERED));
        }
    }

    /// On Windows the C runtime is at hand: the listed exceptions and a
    /// sample of the rest of the turn give its values exactly (the
    /// `sine_exceptions` example checks every angle).
    #[cfg(windows)]
    #[test]
    fn sine_is_the_windows_runtime_sine() {
        for bits in (0..=LAST_COVERED).step_by(99_991) {
            let x = f32::from_bits(bits);
            for x in [x, -x] {
                assert_eq!(sin(x).to_bits(), x.sin().to_bits(), "sin {x}");
                assert_eq!(cos(x).to_bits(), x.cos().to_bits(), "cos {x}");
            }
        }
        for list in [SIN_EXCEPTIONS, COS_EXCEPTIONS] {
            for c in list.chunks_exact(4) {
                let x = f32::from_bits(u32::from_le_bytes([c[0], c[1], c[2], c[3]]) & !UP);
                assert_eq!(sin(x).to_bits(), x.sin().to_bits(), "sin {x}");
                assert_eq!(cos(x).to_bits(), x.cos().to_bits(), "cos {x}");
            }
        }
    }
}
