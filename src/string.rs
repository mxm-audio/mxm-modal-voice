//! Ported from elements/dsp/string.{h,cc}: the string, a comb filter with dispersion and a
//! damping filter.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `MIC_W` is not defined (the module's build), so the IIR damping and its delay compensation are
//! in. `src_phase_`, which `Init` never sets, starts at the static object's zero. The C++'s
//! interpolators write their state back when the function returns; here when they are dropped,
//! at the end of `process_internal`.

use crate::cpp::{max, min};
use crate::dsp::K_SAMPLE_RATE;
use crate::resources::LUT_SVF_SHIFT;
use crate::stmlib::dsp::delay_line::DelayLine;
use crate::stmlib::dsp::dsp::{cast_usize, crossfade, interpolate};
use crate::stmlib::dsp::filter::{DCBlocker, FILTER_MODE_LOW_PASS, FREQUENCY_ACCURATE, Svf};
use crate::stmlib::dsp::parameter_interpolator::ParameterInterpolator;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;

pub const K_DELAY_LINE_SIZE: usize = 2048;

#[derive(Default)]
pub struct DampingFilter {
    x_: f32,
    x__: f32,
    brightness_: f32,
    brightness_increment_: f32,
    damping_: f32,
    damping_increment_: f32,
}

impl DampingFilter {
    pub fn init(&mut self) {
        self.x_ = 0.0;
        self.x__ = 0.0;
        self.brightness_ = 0.0;
        self.brightness_increment_ = 0.0;
        self.damping_ = 0.0;
        self.damping_increment_ = 0.0;
    }

    #[inline]
    pub fn configure(&mut self, damping: f32, brightness: f32, size: usize) {
        if size == 0 {
            self.damping_ = damping;
            self.brightness_ = brightness;
            self.damping_increment_ = 0.0;
            self.brightness_increment_ = 0.0;
        } else {
            let step = 1.0 / size as f32;
            self.damping_increment_ = (damping - self.damping_) * step;
            self.brightness_increment_ = (brightness - self.brightness_) * step;
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let h0 = (1.0 + self.brightness_) * 0.5;
        let h1 = (1.0 - self.brightness_) * 0.25;
        let y = self.damping_ * (h0 * self.x_ + h1 * (x + self.x__));
        self.x__ = self.x_;
        self.x_ = x;
        self.brightness_ += self.brightness_increment_;
        self.damping_ += self.damping_increment_;
        y
    }
}

pub type StringDelayLine = DelayLine<K_DELAY_LINE_SIZE>;
pub type StiffnessDelayLine = DelayLine<{ K_DELAY_LINE_SIZE / 2 }>;

#[derive(Default)]
pub struct String {
    frequency_: f32,
    dispersion_: f32,
    brightness_: f32,
    damping_: f32,
    position_: f32,

    delay_: f32,
    clamped_position_: f32,
    previous_dispersion_: f32,
    previous_damping_compensation_: f32,

    enable_dispersion_: bool,
    dispersion_noise_: f32,

    // Very crappy linear interpolation upsampler used for low pitches that do not fit the delay
    // line. Rarely used.
    src_phase_: f32,
    out_sample_: [f32; 2],
    aux_sample_: [f32; 2],

    curved_bridge_: f32,

    string_: StringDelayLine,
    stretch_: StiffnessDelayLine,

    fir_damping_filter_: DampingFilter,
    iir_damping_filter_: Svf,
    dc_blocker_: DCBlocker,
}

impl String {
    /// Every field as the static object's zero, the delay lines too, allocating nothing: for a
    /// host, not in the C++.
    pub fn clear(&mut self) {
        self.frequency_ = 0.0;
        self.dispersion_ = 0.0;
        self.brightness_ = 0.0;
        self.damping_ = 0.0;
        self.position_ = 0.0;
        self.delay_ = 0.0;
        self.clamped_position_ = 0.0;
        self.previous_dispersion_ = 0.0;
        self.previous_damping_compensation_ = 0.0;
        self.enable_dispersion_ = false;
        self.dispersion_noise_ = 0.0;
        self.src_phase_ = 0.0;
        self.out_sample_ = [0.0; 2];
        self.aux_sample_ = [0.0; 2];
        self.curved_bridge_ = 0.0;
        self.string_.clear();
        self.stretch_.clear();
        self.fir_damping_filter_ = DampingFilter::default();
        self.iir_damping_filter_ = Svf::default();
        self.dc_blocker_ = DCBlocker::default();
    }

