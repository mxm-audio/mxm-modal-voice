//! Ported from stmlib/dsp/units.{h,cc}: conversion from semitones to frequency ratio. The two
//! tables are generated into `super::units_tables` (`gen/tables.py`).
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).

use super::dsp::{cast_i32, make_integral_fractional};
use crate::cpp::at;

pub use super::units_tables::{LUT_PITCH_RATIO_HIGH, LUT_PITCH_RATIO_LOW};

#[inline]
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    let pitch = semitones + 128.0;
    let (pitch_integral, pitch_fractional) = make_integral_fractional(pitch);

    at(&LUT_PITCH_RATIO_HIGH, pitch_integral)
        * at(&LUT_PITCH_RATIO_LOW, cast_i32(pitch_fractional * 256.0))
}

#[inline]
pub fn semitones_to_ratio_safe(mut semitones: f32) -> f32 {
    let mut scale = 1.0f32;
    while semitones > 120.0 {
        semitones -= 120.0;
        scale *= 1024.0;
    }
    while semitones < -120.0 {
        semitones += 120.0;
        scale *= 1.0 / 1024.0;
    }
    scale * semitones_to_ratio(semitones)
}

#[inline]
pub fn exp2_safe(value: f32) -> f32 {
    semitones_to_ratio_safe(value * 12.0)
}
