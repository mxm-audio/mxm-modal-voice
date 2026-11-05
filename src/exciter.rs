//! Ported from elements/dsp/exciter.{h,cc}: the exciter, bow, blow and strike alike.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ calls through a table of member pointers indexed by the model; here a `match` on it.
//! `Random::GetWord` (the module's one static generator) is the voice's [`Random`], passed in.
//! `phase_` and `particle_range_`, which `Init` never sets, start at the static object's zero.

use crate::cpp::{at, at_usize};
use crate::dsp::K_SAMPLE_RATE;
use crate::resources::{
    LUT_APPROX_SVF_G, LUT_APPROX_SVF_GAIN, LUT_APPROX_SVF_H, LUT_APPROX_SVF_R, SMP_BOUNDARIES,
    SMP_NOISE_SAMPLE, SMP_SAMPLE_DATA,
};
use crate::stmlib::dsp::dsp::{cast_u32, cast_usize, make_integral_fractional};
use crate::stmlib::dsp::filter::{FILTER_MODE_LOW_PASS, Svf};
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::utils::random::Random;

/// `ExciterModel`, an `int` as the C++ enum is.
pub type ExciterModel = i32;
pub const EXCITER_MODEL_GRANULAR_SAMPLE_PLAYER: ExciterModel = 0;
pub const EXCITER_MODEL_SAMPLE_PLAYER: ExciterModel = 1;
pub const EXCITER_MODEL_MALLET: ExciterModel = 2;
pub const EXCITER_MODEL_PLECTRUM: ExciterModel = 3;
pub const EXCITER_MODEL_PARTICLES: ExciterModel = 4;
pub const EXCITER_MODEL_FLOW: ExciterModel = 5;
pub const EXCITER_MODEL_NOISE: ExciterModel = 6;

pub const EXCITER_FLAG_RISING_EDGE: u8 = 1;
pub const EXCITER_FLAG_FALLING_EDGE: u8 = 2;
pub const EXCITER_FLAG_GATE: u8 = 4;

#[derive(Default)]
pub struct Exciter {
    model_: ExciterModel,
    parameter_: f32,
    timbre_: f32,
    lp_: Svf,
    damp_state_: f32,
    particle_state_: f32,
    particle_range_: f32,
    damping_: f32,
    signature_: f32,
    phase_: u32,
    delay_: u32,
    plectrum_delay_: u32,
}

impl Exciter {
    pub fn init(&mut self) {
        self.set_model(EXCITER_MODEL_MALLET);
        self.set_parameter(0.0);
        self.set_timbre(0.99);
        self.lp_.init();
        self.damp_state_ = 0.0;
        self.delay_ = 0;
        self.plectrum_delay_ = 0;
        self.particle_state_ = 0.5;
        self.damping_ = 0.0;
        self.signature_ = 0.0;
    }

    #[inline]
    pub fn set_signature(&mut self, signature: f32) {
        self.signature_ = signature;
    }

    #[inline]
    pub fn set_model(&mut self, model: ExciterModel) {
        self.model_ = model;
    }

    #[inline]
    pub fn set_parameter(&mut self, parameter: f32) {
        self.parameter_ = parameter;
    }

    #[inline]
    pub fn set_timbre(&mut self, timbre: f32) {
        self.timbre_ = timbre;
    }

    #[inline]
    pub fn set_meta(&mut self, mut meta: f32, first: ExciterModel, last: ExciterModel) {
        meta *= (last - first + 1) as f32;
        let (meta_integral, meta_fractional) = make_integral_fractional(meta);
        self.model_ = first.wrapping_add(meta_integral);
        self.parameter_ = meta_fractional;
        if self.model_ > EXCITER_MODEL_NOISE {
            self.model_ = EXCITER_MODEL_NOISE;
        }
    }

    #[inline]
    pub fn damping(&self) -> f32 {
        self.damping_
    }

    #[inline]
    pub fn filter(&self) -> &Svf {
        &self.lp_
    }

