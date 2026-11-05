//! Ported from elements/dsp/ominous_voice.{h,cc}: "Ominous", a dark 2x2-op FM synth (based on
//! Braids' FM and FBFM modes), the module's hidden second voice.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! What `Init` never sets (`damping_`, `feedback_`, the oscillators' phases) starts at the static
//! object's zero. The two `NaiveSvf::set_f_q<FREQUENCY_EXACT>` in `Init` take the reference
//! build's `sinf` results (`stmlib::dsp::filter`), so nothing here calls a maths library.

use crate::cpp::at_usize;
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::multistage_envelope::{
    ENVELOPE_FLAG_FALLING_EDGE, ENVELOPE_FLAG_GATE, ENVELOPE_FLAG_RISING_EDGE, MultistageEnvelope,
};
use crate::patch::Patch;
use crate::resources::{
    LUT_DETUNE_QUANTIZER, LUT_FM_FREQUENCY_QUANTIZER, LUT_MIDI_TO_F_HIGH, LUT_MIDI_TO_F_LOW,
    LUT_MIDI_TO_INCREMENT_HIGH, LUT_SINE,
};
use crate::stmlib::dsp::dsp::{cast_i32, cast_u32, clip16, interpolate, interpolate_wrap};
use crate::stmlib::dsp::filter::{
    FILTER_MODE_LOW_PASS, FREQUENCY_EXACT, FREQUENCY_FAST, NaiveSvf, Svf,
};

pub const K_OVERSAMPLING_DOWN_MIDI: f32 = -36.0;
pub const K_OVERSAMPLING_UP: usize = 8;
pub const K_NUM_OSCILLATORS: usize = 2;

// scipy.signal.remez(101, [0, 0.3 / 8, 0.495 / 8, 0.5], [1, 0]);
pub static K_DOWNSAMPLING_FILTER: [f32; 101] = [
    -0.001859272945,
    0.001184937535,
    0.001212413444,
    0.001369688661,
    0.001555406705,
    0.001685761819,
    0.001692922383,
    0.001526182555,
    0.001157229282,
    0.000582212588,
    -0.000172916131,
    -0.001054896973,
    -0.001981709311,
    -0.002852566234,
    -0.003555372122,
    -0.003977161477,
    -0.004020648315,
    -0.003616652394,
    -0.002735861951,
    -0.001399720214,
    0.000314317050,
    0.002272758644,
    0.004295073578,
    0.006166173855,
    0.007654982354,
    0.008534833653,
    0.008614353501,
    0.007761247708,
    0.005919024408,
    0.003133736224,
    -0.000445934810,
    -0.004563290490,
    -0.008867277113,
    -0.012932060625,
    -0.016287075604,
    -0.018456326393,
    -0.018994127671,
    -0.017542469537,
    -0.013838153737,
    -0.007789574712,
    0.000541523640,
    0.010920252821,
    0.022936167260,
    0.036030557729,
    0.049530599001,
    0.062696232995,
    0.074769247024,
    0.085030908865,
    0.092854033134,
    0.097753623479,
    0.099422113893,
    0.097753623479,
    0.092854033134,
    0.085030908865,
    0.074769247024,
    0.062696232995,
    0.049530599001,
    0.036030557729,
    0.022936167260,
    0.010920252821,
    0.000541523640,
    -0.007789574712,
    -0.013838153737,
    -0.017542469537,
    -0.018994127671,
    -0.018456326393,
    -0.016287075604,
    -0.012932060625,
    -0.008867277113,
    -0.004563290490,
    -0.000445934810,
    0.003133736224,
    0.005919024408,
    0.007761247708,
    0.008614353501,
    0.008534833653,
    0.007654982354,
    0.006166173855,
    0.004295073578,
    0.002272758644,
    0.000314317050,
    -0.001399720214,
    -0.002735861951,
    -0.003616652394,
    -0.004020648315,
    -0.003977161477,
    -0.003555372122,
    -0.002852566234,
    -0.001981709311,
    -0.001054896973,
    -0.000172916131,
    0.000582212588,
    0.001157229282,
    0.001526182555,
    0.001692922383,
    0.001685761819,
    0.001555406705,
    0.001369688661,
    0.001212413444,
    0.001184937535,
    -0.001859272945,
];

/// `FIRDownsampler<filter_size, buffer_size, ratio>`, for the one instantiation Elements makes:
/// `<101, 128, kOversamplingUp>`.
pub struct FIRDownsampler {
    ptr_: i32,
    coefficients_: &'static [f32; 101],
    buffer_: [f32; 256],
}

