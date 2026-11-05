//! Ported from elements/dsp/resonator.{h,cc}: the modal resonator, 64 band-pass modes and 8
//! bowed ones.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `clock_divider_` and `lfo_phase_`, which `Init` never sets, start at the static object's zero
//! and carry on across an `init`, as in the C++.

use crate::cpp::min;
use crate::dsp::K_SAMPLE_RATE;
use crate::resources::{LUT_4_DECADES, LUT_STIFFNESS};
use crate::stmlib::dsp::cosine_oscillator::{COSINE_OSCILLATOR_APPROXIMATE, CosineOscillator};
use crate::stmlib::dsp::delay_line::DelayLine;
use crate::stmlib::dsp::dsp::{cast_usize, interpolate};
use crate::stmlib::dsp::filter::{
    FILTER_MODE_BAND_PASS, FILTER_MODE_BAND_PASS_NORMALIZED, FREQUENCY_FAST, Svf,
};

pub const K_MAX_MODES: usize = 64;
pub const K_MAX_BOWED_MODES: usize = 8;
pub const K_MAX_DELAY_LINE_SIZE: usize = 1024;

pub struct Resonator {
    frequency_: f32,
    geometry_: f32,
    brightness_: f32,
    position_: f32,
    previous_position_: f32,
    damping_: f32,
    modulation_frequency_: f32,
    modulation_offset_: f32,
    lfo_phase_: f32,
    bow_signal_: f32,
    resolution_: usize,
    f_: [Svf; K_MAX_MODES],
    f_bow_: [Svf; K_MAX_BOWED_MODES],
    d_bow_: [DelayLine<K_MAX_DELAY_LINE_SIZE>; K_MAX_BOWED_MODES],
    clock_divider_: usize,
}

impl Default for Resonator {
    fn default() -> Self {
        Self {
            frequency_: 0.0,
            geometry_: 0.0,
            brightness_: 0.0,
            position_: 0.0,
            previous_position_: 0.0,
            damping_: 0.0,
            modulation_frequency_: 0.0,
            modulation_offset_: 0.0,
            lfo_phase_: 0.0,
            bow_signal_: 0.0,
            resolution_: 0,
            f_: std::array::from_fn(|_| Svf::default()),
            f_bow_: std::array::from_fn(|_| Svf::default()),
            d_bow_: std::array::from_fn(|_| DelayLine::default()),
            clock_divider_: 0,
        }
    }
}

impl Resonator {
    pub fn init(&mut self) {
        for f in &mut self.f_ {
            f.init();
        }
        for i in 0..K_MAX_BOWED_MODES {
            self.f_bow_[i].init();
            self.d_bow_[i].init();
        }

        self.set_frequency(220.0 / K_SAMPLE_RATE);
        self.set_geometry(0.25);
        self.set_brightness(0.5);
        self.set_damping(0.3);
        self.set_position(0.999);
        self.previous_position_ = 0.0;
        self.set_resolution(K_MAX_MODES);

        self.bow_signal_ = 0.0;
    }