    pub fn process(&mut self, flags: u8, out: &mut [f32], size: usize, random: &mut Random) {
        self.damping_ = 0.0;
        match self.model_ {
            EXCITER_MODEL_GRANULAR_SAMPLE_PLAYER => {
                self.process_granular_sample_player(flags, out, size, random)
            }
            EXCITER_MODEL_SAMPLE_PLAYER => self.process_sample_player(flags, out, size),
            EXCITER_MODEL_MALLET => self.process_mallet(flags, out, size),
            EXCITER_MODEL_PLECTRUM => self.process_plectrum(flags, out, size),
            EXCITER_MODEL_PARTICLES => self.process_particles(flags, out, size, random),
            EXCITER_MODEL_FLOW => self.process_flow(flags, out, size, random),
            EXCITER_MODEL_NOISE => self.process_noise(flags, out, size, random),
            // set_model and set_meta keep the model in the table's range.
            _ => {}
        }
        // Apply filters.
        if self.model_ != EXCITER_MODEL_GRANULAR_SAMPLE_PLAYER
            && self.model_ != EXCITER_MODEL_SAMPLE_PLAYER
        {
            let cutoff_index = cast_u32(self.timbre_ * 256.0) as usize;
            if self.model_ == EXCITER_MODEL_NOISE {
                let resonance_index = cast_u32(self.parameter_ * 256.0) as usize;
                self.lp_.set_g_r(
                    at_usize(&LUT_APPROX_SVF_G, cutoff_index),
                    at_usize(&LUT_APPROX_SVF_R, resonance_index),
                );
            } else {
                self.lp_.set_g_r_h(
                    at_usize(&LUT_APPROX_SVF_G, cutoff_index),
                    2.0,
                    at_usize(&LUT_APPROX_SVF_H, cutoff_index),
                );
            }
            self.lp_
                .process_buffer_in_place::<FILTER_MODE_LOW_PASS>(out, size);
        }
    }

    #[inline]
    fn get_pulse_amplitude(cutoff: f32) -> f32 {
        let cutoff_index = cast_u32(cutoff * 256.0);
        at_usize(&LUT_APPROX_SVF_GAIN, cutoff_index as usize)
    }

    /// `RandomSample()`: `static_cast<float>(Random::GetWord()) / 4294967296.0f`.
    #[inline]
    fn random_sample(random: &mut Random) -> f32 {
        random.get_word() as f32 / 4294967296.0
    }

    pub fn process_granular_sample_player(
        &mut self,
        _flags: u8,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let restart_prob = cast_u32(0.01 * 4294967296.0);
        let restart_point = cast_u32(self.parameter_ * 32767.0) << 17;
        let phase_increment = cast_u32(131072.0 * semitones_to_ratio(72.0 * self.timbre_ - 60.0));
        let base = cast_usize(self.signature_ * 8192.0);

        let mut phase = self.phase_;
        for out in &mut out[..size] {
            let phase_integral = (phase >> 17) as usize;
            let phase_fractional = (phase & 0x1ffff) as f32 / 131072.0;
            let a = f32::from(at_usize(&SMP_NOISE_SAMPLE, base + phase_integral));
            let b = f32::from(at_usize(&SMP_NOISE_SAMPLE, base + phase_integral + 1));
            *out = (a + (b - a) * phase_fractional) / 32768.0;
            phase = phase.wrapping_add(phase_increment);
            if random.get_word() < restart_prob {
                phase = restart_point;
            }
        }
        self.phase_ = phase;
        self.damping_ = 0.0;
    }

    pub fn process_sample_player(&mut self, flags: u8, out: &mut [f32], size: usize) {
        let index = (1.0 - self.parameter_) * 8.0;
        let (mut index_integral, mut index_fractional) = make_integral_fractional(index);
        if index_integral == 8 {
            index_integral = 7;
            index_fractional = 1.0;
        }
        // `smp_boundaries` is `size_t`; the C++ keeps each in a `uint32_t`.
        let boundary = |i: i32| at(&SMP_BOUNDARIES, i) as u32;
        let offset_1 = boundary(index_integral);
        let offset_2 = boundary(index_integral.wrapping_add(1));
        let length_1 = offset_2.wrapping_sub(offset_1).wrapping_sub(1);
        let length_2 = boundary(index_integral.wrapping_add(2))
            .wrapping_sub(offset_2)
            .wrapping_sub(1);
        let phase_increment =
            cast_u32(65536.0 * semitones_to_ratio(72.0 * self.timbre_ - 36.0 + 7.0));

        let mut damp = self.damp_state_;
        let mut phase = self.phase_;
        if flags & EXCITER_FLAG_RISING_EDGE != 0 {
            damp = 0.0;
            phase = 0;
        }
        if flags & EXCITER_FLAG_GATE == 0 {
            damp = 1.0 - 0.95 * (1.0 - damp);
        }
        for out in &mut out[..size] {
            let phase_integral = phase >> 16;
            let phase_fractional = (phase & 0xffff) as f32 / 65536.0;
            let mut sample_1 = 0.0;
            let mut sample_2 = 0.0;
            let mut step = false;
            if phase_integral < length_1 {
                let base = offset_1.wrapping_add(phase_integral) as usize;
                let a = f32::from(at_usize(&SMP_SAMPLE_DATA, base));
                let b = f32::from(at_usize(&SMP_SAMPLE_DATA, base + 1));
                sample_1 = a + (b - a) * phase_fractional;
                step = true;
            }
            if phase_integral < length_2 {
                let base = offset_2.wrapping_add(phase_integral) as usize;
                let a = f32::from(at_usize(&SMP_SAMPLE_DATA, base));
                let b = f32::from(at_usize(&SMP_SAMPLE_DATA, base + 1));
                sample_2 = a + (b - a) * phase_fractional;
                step = true;
            }
            if step {
                phase = phase.wrapping_add(phase_increment);
            }
            *out = (sample_1 + (sample_2 - sample_1) * index_fractional) / 65536.0;
        }
        self.phase_ = phase;
        self.damping_ = damp
            * if self.parameter_ >= 0.8 {
                self.parameter_ * 5.0 - 4.0
            } else {
                0.0
            };
        self.damp_state_ = damp;
    }