impl Default for FIRDownsampler {
    fn default() -> Self {
        Self {
            ptr_: 0,
            coefficients_: &K_DOWNSAMPLING_FILTER,
            buffer_: [0.0; 256],
        }
    }
}

impl FIRDownsampler {
    const FILTER_SIZE: usize = 101;
    const BUFFER_SIZE: i32 = 128;
    const RATIO: usize = K_OVERSAMPLING_UP;

    pub fn init(&mut self, filter_coefficients: &'static [f32; 101]) {
        self.coefficients_ = filter_coefficients;
        self.buffer_.fill(0.0);
        self.ptr_ = 0;
    }

    /// size is expected to be a multiple of the downsampling ratio.
    pub fn process(&mut self, in_: &[f32], out: &mut [f32], mut size: usize) {
        let mut in_ = in_.iter();
        let mut out = out.iter_mut();
        while size != 0 {
            for _ in 0..Self::RATIO {
                let x = in_.next().copied().unwrap_or(0.0);
                let ptr = self.ptr_ as usize;
                self.buffer_[ptr] = x;
                self.buffer_[ptr + Self::BUFFER_SIZE as usize] = x;
                self.ptr_ = (self.ptr_ + (Self::BUFFER_SIZE - 1)) & (Self::BUFFER_SIZE - 1);
                size = size.wrapping_sub(1);
            }
            let mut s = 0.0f32;
            let ptr = self.ptr_ as usize;
            for i in 0..Self::FILTER_SIZE {
                s += self.buffer_[ptr + i + 1] * self.coefficients_[i];
            }
            if let Some(out) = out.next() {
                *out = s;
            }
        }
    }
}

#[derive(Default)]
pub struct Spatializer {
    behind_: [f32; K_MAX_BLOCK_SIZE],
    left_: f32,
    right_: f32,
    angle_: f32,
    distance_: f32,
    fixed_position_: f32,
    behind_filter_: NaiveSvf,
}

impl Spatializer {
    pub fn init(&mut self, fixed_position: f32) {
        self.angle_ = 0.0;
        self.fixed_position_ = fixed_position;
        self.left_ = 0.0;
        self.right_ = 0.0;
        self.distance_ = 0.0;
        self.behind_filter_.init();
        self.behind_filter_.set_f_q::<FREQUENCY_EXACT>(0.05, 1.0);
    }

    #[inline]
    pub fn rotate(&mut self, rotation_speed: f32) {
        self.angle_ += rotation_speed;
        if self.angle_ >= 1.0 {
            self.angle_ -= 1.0;
        }
        if self.angle_ < 0.0 {
            self.angle_ += 1.0;
        }
    }

    #[inline]
    pub fn set_distance(&mut self, distance: f32) {
        self.distance_ = distance;
    }

    pub fn process(&mut self, source: &[f32], center: &mut [f32], sides: &mut [f32], size: usize) {
        // `Process<FILTER_MODE_LOW_PASS>(source, behind_, size, 1)`: decimating by one.
        self.behind_filter_
            .process_buffer::<FILTER_MODE_LOW_PASS>(source, &mut self.behind_, size);
        let angle = self.angle_;
        let mut x = self.distance_ * interpolate_wrap(&LUT_SINE, angle, 4096.0);
        let y = self.distance_ * interpolate_wrap(&LUT_SINE, angle + 0.25, 4096.0);
        let backfront = (1.0 + y) * 0.5 * self.distance_;
        x += self.fixed_position_ * (1.0 - self.distance_);

        let target_left = interpolate_wrap(&LUT_SINE, (1.0 + x) * 0.125, 4096.0);
        let target_right = interpolate_wrap(&LUT_SINE, (3.0 + x) * 0.125, 4096.0);

        // Prevent zipper noise during rendering.
        let step = 1.0 / size as f32;
        let left_increment = (target_left - self.left_) * step;
        let right_increment = (target_right - self.right_) * step;

        for i in 0..size {
            self.left_ += left_increment;
            self.right_ += right_increment;
            let y = source[i] + backfront * (self.behind_[i] - source[i]);
            let l = self.left_ * y;
            let r = self.right_ * y;
            center[i] += (l + r) * 0.5;
            sides[i] += (l - r) * 0.5 / 0.7;
        }
    }
}

#[derive(Default)]
pub struct FmOscillator {
    fm_amount_: f32,
    previous_sample_: f32,
    phase_carrier_: u32,
    phase_mod_: u32,
}