    pub fn init(&mut self, enable_dispersion: bool) {
        self.enable_dispersion_ = enable_dispersion;

        self.string_.init();
        self.stretch_.init();
        self.fir_damping_filter_.init();
        self.iir_damping_filter_.init();

        self.set_frequency(220.0 / K_SAMPLE_RATE);
        self.set_dispersion(0.25);
        self.set_brightness(0.5);
        self.set_damping(0.3);
        self.set_position(0.8);

        self.delay_ = 1.0 / self.frequency_;
        self.clamped_position_ = 0.0;
        self.previous_dispersion_ = 0.0;
        self.dispersion_noise_ = 0.0;
        self.curved_bridge_ = 0.0;
        self.previous_damping_compensation_ = 0.0;

        self.out_sample_ = [0.0; 2];
        self.aux_sample_ = [0.0; 2];

        self.dc_blocker_.init(1.0 - 20.0 / K_SAMPLE_RATE);
    }

    #[inline]
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency_ = frequency;
    }

    /// `set_frequency(float frequency, float coefficient)`: a step towards it.
    #[inline]
    pub fn set_frequency_coefficient(&mut self, frequency: f32, coefficient: f32) {
        self.frequency_ += coefficient * (frequency - self.frequency_);
    }

    #[inline]
    pub fn set_dispersion(&mut self, mut dispersion: f32) {
        dispersion = if dispersion < 0.24 {
            (dispersion - 0.24) * 4.166
        } else if dispersion > 0.26 {
            (dispersion - 0.26) * 1.35135
        } else {
            0.0
        };
        self.dispersion_ = dispersion;
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
    pub fn mutable_string(&mut self) -> &mut StringDelayLine {
        &mut self.string_
    }

    /// Adds the string's output to `out` and its pickup's to `aux`, as the C++ does.
    pub fn process(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        if self.enable_dispersion_ {
            self.process_internal::<true>(in_, out, aux, size, random);
        } else {
            self.process_internal::<false>(in_, out, aux, size, random);
        }
    }

    fn process_internal<const ENABLE_DISPERSION: bool>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        random: &mut Random,
    ) {
        let mut delay = 1.0 / self.frequency_;
        constrain(&mut delay, 4.0, K_DELAY_LINE_SIZE as f32 - 4.0);

        // If there is not enough delay time in the delay line, we play at the lowest possible
        // note and we upsample on the fly with a shitty linear interpolator. We don't care
        // because it's a corner case (f0 < 11.7Hz)
        let mut src_ratio = delay * self.frequency_;
        if src_ratio >= 0.9999 {
            // When we are above 11.7 Hz, we make sure that the linear interpolator does not get
            // in the way.
            self.src_phase_ = 1.0;
            src_ratio = 1.0;
        }

        let clamped_position = 0.5 - 0.98 * (self.position_ - 0.5).abs();

        // Linearly interpolate all comb-related CV parameters for each sample.
        let mut delay_modulation = ParameterInterpolator::new(&mut self.delay_, delay, size);
        let mut position_modulation =
            ParameterInterpolator::new(&mut self.clamped_position_, clamped_position, size);
        let mut dispersion_modulation =
            ParameterInterpolator::new(&mut self.previous_dispersion_, self.dispersion_, size);

        // For damping/absorption, the interpolation is done in the filter code.
        let lf_damping = self.damping_ * (2.0 - self.damping_);
        let rt60 = 0.07 * semitones_to_ratio(lf_damping * 96.0) * K_SAMPLE_RATE;
        let rt60_base_2_12 = max(-120.0 * delay / src_ratio / rt60, -127.0);
        let mut damping_coefficient = semitones_to_ratio(rt60_base_2_12);
        let mut brightness = self.brightness_ * self.brightness_;
        let noise_filter = semitones_to_ratio((self.brightness_ - 1.0) * 48.0);
        let mut damping_cutoff = min(
            24.0 + self.damping_ * self.damping_ * 48.0
                + self.brightness_ * self.brightness_ * 24.0,
            84.0,
        );
        let mut damping_f = min(self.frequency_ * semitones_to_ratio(damping_cutoff), 0.499);

        // Crossfade to infinite decay.
        if self.damping_ >= 0.95 {
            let to_infinite = 20.0 * (self.damping_ - 0.95);
            damping_coefficient += to_infinite * (1.0 - damping_coefficient);
            brightness += to_infinite * (1.0 - brightness);
            damping_f += to_infinite * (0.4999 - damping_f);
            damping_cutoff += to_infinite * (128.0 - damping_cutoff);
        }

        self.fir_damping_filter_
            .configure(damping_coefficient, brightness, size);
        self.iir_damping_filter_
            .set_f_q::<FREQUENCY_ACCURATE>(damping_f, 0.5);
        let mut damping_compensation_modulation = ParameterInterpolator::new(
            &mut self.previous_damping_compensation_,
            1.0 - interpolate(&LUT_SVF_SHIFT, damping_cutoff, 1.0),
            size,
        );

        for n in 0..size {
            self.src_phase_ += src_ratio;
            if self.src_phase_ > 1.0 {
                self.src_phase_ -= 1.0;

                let mut delay = delay_modulation.next();
                let comb_delay = delay * position_modulation.next();

                delay *= damping_compensation_modulation.next(); // IIR delay.
                delay -= 1.0; // FIR delay.

                let mut s;

                if ENABLE_DISPERSION {
                    let mut noise = 2.0 * random.get_float() - 1.0;
                    noise *= 1.0 / (0.2 + noise_filter);
                    self.dispersion_noise_ += noise_filter * (noise - self.dispersion_noise_);

                    let dispersion = dispersion_modulation.next();
                    let stretch_point = if dispersion <= 0.0 {
                        0.0
                    } else {
                        dispersion * (2.0 - dispersion) * 0.475
                    };
                    let mut noise_amount = if dispersion > 0.75 {
                        4.0 * (dispersion - 0.75)
                    } else {
                        0.0
                    };
                    let mut bridge_curving = if dispersion < 0.0 { -dispersion } else { 0.0 };

                    noise_amount = noise_amount * noise_amount * 0.025;
                    let ac_blocking_amount = bridge_curving;

                    bridge_curving = bridge_curving * bridge_curving * 0.01;
                    let ap_gain = -0.618 * dispersion / (0.15 + dispersion.abs());

                    let mut delay_fm = 1.0f32;
                    delay_fm += self.dispersion_noise_ * noise_amount;
                    delay_fm -= self.curved_bridge_ * bridge_curving;
                    delay *= delay_fm;

                    let ap_delay = delay * stretch_point;
                    let main_delay = delay - ap_delay;
                    if ap_delay >= 4.0 && main_delay >= 4.0 {
                        s = self.string_.read_hermite(main_delay);
                        // Allpass takes a `size_t` delay: the float is truncated.
                        s = self.stretch_.allpass(s, cast_usize(ap_delay), ap_gain);
                    } else {
                        s = self.string_.read_hermite(delay);
                    }
                    let mut s_ac = s;
                    self.dc_blocker_
                        .process(core::slice::from_mut(&mut s_ac), 1);
                    s += ac_blocking_amount * (s_ac - s);

                    let value = s.abs() - 0.025;
                    let sign = if s > 0.0 { 1.0 } else { -1.5 };
                    self.curved_bridge_ = (value.abs() + value) * sign;
                } else {
                    s = self.string_.read_hermite(delay);
                }

                s += in_[n]; // When f0 < 11.7 Hz, causes ugly bitcrushing on the input!
                s = self.fir_damping_filter_.process(s);
                s = self.iir_damping_filter_.process::<FILTER_MODE_LOW_PASS>(s);
                self.string_.write(s);

                self.out_sample_[1] = self.out_sample_[0];
                self.aux_sample_[1] = self.aux_sample_[0];

                self.out_sample_[0] = s;
                self.aux_sample_[0] = self.string_.read_f(comb_delay);
            }
            out[n] += crossfade(self.out_sample_[1], self.out_sample_[0], self.src_phase_);
            aux[n] += crossfade(self.aux_sample_[1], self.aux_sample_[0], self.src_phase_);
        }
    }
}
