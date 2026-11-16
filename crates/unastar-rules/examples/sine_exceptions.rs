//! Writes (or checks) `data/sine-exceptions.bin`: the angles up to 256 radians
//! where the Windows C runtime's `f32` sine and cosine, which BDS 1.26.51
//! (Windows) moves players with, differ from libm's `f64` ones rounded to
//! `f32` (always by one ULP). [`unastar_rules::math::sin`] applies them, so
//! every platform gets the Windows runtime's values.
//!
//! ```sh
//! cargo run -p unastar-rules --example sine_exceptions            # write
//! cargo run -p unastar-rules --example sine_exceptions -- --check # compare every angle
//! ```
//!
//! Both run only on Windows, where `f32::sin` is the C runtime's. The file
//! is `u32` LE counts of the sine's and the cosine's exceptions, then each
//! exception as a LEB128 varint of `(bits - previous bits) << 1 | up`, in
//! increasing bits of the positive angle, the first from 0.

/// The positive `f32` angles the file covers: 0 to 256 radians (yaws to
/// about 14,600 degrees either way).
#[cfg(windows)]
const LAST: u32 = 0x4380_0000;

/// The C runtime's function and the rounded one.
#[cfg(windows)]
type Sine = (fn(f32) -> f32, fn(f32) -> f32);

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    use unastar_rules::math;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/sine-exceptions.bin");
    if std::env::args().any(|a| a == "--check") {
        let mut wrong = 0u64;
        for bits in 0..=LAST {
            let x = f32::from_bits(bits);
            for x in [x, -x] {
                if math::sin(x).to_bits() != x.sin().to_bits()
                    || math::cos(x).to_bits() != x.cos().to_bits()
                {
                    wrong += 1;
                }
            }
        }
        println!("{wrong} angles differ from the C runtime");
        return if wrong == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let mut counts = Vec::new();
    let mut body = Vec::new();
    let functions: [Sine; 2] = [(f32::sin, math::rounded_sin), (f32::cos, math::rounded_cos)];
    for (runtime, rounded) in functions {
        let (mut count, mut previous) = (0u32, 0u32);
        for bits in 0..=LAST {
            let x = f32::from_bits(bits);
            let (theirs, ours) = (runtime(x), rounded(x));
            if theirs.to_bits() == ours.to_bits() {
                continue;
            }
            let up = theirs > ours;
            let next = if up { ours.next_up() } else { ours.next_down() };
            assert_eq!(theirs.to_bits(), next.to_bits(), "{x}: more than one ULP");
            let mut value = u64::from(bits - previous) << 1 | u64::from(up);
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                if value == 0 {
                    body.push(byte);
                    break;
                }
                body.push(byte | 0x80);
            }
            previous = bits;
            count += 1;
        }
        counts.extend(count.to_le_bytes());
    }
    counts.extend(body);
    std::fs::write(&path, counts).expect("writing the exceptions");
    println!("wrote {}", path.display());
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("only the Windows C runtime's sine is BDS's; run this on Windows");
    std::process::ExitCode::FAILURE
}