impl FmOscillator {
    pub fn init(&mut self) {
        self.fm_amount_ = 0.0;
        self.previous_sample_ = 0.0;
    }

    #[inline]
    fn midi_to_increment(midi_pitch: f32) -> f32 {
        let mut pitch = cast_i32(midi_pitch * 256.0);
        pitch = 32768 + clip16(pitch.wrapping_sub(20480));
        let pitch = pitch as usize;
        at_usize(&LUT_MIDI_TO_INCREMENT_HIGH, pitch >> 8)
            * at_usize(&LUT_MIDI_TO_F_LOW, pitch & 0xff)
    }

    #[inline]
    fn sine(phase: u32) -> f32 {
        let integral = (phase >> 20) as usize;
        let fractional = (phase << 12) as f32 / 4294967296.0;
        let a = LUT_SINE[integral];
        let b = LUT_SINE[integral + 1];
        a + (b - a) * fractional
    }

    #[inline]
    fn sine_fm(mut phase: u32, fm: f32) -> f32 {
        phase = phase.wrapping_add(cast_u32(fm * 2147483648.0));
        Self::sine(phase)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        frequency: f32,
        mut ratio: f32,
        feedback_amount: f32,
        target_fm_amount: f32,
        external_fm: &[f32],
        destination: &mut [f32],
        size: usize,
    ) {
        ratio = interpolate(&LUT_FM_FREQUENCY_QUANTIZER, ratio, 128.0);

        let inc_carrier = cast_u32(Self::midi_to_increment(frequency));
        let inc_mod = cast_u32(Self::midi_to_increment(frequency + ratio));

        let mut phase_carrier = self.phase_carrier_;
        let mut phase_mod = self.phase_mod_;

        // Linear interpolation on FM amount parameter.
        let step = 1.0 / size as f32;
        let mut fm_amount = self.fm_amount_;
        let fm_amount_increment = (target_fm_amount - fm_amount) * step;

        let mut previous_sample = self.previous_sample_;

        // To prevent aliasing, reduce FM amount when frequency or feedback are too high.
        let brightness = frequency + ratio * 0.75 - 60.0 + feedback_amount * 24.0;
        let mut amount_attenuation = if brightness <= 0.0 {
            1.0
        } else {
            1.0 - brightness * brightness * 0.0015
        };
        if amount_attenuation < 0.0 {
            amount_attenuation = 0.0;
        }

        for i in 0..size {
            fm_amount += fm_amount_increment;
            phase_carrier = phase_carrier.wrapping_add(inc_carrier);
            phase_mod = phase_mod.wrapping_add(inc_mod);
            let mod_ = Self::sine_fm(phase_mod, feedback_amount * previous_sample);
            previous_sample = Self::sine_fm(
                phase_carrier,
                amount_attenuation * (mod_ * fm_amount + external_fm[i]),
            );
            destination[i] = previous_sample;
        }

        self.phase_carrier_ = phase_carrier;
        self.phase_mod_ = phase_mod;
        self.fm_amount_ = fm_amount;
        self.previous_sample_ = previous_sample;
    }
}

pub struct OminousVoice {
    external_fm_oversampled_: [f32; K_OVERSAMPLING_UP * K_MAX_BLOCK_SIZE],
    osc_oversampled_: [f32; K_OVERSAMPLING_UP * K_MAX_BLOCK_SIZE],
    osc_: [f32; K_MAX_BLOCK_SIZE],

    previous_gate_: bool,
    envelope_: MultistageEnvelope,
    level_state_: f32,
    damping_: f32,
    feedback_: f32,

    osc_level_: [f32; K_NUM_OSCILLATORS],
    external_fm_state_: [f32; K_NUM_OSCILLATORS],

    oscillator_: [FmOscillator; K_NUM_OSCILLATORS],
    iir_downsampler_: [NaiveSvf; K_NUM_OSCILLATORS],
    fir_downsampler_: [FIRDownsampler; K_NUM_OSCILLATORS],
    filter_: [Svf; K_NUM_OSCILLATORS],
    spatializer_: [Spatializer; K_NUM_OSCILLATORS],
}

impl Default for OminousVoice {
    fn default() -> Self {
        Self {
            external_fm_oversampled_: [0.0; K_OVERSAMPLING_UP * K_MAX_BLOCK_SIZE],
            osc_oversampled_: [0.0; K_OVERSAMPLING_UP * K_MAX_BLOCK_SIZE],
            osc_: [0.0; K_MAX_BLOCK_SIZE],
            previous_gate_: false,
            envelope_: MultistageEnvelope::default(),
            level_state_: 0.0,
            damping_: 0.0,
            feedback_: 0.0,
            osc_level_: [0.0; K_NUM_OSCILLATORS],
            external_fm_state_: [0.0; K_NUM_OSCILLATORS],
            oscillator_: Default::default(),
            iir_downsampler_: Default::default(),
            fir_downsampler_: Default::default(),
            filter_: Default::default(),
            spatializer_: Default::default(),
        }
    }
}

