//! Ported from elements/dsp/multistage_envelope.{h,cc}: float version of Peaks' multistage
//! envelope.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `EnvelopeShape` is an offset from `LUT_ENV_LINEAR` into the table of tables, as the C++ adds
//! it. A shape, level or time the C++ never sets keeps the zero of the firmware's static object.

use crate::cpp::at_usize;
use crate::resources::{LOOKUP_TABLE_TABLE, LUT_ENV_INCREMENTS};
use crate::stmlib::dsp::dsp::{cast_u16, cast_usize};

pub type EnvelopeShape = usize;
pub const ENV_SHAPE_LINEAR: EnvelopeShape = 0;
pub const ENV_SHAPE_EXPONENTIAL: EnvelopeShape = 1;
pub const ENV_SHAPE_QUARTIC: EnvelopeShape = 2;

pub const ENVELOPE_FLAG_RISING_EDGE: u8 = 1;
pub const ENVELOPE_FLAG_FALLING_EDGE: u8 = 2;
pub const ENVELOPE_FLAG_GATE: u8 = 4;

pub const K_MAX_NUM_SEGMENTS: usize = 6;

/// `LUT_ENV_LINEAR` in `resources.h`: the first of the shapes in `lookup_table_table`.
const LUT_ENV_LINEAR: usize = 10;

#[derive(Default)]
pub struct MultistageEnvelope {
    level_: [f32; K_MAX_NUM_SEGMENTS],
    time_: [f32; K_MAX_NUM_SEGMENTS],
    shape_: [EnvelopeShape; K_MAX_NUM_SEGMENTS],
    segment_: i16,
    start_value_: f32,
    value_: f32,
    phase_: f32,
    num_segments_: u16,
    sustain_point_: u16,
    loop_start_: u16,
    loop_end_: u16,
    hard_reset_: bool,
}

impl MultistageEnvelope {
    pub fn init(&mut self) {
        self.set_adsr(0.0, 0.25, 0.25, 0.5);
        self.segment_ = self.num_segments_ as i16;
        self.phase_ = 0.0;
        self.start_value_ = 0.0;
        self.value_ = 0.0;
        self.hard_reset_ = false;
    }

    #[inline]
    pub fn process(&mut self, flags: u8) -> f32 {
        if flags & ENVELOPE_FLAG_RISING_EDGE != 0 {
            self.start_value_ =
                if i32::from(self.segment_) == i32::from(self.num_segments_) || self.hard_reset_ {
                    self.level_[0]
                } else {
                    self.value_
                };
            self.segment_ = 0;
            self.phase_ = 0.0;
        } else if flags & ENVELOPE_FLAG_FALLING_EDGE != 0 && self.sustain_point_ != 0 {
            self.start_value_ = self.value_;
            self.segment_ = self.sustain_point_ as i16;
            self.phase_ = 0.0;
        } else if self.phase_ >= 1.0 {
            self.start_value_ = at_usize(&self.level_, self.segment() + 1);
            self.segment_ = self.segment_.wrapping_add(1);
            self.phase_ = 0.0;
            if i32::from(self.segment_) == i32::from(self.loop_end_) {
                self.segment_ = self.loop_start_ as i16;
            }
        }

        let done = i32::from(self.segment_) == i32::from(self.num_segments_);
        let sustained = self.sustain_point_ != 0
            && i32::from(self.segment_) == i32::from(self.sustain_point_)
            && flags & ENVELOPE_FLAG_GATE != 0;

        let mut phase_increment = 0.0;
        if !sustained && !done {
            phase_increment =
                Self::interpolate8(&LUT_ENV_INCREMENTS, at_usize(&self.time_, self.segment()));
        }
        let t = Self::interpolate8(
            LOOKUP_TABLE_TABLE[LUT_ENV_LINEAR + at_usize(&self.shape_, self.segment())],
            self.phase_,
        );
        self.phase_ += phase_increment;
        self.value_ = self.start_value_
            + (at_usize(&self.level_, self.segment() + 1) - self.start_value_) * t;
        self.value_
    }

    #[inline]
    pub fn process_buffer(&mut self, flags_in: &[u8], out: &mut [f32], size: usize) {
        for (flags, out) in flags_in[..size].iter().zip(&mut out[..size]) {
            *out = self.process(*flags);
        }
    }

