//! Ported from stmlib/dsp/dsp.h: DSP utility routines; and the float-to-integer conversions as
//! MSVC x64 compiles them, which every ported file uses instead of a bare `as`.
//!
//! Copyright 2012 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The macros are functions: `MAKE_INTEGRAL_FRACTIONAL(x)` is [`make_integral_fractional`],
//! `ONE_POLE`, `SLOPE` and `SLEW` take the state by `&mut`. `Clip16`, `ClipU16` and `Sqrt` are
//! the `TEST` build's (the desktop build the reference is), not the ARM assembly.

use crate::cpp::at;

// ---------------------------------------------------------------------------------------------
// Float to integer, as MSVC x64 compiles `static_cast` (VS 2022 BuildTools, `/O2 /fp:precise`,
// checked by a probe compiled with it, 2026-11-04). Rust's `as` saturates and sends a NaN to
// zero; the C++ does neither, so a port never writes a bare `as` from a float to an integer.

/// `static_cast<int32_t>(float)` (also `int`): `cvttss2si`, truncation toward zero; a NaN or a
/// value outside the `int32_t` range gives `INT32_MIN`.
#[inline]
pub fn cast_i32(x: f32) -> i32 {
    if (-2_147_483_648.0..2_147_483_648.0).contains(&x) {
        x as i32
    } else {
        i32::MIN
    }
}

/// `static_cast<int32_t>(double)`: `cvttsd2si`, as [`cast_i32`].
#[inline]
pub fn cast_i32_f64(x: f64) -> i32 {
    if x > -2_147_483_649.0 && x < 2_147_483_648.0 {
        x as i32
    } else {
        i32::MIN
    }
}

/// `static_cast<int64_t>(float)`: the 64-bit `cvttss2si`; a NaN or a value outside the range
/// gives `INT64_MIN`.
#[inline]
pub fn cast_i64(x: f32) -> i64 {
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&x) {
        x as i64
    } else {
        i64::MIN
    }
}

/// `static_cast<int64_t>(double)`, as [`cast_i64`].
#[inline]
pub fn cast_i64_f64(x: f64) -> i64 {
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&x) {
        x as i64
    } else {
        i64::MIN
    }
}

/// `static_cast<uint32_t>(float)`: MSVC converts through the 64-bit `cvttss2si` and keeps the
/// low 32 bits, so a negative value wraps (−1.5 gives 4 294 967 295), a value past 2³² wraps
/// (5e9 gives 705 032 704), and a NaN or a value beyond ±2⁶³ gives 0.
#[inline]
pub fn cast_u32(x: f32) -> u32 {
    cast_i64(x) as u32
}

/// `static_cast<uint32_t>(double)`, as [`cast_u32`].
#[inline]
pub fn cast_u32_f64(x: f64) -> u32 {
    cast_i64_f64(x) as u32
}

/// A `float` stored in a `size_t` (a sample count): on x64 the 64-bit unsigned conversion,
/// which for a value in [0, 2⁶³) is the 64-bit `cvttss2si`, truncation toward zero. Elements
/// stores only such values; outside that range this keeps [`cast_i64`]'s 64 bits, which is not
/// measured as MSVC's.
#[inline]
pub fn cast_usize(x: f32) -> usize {
    cast_i64(x) as u64 as usize
}

/// `static_cast<int16_t>(float)`: through the 32-bit `cvttss2si`, keeping the low 16 bits
/// (70 000.7 gives 4 464; out of the `int32_t` range, 0).
#[inline]
pub fn cast_i16(x: f32) -> i16 {
    cast_i32(x) as i16
}

/// `static_cast<uint16_t>(float)`: as [`cast_i16`], unsigned.
#[inline]
pub fn cast_u16(x: f32) -> u16 {
    cast_i32(x) as u16
}

/// `static_cast<int8_t>(float)`: through the 32-bit `cvttss2si`, keeping the low 8 bits.
#[inline]
pub fn cast_i8(x: f32) -> i8 {
    cast_i32(x) as i8
}