impl OminousVoice {
    /// A voice as the firmware's static one is before `Init`: every field zero.
    pub fn new() -> Box<Self> {
        Box::default()
    }

    /// Every field as [`OminousVoice::new`] made it, allocating nothing: for a host's reset,
    /// not in the C++.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn init(&mut self) {
        self.envelope_.init();
        self.envelope_.set_adsr(0.5, 0.5, 0.5, 0.5);
        self.previous_gate_ = false;
        self.level_state_ = 0.0;
        for i in 0..K_NUM_OSCILLATORS {
            self.external_fm_state_[i] = 0.0;
            self.oscillator_[i].init();

            // Downsampling is done mostly by the FIR, but since the stopband attenuation peaks
            // at -48dB, we can get a few extra dB of attenution with the IIR for the highest
            // frequencies.
            self.fir_downsampler_[i].init(&K_DOWNSAMPLING_FILTER);
            self.iir_downsampler_[i].init();
            self.iir_downsampler_[i]
                .set_f_q::<FREQUENCY_EXACT>(1.0 / K_OVERSAMPLING_UP as f32 * 0.8, 0.5);

            self.osc_level_[i] = 0.0;
            self.filter_[i].init();

            self.spatializer_[i].init(if i == 0 { -0.7 } else { 0.7 });
        }
    }

    /// The envelope as it ended the last block: for a host (its *Contour* source), not in the
    /// C++.
    #[inline]
    pub fn envelope(&self) -> f32 {
        self.envelope_.value()
    }

    fn configure_envelope(&mut self, patch: &Patch) {
        if patch.exciter_envelope_shape < 0.4 {
            let a = 0.0;
            let dr = (patch.exciter_envelope_shape * 0.625 + 0.2) * 1.8;
            self.envelope_.set_adsr(a, dr, 0.0, dr);
        } else if patch.exciter_envelope_shape < 0.6 {
            let s = (patch.exciter_envelope_shape - 0.4) * 5.0;
            self.envelope_.set_adsr(0.0, 0.80, s, 0.80);
        } else {
            let a = 0.0;
            let dr = ((1.0 - patch.exciter_envelope_shape) * 0.75 + 0.15) * 1.8;
            self.envelope_.set_adsr(a, dr, 1.0, dr);
        }
    }

    /// `Upsample<kOversamplingUp>`.
    fn upsample(state: &mut f32, source: &[f32], destination: &mut [f32], source_size: usize) {
        let down = 1.0 / K_OVERSAMPLING_UP as f32;
        let mut s = *state;
        let mut d = 0;
        for &x in &source[..source_size] {
            let increment = (x - s) * down;
            for _ in 0..K_OVERSAMPLING_UP {
                destination[d] = s;
                d += 1;
                s += increment;
            }
        }
        *state = s;
    }

    #[inline]
    fn get_gate_flags(&mut self, gate_in: bool) -> u8 {
        let mut flags = 0;
        if gate_in {
            if !self.previous_gate_ {
                flags |= ENVELOPE_FLAG_RISING_EDGE;
            }
            flags |= ENVELOPE_FLAG_GATE;
        } else if self.previous_gate_ {
            flags = ENVELOPE_FLAG_FALLING_EDGE;
        }
        self.previous_gate_ = gate_in;
        flags
    }

    #[inline]
    fn midi_to_frequency(mut midi_pitch: f32) -> f32 {
        if midi_pitch < -12.0 {
            midi_pitch = -12.0;
        }
        let mut pitch = cast_i32(midi_pitch * 256.0);
        pitch = 32768 + clip16(pitch.wrapping_sub(20480));
        let pitch = pitch as usize;
        at_usize(&LUT_MIDI_TO_F_HIGH, pitch >> 8) * at_usize(&LUT_MIDI_TO_F_LOW, pitch & 0xff)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        patch: &Patch,
        mut frequency: f32,
        strength: f32,
        gate_in: bool,
        blow_in: &[f32],
        strike_in: &[f32],
        raw: &mut [f32],
        center: &mut [f32],
        sides: &mut [f32],
        size: usize,
    ) {
        let flags = self.get_gate_flags(gate_in);

        // Compute the envelope.
        self.configure_envelope(patch);
        let mut level = self.envelope_.process(flags);
        level += if strength >= 0.5 {
            2.0 * strength - 1.0
        } else {
            0.0
        };
        let level_increment = (level - self.level_state_) / size as f32;

        self.damping_ += 0.1 * (patch.resonator_damping - self.damping_);
        let filter_env_amount = if self.damping_ <= 0.9 {
            1.1 * self.damping_
        } else {
            0.99
        };
        let vca_env_amount =
            1.0 + self.damping_ * self.damping_ * self.damping_ * self.damping_ * 0.5;

        // Comfigure the filter.
        let mut cutoff_midi = 12.0f32;
        cutoff_midi += patch.resonator_brightness * 140.0;
        cutoff_midi += filter_env_amount * level * 120.0;
        cutoff_midi += 0.5 * (frequency - 64.0);

        let cutoff = Self::midi_to_frequency(cutoff_midi);
        let q_bump = patch.resonator_geometry - 0.6;
        let q = 1.72 - q_bump * q_bump * 2.0;
        let cutoff_2 = cutoff * (1.0 + patch.resonator_modulation_offset);
        self.filter_[0].set_f_q::<FREQUENCY_FAST>(cutoff, q);
        self.filter_[1].set_f_q::<FREQUENCY_FAST>(cutoff_2, q * 1.25);

        // Process each oscillator.
        center[..size].fill(0.0);
        sides[..size].fill(0.0);
        raw[..size].fill(0.0);

        let rotation_speed = [1.0f32, 1.123456];

        self.feedback_ += 0.01 * (patch.exciter_bow_timbre - self.feedback_);
        frequency += K_OVERSAMPLING_DOWN_MIDI;
        for i in 0..2 {
            Self::upsample(
                &mut self.external_fm_state_[i],
                if i == 0 { blow_in } else { strike_in },
                &mut self.external_fm_oversampled_,
                size,
            );

            // The C++ shadows the envelope's `level` with each oscillator's here.
            let (detune, ratio, amount, osc_level) = if i == 0 {
                (
                    0.0,
                    patch.exciter_blow_meta,
                    patch.exciter_blow_timbre,
                    patch.exciter_blow_level,
                )
            } else {
                (
                    interpolate(&LUT_DETUNE_QUANTIZER, patch.exciter_bow_level, 64.0),
                    patch.exciter_strike_meta,
                    patch.exciter_strike_timbre,
                    patch.exciter_strike_level,
                )
            };

            self.oscillator_[i].process(
                frequency + detune,
                ratio,
                self.feedback_ * (0.25 + 0.15 * patch.exciter_signature),
                (2.0 - patch.exciter_signature * self.feedback_) * amount,
                &self.external_fm_oversampled_,
                &mut self.osc_oversampled_,
                size * K_OVERSAMPLING_UP,
            );

            // `Process<FILTER_MODE_LOW_PASS>(osc_oversampled_, osc_oversampled_, …, 1)`.
            self.iir_downsampler_[i].process_buffer_in_place::<FILTER_MODE_LOW_PASS>(
                &mut self.osc_oversampled_,
                size * K_OVERSAMPLING_UP,
            );

            self.fir_downsampler_[i].process(
                &self.osc_oversampled_,
                &mut self.osc_,
                size * K_OVERSAMPLING_UP,
            );

            // Copy to raw buffer.
            let mut level_state = self.osc_level_[i];
            for j in 0..size {
                level_state += 0.01 * (osc_level - level_state);
                self.osc_[j] *= level_state;
                raw[j] += self.osc_[j];
            }
            self.osc_level_[i] = level_state;

            // Apply filter.
            self.filter_[i].process_multimode_in_place(
                &mut self.osc_,
                size,
                patch.resonator_geometry,
            );

            // Apply VCA.
            let mut l = self.level_state_;
            for j in 0..size {
                let mut gain = l * vca_env_amount;
                if gain >= 1.0 {
                    gain = 1.0;
                }
                self.osc_[j] *= gain;
                l += level_increment;
            }

            // Spatialize.
            let f = patch.resonator_position * patch.resonator_position * 0.001;
            let distance = patch.resonator_position;
            self.spatializer_[i].rotate(f * rotation_speed[i]);
            self.spatializer_[i].set_distance(distance * (2.0 - distance));
            self.spatializer_[i].process(&self.osc_, center, sides, size);
        }
        self.level_state_ = level;
    }
}
