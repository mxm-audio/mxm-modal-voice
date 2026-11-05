//! Ported from elements/dsp/voice.{h,cc}: the modal synthesis voice, its exciters into its
//! resonator, the modal one or the strings.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The module has one static `Random`; here each voice carries its own, starting where that one
//! starts, so a voice draws exactly what the module's one voice draws. `Init` does not reset it,
//! as the C++ never resets its generator.

use crate::cpp::at;
use crate::dsp::{K_MAX_BLOCK_SIZE, K_SAMPLE_RATE};
use crate::exciter::{
    EXCITER_FLAG_FALLING_EDGE, EXCITER_FLAG_GATE, EXCITER_FLAG_RISING_EDGE, EXCITER_MODEL_FLOW,
    EXCITER_MODEL_GRANULAR_SAMPLE_PLAYER, EXCITER_MODEL_PARTICLES, EXCITER_MODEL_SAMPLE_PLAYER,
    Exciter,
};
use crate::fx::diffuser::Diffuser;
use crate::multistage_envelope::MultistageEnvelope;
use crate::patch::Patch;
use crate::resonator::Resonator;
use crate::resources::{LUT_ACCENT_GAIN_COARSE, LUT_ACCENT_GAIN_FINE};
use crate::stmlib::dsp::dsp::{cast_i32, make_integral_fractional};
use crate::stmlib::dsp::filter::DCBlocker;
use crate::stmlib::dsp::units::semitones_to_ratio;
use crate::stmlib::stmlib::constrain;
use crate::stmlib::utils::random::Random;
use crate::string::String;
use crate::tube::Tube;

pub const K_NUM_STRINGS: usize = 5;

/// `ResonatorModel`.
pub type ResonatorModel = u8;
pub const RESONATOR_MODEL_MODAL: ResonatorModel = 0;
pub const RESONATOR_MODEL_STRING: ResonatorModel = 1;
pub const RESONATOR_MODEL_STRINGS: ResonatorModel = 2;

/// `chords`: the strings' transpositions, one chord a row, as `voice.cc` has them.
pub static CHORDS: [[f32; 5]; 11] = [
    [0.0, -12.0, 0.0, 0.01, 12.0],
    [0.0, -12.0, 3.0, 7.0, 10.0],
    [0.0, -12.0, 3.0, 7.0, 12.0],
    [0.0, -12.0, 3.0, 7.0, 14.0],
    [0.0, -12.0, 3.0, 7.0, 17.0],
    [0.0, -12.0, 7.0, 12.0, 19.0],
    [0.0, -12.0, 4.0, 7.0, 17.0],
    [0.0, -12.0, 4.0, 7.0, 14.0],
    [0.0, -12.0, 4.0, 7.0, 12.0],
    [0.0, -12.0, 4.0, 7.0, 11.0],
    [0.0, -12.0, 5.0, 7.0, 12.0],
];

#[derive(Default)]
pub struct Voice {
    envelope_: MultistageEnvelope,
    tube_: Tube,
    bow_: Exciter,
    blow_: Exciter,
    strike_: Exciter,
    diffuser_: Diffuser,
    resonator_: Resonator,
    string_: [String; K_NUM_STRINGS],
    dc_blocker_: DCBlocker,

    strength_: f32,
    envelope_value_: f32,
    exciter_level_: f32,

    bow_buffer_: [f32; K_MAX_BLOCK_SIZE],
    bow_strength_buffer_: [f32; K_MAX_BLOCK_SIZE],
    blow_buffer_: [f32; K_MAX_BLOCK_SIZE],
    strike_buffer_: [f32; K_MAX_BLOCK_SIZE],

    previous_gate_: bool,
    resonator_model_: ResonatorModel,
    chord_index_: f32,

    /// The module's one static generator, this voice's own (not in the C++).
    random: Random,
}

impl Voice {
    /// A voice as the firmware's static one is before `Init`: every field zero, the random
    /// stream at its start. Its buffers, some 100 KB, are allocated here, once.
    pub fn new() -> Box<Self> {
        Box::default()
    }

    pub fn init(&mut self) {
        self.envelope_.init();
        self.bow_.init();
        self.blow_.init();
        self.strike_.init();
        self.diffuser_.init();

        self.reset_resonator();

        self.bow_.set_model(EXCITER_MODEL_FLOW);
        self.bow_.set_parameter(0.7);
        self.bow_.set_timbre(0.5);

        self.blow_.set_model(EXCITER_MODEL_GRANULAR_SAMPLE_PLAYER);

        self.envelope_.set_adsr(0.5, 0.5, 0.5, 0.5);
        self.previous_gate_ = false;
        self.strength_ = 0.0;
        self.exciter_level_ = 0.0;
        self.envelope_value_ = 0.0;
        self.chord_index_ = 0.0;
        self.resonator_model_ = RESONATOR_MODEL_MODAL;
    }