    pub fn process_mallet(&mut self, flags: u8, out: &mut [f32], size: usize) {
        out[..size].fill(0.0);
        if flags & EXCITER_FLAG_RISING_EDGE != 0 {
            self.damp_state_ = 0.0;
            out[0] = Self::get_pulse_amplitude(self.timbre_);
        }
        if flags & EXCITER_FLAG_GATE == 0 {
            self.damp_state_ = 1.0 - 0.95 * (1.0 - self.damp_state_);
        }
        self.damping_ = self.damp_state_ * (1.0 - self.parameter_);
    }

    pub fn process_plectrum(&mut self, flags: u8, out: &mut [f32], size: usize) {
        let amplitude = Self::get_pulse_amplitude(self.timbre_);
        let mut damp = self.damp_state_;
        let mut impulse = 0.0;
        if flags & EXCITER_FLAG_RISING_EDGE != 0 {
            impulse = -amplitude * (0.05 + self.signature_ * 0.2);
            self.plectrum_delay_ =
                cast_u32(4096.0 * self.parameter_ * self.parameter_).wrapping_add(64);
        }
        for out in &mut out[..size] {
            if self.plectrum_delay_ != 0 {
                self.plectrum_delay_ -= 1;
                if self.plectrum_delay_ == 0 {
                    impulse = amplitude;
                }
                damp = 1.0 - 0.997 * (1.0 - damp);
            } else {
                damp *= 0.9;
            }
            *out = impulse;
            impulse = 0.0;
        }
        self.damping_ = damp * 0.5;
        self.damp_state_ = damp;
    }

    pub fn process_particles(
        &mut self,
        flags: u8,
        out: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        if flags & EXCITER_FLAG_RISING_EDGE != 0 {
            self.particle_state_ = Self::random_sample(random);
            self.particle_state_ = 1.0 - 0.6 * self.particle_state_ * self.particle_state_;
            self.delay_ = 0;
            self.particle_range_ = 1.0;
        }
        out[..size].fill(0.0);

        if flags & EXCITER_FLAG_GATE != 0 {
            let up_probability = cast_u32(0.7 * 4294967296.0);
            let down_probability = cast_u32(0.3 * 4294967296.0);
            let amplitude = Self::get_pulse_amplitude(self.timbre_);
            for out in &mut out[..size] {
                if self.delay_ == 0 {
                    let mut amount = Self::random_sample(random);
                    amount = 1.05 + 0.5 * amount * amount;
                    if random.get_word() > up_probability {
                        self.particle_state_ *= amount;
                        if self.particle_state_ >= (self.particle_range_ + 0.25) {
                            self.particle_state_ = self.particle_range_ + 0.25;
                        }
                    } else if random.get_word() < down_probability {
                        self.particle_state_ /= amount;
                        if self.particle_state_ <= 0.02 {
                            self.particle_state_ = 0.02;
                        }
                    }
                    self.delay_ = cast_u32(self.particle_state_ * 0.15 * K_SAMPLE_RATE);
                    let mut gain = 1.0 - self.particle_range_;
                    gain *= gain;
                    *out = self.particle_state_ * amplitude * (1.0 - gain);
                    let decay_factor = 1.0 - self.parameter_;
                    self.particle_range_ *= 1.0 - decay_factor * decay_factor * 0.5;
                } else {
                    self.delay_ -= 1;
                }
            }
        }
    }

    pub fn process_flow(&mut self, flags: u8, out: &mut [f32], size: usize, random: &mut Random) {
        let scale = self.parameter_ * self.parameter_ * self.parameter_ * self.parameter_;
        let threshold = 0.0001 + scale * 0.125;
        if flags & EXCITER_FLAG_RISING_EDGE != 0 {
            self.particle_state_ = 0.5;
        }
        for out in &mut out[..size] {
            let sample = Self::random_sample(random);
            if sample < threshold {
                self.particle_state_ = -self.particle_state_;
            }
            *out = self.particle_state_ + (sample - 0.5 - self.particle_state_) * scale;
        }
    }

    pub fn process_noise(&mut self, _flags: u8, out: &mut [f32], size: usize, random: &mut Random) {
        for out in &mut out[..size] {
            *out = Self::random_sample(random) - 0.5;
        }
    }
}