    /// `segment_` as an index: the C++ indexes with the `int16_t`, which is never negative; a
    /// segment past the arrays reads zero ([`at_usize`]) where the C++ would read past them.
    #[inline]
    fn segment(&self) -> usize {
        self.segment_ as usize
    }

    #[inline]
    pub fn set_time(&mut self, segment: u16, time: f32) {
        self.time_[usize::from(segment)] = time;
    }

    #[inline]
    pub fn set_level(&mut self, segment: u16, level: f32) {
        self.level_[usize::from(segment)] = level;
    }

    #[inline]
    pub fn set_num_segments(&mut self, num_segments: u16) {
        self.num_segments_ = num_segments;
    }

    /// The C++ takes a `float` and stores it in the `uint16_t`.
    #[inline]
    pub fn set_sustain_point(&mut self, sustain_point: f32) {
        self.sustain_point_ = cast_u16(sustain_point);
    }

    #[inline]
    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.num_segments_ = 3;
        self.sustain_point_ = 2;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = release;

        self.shape_[0] = ENV_SHAPE_QUARTIC;
        self.shape_[1] = ENV_SHAPE_EXPONENTIAL;
        self.shape_[2] = ENV_SHAPE_EXPONENTIAL;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    #[inline]
    pub fn set_ad(&mut self, attack: f32, decay: f32) {
        self.num_segments_ = 2;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_EXPONENTIAL;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    /// As the C++ writes it, the third time is the sustain level.
    #[inline]
    pub fn set_adr(&mut self, attack: f32, decay: f32, sustain: f32, _release: f32) {
        self.num_segments_ = 3;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = sustain;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;
        self.shape_[2] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    #[inline]
    pub fn set_ar(&mut self, attack: f32, decay: f32) {
        self.num_segments_ = 2;
        self.sustain_point_ = 1;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    #[inline]
    pub fn set_adsar(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.num_segments_ = 4;
        self.sustain_point_ = 2;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 1.0;
        self.level_[4] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = attack;
        self.time_[3] = release;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;
        self.shape_[2] = ENV_SHAPE_LINEAR;
        self.shape_[3] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    #[inline]
    pub fn set_adar(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.num_segments_ = 4;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 1.0;
        self.level_[4] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = attack;
        self.time_[3] = release;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;
        self.shape_[2] = ENV_SHAPE_LINEAR;
        self.shape_[3] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 0;
    }

    #[inline]
    pub fn set_ad_loop(&mut self, attack: f32, decay: f32) {
        self.num_segments_ = 2;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 2;
    }

    #[inline]
    pub fn set_adr_loop(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.num_segments_ = 3;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = release;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;
        self.shape_[2] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 3;
    }

    #[inline]
    pub fn set_adar_loop(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.num_segments_ = 4;
        self.sustain_point_ = 0;

        self.level_[0] = 0.0;
        self.level_[1] = 1.0;
        self.level_[2] = sustain;
        self.level_[3] = 1.0;
        self.level_[4] = 0.0;

        self.time_[0] = attack;
        self.time_[1] = decay;
        self.time_[2] = attack;
        self.time_[3] = release;

        self.shape_[0] = ENV_SHAPE_LINEAR;
        self.shape_[1] = ENV_SHAPE_LINEAR;
        self.shape_[2] = ENV_SHAPE_LINEAR;
        self.shape_[3] = ENV_SHAPE_LINEAR;

        self.loop_start_ = 0;
        self.loop_end_ = 4;
    }

    #[inline]
    pub fn set_hard_reset(&mut self, hard_reset: bool) {
        self.hard_reset_ = hard_reset;
    }

    /// The envelope's value after its last `process`: for a host, not in the C++.
    #[inline]
    pub fn value(&self) -> f32 {
        self.value_
    }

    #[inline]
    fn interpolate8(table: &[f32], mut index: f32) -> f32 {
        index *= 256.0;
        let integral = cast_usize(index);
        // size_t to float rounds to nearest, as `as` does.
        let fractional = index - integral as f32;
        let a = at_usize(table, integral);
        let b = at_usize(table, integral.wrapping_add(1));
        a + (b - a) * fractional
    }
}
