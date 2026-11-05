//! Ported from stmlib/stmlib.h: the macros every file uses.
//!
//! Copyright 2012 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `CONSTRAIN` and `CLIP` are functions here, and `FourCC` a `const fn`. Not ported, having no
//! behaviour of their own: `DISALLOW_COPY_AND_ASSIGN` (a Rust type is not `Clone` unless it says
//! so), `JOIN`, `STATIC_ASSERT` (`const` assertions do it), `IN_RAM`, `UNROLL*`, and the `Word`
//! and `LongWord` unions, which reinterpret bytes (`to_le_bytes` and `from_le_bytes` do that
//! without `unsafe`).

/// `CONSTRAIN(var, min, max)`: `if (var < min) var = min; else if (var > max) var = max;`.
/// A NaN passes through unchanged, as in the C++.
#[inline]
pub fn constrain<T: PartialOrd + Copy>(var: &mut T, min: T, max: T) {
    if *var < min {
        *var = min;
    } else if *var > max {
        *var = max;
    }
}

/// `CLIP(x)`: `if (x < -32767) x = -32767; if (x > 32767) x = 32767;`.
#[inline]
pub fn clip(x: &mut i32) {
    if *x < -32767 {
        *x = -32767;
    }
    if *x > 32767 {
        *x = 32767;
    }
}

/// `FourCC<a, b, c, d>::value`: `(((((d << 8) | c) << 8) | b) << 8) | a`, in `uint32_t`.
pub const fn four_cc(a: u32, b: u32, c: u32, d: u32) -> u32 {
    (((((d << 8) | c) << 8) | b) << 8) | a
}