/// `static_cast<uint8_t>(float)`: as [`cast_i8`], unsigned.
#[inline]
pub fn cast_u8(x: f32) -> u8 {
    cast_i32(x) as u8
}

// ---------------------------------------------------------------------------------------------
// dsp.h

/// `MAKE_INTEGRAL_FRACTIONAL(x)`: `x_integral = static_cast<int32_t>(x)` and
/// `x_fractional = x - static_cast<float>(x_integral)`, returned as `(integral, fractional)`.
#[inline]
pub fn make_integral_fractional(x: f32) -> (i32, f32) {
    let integral = cast_i32(x);
    // int32_t to float rounds to nearest, as `as` does.
    let fractional = x - integral as f32;
    (integral, fractional)
}

#[inline]
pub fn interpolate(table: &[f32], mut index: f32, size: f32) -> f32 {
    index *= size;
    let (index_integral, index_fractional) = make_integral_fractional(index);
    let a = at(table, index_integral);
    let b = at(table, index_integral.wrapping_add(1));
    a + (b - a) * index_fractional
}

#[inline]
pub fn interpolate_hermite(table: &[f32], mut index: f32, size: f32) -> f32 {
    index *= size;
    let (index_integral, index_fractional) = make_integral_fractional(index);
    let xm1 = at(table, index_integral.wrapping_sub(1));
    let x0 = at(table, index_integral);
    let x1 = at(table, index_integral.wrapping_add(1));
    let x2 = at(table, index_integral.wrapping_add(2));
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b_neg = w + a;
    let f = index_fractional;
    (((a * f) - b_neg) * f + c) * f + x0
}

#[inline]
pub fn interpolate_wrap(table: &[f32], mut index: f32, size: f32) -> f32 {
    index -= cast_i32(index) as f32;
    index *= size;
    let (index_integral, index_fractional) = make_integral_fractional(index);
    let a = at(table, index_integral);
    let b = at(table, index_integral.wrapping_add(1));
    a + (b - a) * index_fractional
}

#[inline]
pub fn smooth_step(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

/// `ONE_POLE(out, in, coefficient)`: `out += (coefficient) * ((in) - out);`.
#[inline]
pub fn one_pole(out: &mut f32, in_: f32, coefficient: f32) {
    *out += coefficient * (in_ - *out);
}

/// `SLOPE(out, in, positive, negative)`.
#[inline]
pub fn slope(out: &mut f32, in_: f32, positive: f32, negative: f32) {
    let error = in_ - *out;
    *out += (if error > 0.0 { positive } else { negative }) * error;
}

/// `SLEW(out, in, delta)`.
#[inline]
pub fn slew(out: &mut f32, in_: f32, delta: f32) {
    let mut error = in_ - *out;
    let d = delta;
    if error > d {
        error = d;
    } else if error < -d {
        error = -d;
    }
    *out += error;
}

#[inline]
pub fn crossfade(a: f32, b: f32, fade: f32) -> f32 {
    a + (b - a) * fade
}

#[inline]
pub fn soft_limit(x: f32) -> f32 {
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        soft_limit(x)
    }
}

/// The `TEST` build's `Clip16`.
#[inline]
pub fn clip16(x: i32) -> i32 {
    if x < -32768 {
        -32768
    } else if x > 32767 {
        32767
    } else {
        x
    }
}

/// The `TEST` build's `ClipU16`.
#[inline]
pub fn clip_u16(x: i32) -> u16 {
    if x < 0 {
        0
    } else if x > 65535 {
        65535
    } else {
        x as u16
    }
}

/// The `TEST` build's `Sqrt`, `sqrtf`: correctly rounded, as `f32::sqrt` is.
#[inline]
pub fn sqrt(x: f32) -> f32 {
    x.sqrt()
}

#[inline]
pub fn soft_convert(x: f32) -> i16 {
    clip16(cast_i32(soft_limit(x * 0.5) * 32768.0)) as i16
}