    fn reset_resonator(&mut self) {
        self.resonator_.init();
        for string in &mut self.string_ {
            string.init(true);
        }
        self.dc_blocker_.init(1.0 - 10.0 / K_SAMPLE_RATE);
        self.resonator_.set_resolution(52); // Runs with 56 extremely tightly.
    }

    /// For metering.
    #[inline]
    pub fn exciter_level(&self) -> f32 {
        self.exciter_level_
    }

    #[inline]
    pub fn panic(&mut self) {
        self.reset_resonator();
    }

    #[inline]
    pub fn set_resonator_model(&mut self, resonator_model: ResonatorModel) {
        self.resonator_model_ = resonator_model;
    }

    /// The exciter's envelope as it ended the last block, before the gain the shape gives it:
    /// for a host (its *Contour* source), not in the C++.
    #[inline]
    pub fn envelope(&self) -> f32 {
        self.envelope_.value()
    }

    /// This voice's random stream: for a host that wants to restart it, not in the C++.
    #[inline]
    pub fn random_mut(&mut self) -> &mut Random {
        &mut self.random
    }

    #[inline]
    fn get_gate_flags(&mut self, gate_in: bool) -> u8 {
        let mut flags = 0;
        if gate_in {
            if !self.previous_gate_ {
                flags |= EXCITER_FLAG_RISING_EDGE;
            }
            flags |= EXCITER_FLAG_GATE;
        } else if self.previous_gate_ {
            flags = EXCITER_FLAG_FALLING_EDGE;
        }
        self.previous_gate_ = gate_in;
        flags
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        patch: &Patch,
        frequency: f32,
        mut strength: f32,
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
        let mut envelope_gain = 1.0f32;
        if patch.exciter_envelope_shape < 0.4 {
            let a = patch.exciter_envelope_shape * 0.75 + 0.15;
            let dr = a * 1.8;
            self.envelope_.set_adsr(a, dr, 0.0, dr);
            envelope_gain = 5.0 - patch.exciter_envelope_shape * 10.0;
        } else if patch.exciter_envelope_shape < 0.6 {
            let s = (patch.exciter_envelope_shape - 0.4) * 5.0;
            self.envelope_.set_adsr(0.45, 0.81, s, 0.81);
        } else {
            let a = (1.0 - patch.exciter_envelope_shape) * 0.75 + 0.15;
            let dr = a * 1.8;
            self.envelope_.set_adsr(a, dr, 1.0, dr);
        }
        let envelope_value = self.envelope_.process(flags) * envelope_gain;
        let envelope_increment = (envelope_value - self.envelope_value_) / size as f32;

        // Configure and evaluate exciters.
        let brightness_factor = 0.4 + 0.6 * patch.resonator_brightness;
        self.bow_
            .set_timbre(patch.exciter_bow_timbre * brightness_factor);

        self.blow_.set_parameter(patch.exciter_blow_meta);
        self.blow_.set_timbre(patch.exciter_blow_timbre);
        self.blow_.set_signature(patch.exciter_signature);

        let strike_meta = patch.exciter_strike_meta;
        self.strike_.set_meta(
            if strike_meta <= 0.4 {
                strike_meta * 0.625
            } else {
                strike_meta * 1.25 - 0.25
            },
            EXCITER_MODEL_SAMPLE_PLAYER,
            EXCITER_MODEL_PARTICLES,
        );
        self.strike_.set_timbre(patch.exciter_strike_timbre);
        self.strike_.set_signature(patch.exciter_signature);

        self.bow_
            .process(flags, &mut self.bow_buffer_, size, &mut self.random);

        let mut blow_level = patch.exciter_blow_level * 1.5;
        let tube_level = if blow_level > 1.0 {
            (blow_level - 1.0) * 2.0
        } else {
            0.0
        };
        blow_level = if blow_level < 1.0 {
            blow_level * 0.4
        } else {
            0.4
        };
        self.blow_
            .process(flags, &mut self.blow_buffer_, size, &mut self.random);
        self.tube_.process(
            frequency,
            envelope_value,
            patch.resonator_damping,
            tube_level,
            &mut self.blow_buffer_,
            tube_level * 0.5,
            size,
        );

        for i in 0..size {
            self.blow_buffer_[i] = self.blow_buffer_[i] * blow_level + blow_in[i];
        }
        self.diffuser_.process(&mut self.blow_buffer_, size);
        self.strike_
            .process(flags, &mut self.strike_buffer_, size, &mut self.random);

        // The Strike exciter is implemented in such a way that raising the level beyond a
        // certain point doesn't change the exciter amplitude, but instead, increasingly mixes
        // the raw exciter signal into the resonator output.
        let mut strike_level = patch.exciter_strike_level * 1.25;
        let strike_bleed = if strike_level > 1.0 {
            (strike_level - 1.0) * 2.0
        } else {
            0.0
        };
        strike_level = if strike_level < 1.0 {
            strike_level
        } else {
            1.0
        };
        strike_level *= 1.5;

        // The strength parameter is very sensitive to zipper noise.
        strength *= 256.0;
        let strength_increment = (strength - self.strength_) / size as f32;

        // Sum all sources of excitation.
        for i in 0..size {
            self.strength_ += strength_increment;
            self.envelope_value_ += envelope_increment;
            let mut input_sample = 0.0f32;
            let mut e = self.envelope_value_;
            let (strength_lut_integral, strength_lut_fractional) =
                make_integral_fractional(self.strength_);
            let accent = at(&LUT_ACCENT_GAIN_COARSE, strength_lut_integral)
                * at(
                    &LUT_ACCENT_GAIN_FINE,
                    cast_i32(256.0 * strength_lut_fractional),
                );
            self.bow_strength_buffer_[i] = e * patch.exciter_bow_level;

            self.strike_buffer_[i] *= accent;
            e *= accent;

            input_sample += self.bow_buffer_[i] * self.bow_strength_buffer_[i] * 0.125 * accent;
            input_sample += self.blow_buffer_[i] * e;
            input_sample += self.strike_buffer_[i] * strike_level;
            input_sample += strike_in[i];

            raw[i] = input_sample * 0.5;
        }

        // Update meter for exciter.
        for raw in &raw[..size] {
            let error = raw * raw - self.exciter_level_;
            self.exciter_level_ += error * if error > 0.0 { 0.5 } else { 0.001 };
        }

        // Some exciters can cause palm mutes on release.
        let mut damping = patch.resonator_damping;
        damping -= self.strike_.damping() * strike_level * 0.125;
        damping -= (1.0 - self.bow_strength_buffer_[0]) * patch.exciter_bow_level * 0.0625;

        if damping <= 0.0 {
            damping = 0.0;
        }

        // Configure resonator.
        if self.resonator_model_ == RESONATOR_MODEL_MODAL {
            self.resonator_.set_frequency(frequency);
            self.resonator_.set_geometry(patch.resonator_geometry);
            self.resonator_.set_brightness(patch.resonator_brightness);
            self.resonator_.set_position(patch.resonator_position);
            self.resonator_.set_damping(damping);
            self.resonator_
                .set_modulation_frequency(patch.resonator_modulation_frequency);
            self.resonator_
                .set_modulation_offset(patch.resonator_modulation_offset);

            // Process through resonator.
            self.resonator_
                .process(&self.bow_strength_buffer_, raw, center, sides, size);
        } else {
            let num_notes = if self.resonator_model_ == RESONATOR_MODEL_STRING {
                1
            } else {
                K_NUM_STRINGS
            };
            let normalization = 1.0 / num_notes as f32;
            self.dc_blocker_.process(raw, size);
            for raw in &mut raw[..size] {
                *raw *= normalization;
            }

            let chord = patch.resonator_geometry * 10.0;
            let hysteresis = if chord > self.chord_index_ { -0.1 } else { 0.1 };
            let mut chord_index = cast_i32(chord + hysteresis + 0.5);
            constrain(&mut chord_index, 0, 10);
            self.chord_index_ = chord_index as f32;

            center[..size].fill(0.0);
            sides[..size].fill(0.0);
            for i in 0..num_notes {
                let transpose = CHORDS[chord_index as usize][i];
                let string = &mut self.string_[i];
                string.set_frequency(frequency * semitones_to_ratio(transpose));
                string.set_brightness(patch.resonator_brightness);
                string.set_position(patch.resonator_position);
                string.set_damping(damping);
                if num_notes == 1 {
                    string.set_dispersion(patch.resonator_geometry);
                } else {
                    let b = patch.resonator_brightness;
                    string.set_dispersion(if b < 0.5 { 0.0 } else { (b - 0.5) * -0.4 });
                }
                string.process(raw, center, sides, size, &mut self.random);
            }
            for i in 0..size {
                let left = center[i];
                let right = sides[i];
                center[i] = left - right;
                sides[i] = left + right;
            }
        }

        // This is where the raw mallet signal bleeds through the exciter output.
        for i in 0..size {
            center[i] += strike_bleed * self.strike_buffer_[i];
        }
    }
}