    #[inline]
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency_ = frequency;
    }

    #[inline]
    pub fn set_geometry(&mut self, geometry: f32) {
        self.geometry_ = geometry;
    }

    #[inline]
    pub fn set_brightness(&mut self, brightness: f32) {
        self.brightness_ = brightness;
    }

    #[inline]
    pub fn set_damping(&mut self, damping: f32) {
        self.damping_ = damping;
    }

    #[inline]
    pub fn set_position(&mut self, position: f32) {
        self.position_ = position;
    }

    #[inline]
    pub fn set_resolution(&mut self, resolution: usize) {
        self.resolution_ = min(resolution, K_MAX_MODES);
    }

    #[inline]
    pub fn set_modulation_frequency(&mut self, modulation_frequency: f32) {
        self.modulation_frequency_ = modulation_frequency;
    }

    #[inline]
    pub fn set_modulation_offset(&mut self, modulation_offset: f32) {
        self.modulation_offset_ = modulation_offset;
    }

    #[inline]
    pub fn bow_table(mut x: f32, velocity: f32) -> f32 {
        x = 0.13 * velocity - x;
        let mut bow = x;
        bow *= 6.0;
        // `fabs(bow) + 0.75`: a double literal, so the sum is a double, rounded back to float.
        bow = (f64::from(bow.abs()) + 0.75) as f32;
        bow *= bow;
        bow *= bow;
        bow = 0.25 / bow;
        if bow < 0.0025 {
            bow = 0.0025;
        }
        if bow > 0.245 {
            bow = 0.245;
        }
        x * bow
    }

    fn compute_filters(&mut self) -> usize {
        self.clock_divider_ = self.clock_divider_.wrapping_add(1);

        let mut stiffness = interpolate(&LUT_STIFFNESS, self.geometry_, 256.0);
        let mut harmonic = self.frequency_;
        let mut stretch_factor = 1.0f32;
        let mut q = 500.0 * interpolate(&LUT_4_DECADES, self.damping_ * 0.8, 256.0);
        let mut brightness_attenuation = 1.0 - self.geometry_;
        // Reduces the range of brightness when geometry is very low, to prevent clipping.
        brightness_attenuation *= brightness_attenuation;
        brightness_attenuation *= brightness_attenuation;
        brightness_attenuation *= brightness_attenuation;
        let brightness = self.brightness_ * (1.0 - 0.2 * brightness_attenuation);
        let mut q_loss = brightness * (2.0 - brightness) * 0.85 + 0.15;
        let q_loss_damping_rate = self.geometry_ * (2.0 - self.geometry_) * 0.1;
        let mut num_modes = 0;
        for i in 0..min(K_MAX_MODES, self.resolution_) {
            // Update the first 24 modes every time (2kHz). The higher modes are refreshed as a
            // slowest rate.
            let update = i <= 24 || ((i & 1) == (self.clock_divider_ & 1));
            let mut partial_frequency = harmonic * stretch_factor;
            if partial_frequency >= 0.49 {
                partial_frequency = 0.49;
            } else {
                num_modes = i + 1;
            }
            if update {
                self.f_[i]
                    .set_f_q::<FREQUENCY_FAST>(partial_frequency, 1.0 + partial_frequency * q);
                if i < K_MAX_BOWED_MODES {
                    let mut period = cast_usize(1.0 / partial_frequency);
                    while period >= K_MAX_DELAY_LINE_SIZE {
                        period >>= 1;
                    }
                    self.d_bow_[i].set_delay(period);
                    let g = self.f_[i].g();
                    self.f_bow_[i].set_g_q(g, 1.0 + partial_frequency * 1500.0);
                }
            }
            stretch_factor += stiffness;
            if stiffness < 0.0 {
                // Make sure that the partials do not fold back into negative frequencies.
                stiffness *= 0.93;
            } else {
                // This helps adding a few extra partials in the highest frequencies.
                stiffness *= 0.98;
            }
            // This prevents the highest partials from decaying too fast.
            q_loss += q_loss_damping_rate * (1.0 - q_loss);
            harmonic += self.frequency_;
            q *= q_loss;
        }

        num_modes
    }

    pub fn process(
        &mut self,
        bow_strength: &[f32],
        in_: &[f32],
        center: &mut [f32],
        sides: &mut [f32],
        size: usize,
    ) {
        let num_modes = self.compute_filters();
        let num_banded_wg = min(K_MAX_BOWED_MODES, num_modes);

        // Linearly interpolate position. This parameter is extremely sensitive to zipper noise.
        let position_increment = (self.position_ - self.previous_position_) / size as f32;
        for n in 0..size {
            // 0.5 Hz LFO used to modulate the position of the stereo side channel.
            self.lfo_phase_ += self.modulation_frequency_;
            if self.lfo_phase_ >= 1.0 {
                self.lfo_phase_ -= 1.0;
            }
            self.previous_position_ += position_increment;
            let lfo = if self.lfo_phase_ > 0.5 {
                1.0 - self.lfo_phase_
            } else {
                self.lfo_phase_
            };
            let mut amplitudes = CosineOscillator::default();
            let mut aux_amplitudes = CosineOscillator::default();
            amplitudes.init::<COSINE_OSCILLATOR_APPROXIMATE>(self.previous_position_);
            aux_amplitudes.init::<COSINE_OSCILLATOR_APPROXIMATE>(self.modulation_offset_ + lfo);

            // Render normal modes.
            let mut input = in_[n] * 0.125;
            let mut sum_center = 0.0f32;
            let mut sum_side = 0.0f32;

            amplitudes.start();
            aux_amplitudes.start();
            for f in &mut self.f_[..num_modes] {
                let s = f.process::<FILTER_MODE_BAND_PASS>(input);
                sum_center += s * amplitudes.next();
                sum_side += s * aux_amplitudes.next();
            }
            sides[n] = sum_side - sum_center;

            // Render bowed modes.
            let mut bow_signal = 0.0f32;
            input += self.bow_signal_;
            amplitudes.start();
            for i in 0..num_banded_wg {
                let mut s = 0.99 * self.d_bow_[i].read();
                bow_signal += s;
                s = self.f_bow_[i].process::<FILTER_MODE_BAND_PASS_NORMALIZED>(input + s);
                self.d_bow_[i].write(s);
                sum_center += s * amplitudes.next() * 8.0;
            }
            self.bow_signal_ = Self::bow_table(bow_signal, bow_strength[n]);
            center[n] = sum_center;
        }
    }
}
