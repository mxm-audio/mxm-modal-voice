//! Ported from elements/dsp/part.{h,cc}: the part, the module's voice (or its hidden one) mixed
//! to stereo, soft-limited and through the reverb.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! What `Init` never sets (`panic_`, `easter_egg_`, `num_voices_`, the reverb's lows) starts at
//! the static object's zero: [`Part::new`] makes the part as the firmware's static one is before
//! `Init`. `Part::Panic` is declared in `part.h` and defined nowhere, so it is not ported.
//! `elements.cc` hands `Init` its static `reverb_buffer`; here the reverb owns that memory.

use crate::cpp::at_usize;
use crate::dsp::K_MAX_BLOCK_SIZE;
use crate::fx::reverb::Reverb;
use crate::ominous_voice::OminousVoice;
use crate::patch::Patch;
use crate::resources::{LUT_MIDI_TO_F_HIGH, LUT_MIDI_TO_F_LOW};
use crate::stmlib::dsp::dsp::{cast_i32, soft_limit};
use crate::voice::{RESONATOR_MODEL_MODAL, ResonatorModel, Voice};

/// `PerformanceState`: what `cv_scaler.cc` reads off the gate, the V/OCT, FM and pressure CVs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerformanceState {
    pub gate: bool,
    pub note: f32,
    pub modulation: f32,
    pub strength: f32,
}

/// Polyphony is actually possible, but you have to reduce the number of modes to 16, and this
/// doesn't sound very good...
pub const K_NUM_VOICES: usize = 1;

pub struct Part {
    patch_: Patch,
    voice_: [Box<Voice>; K_NUM_VOICES],
    ominous_voice_: [Box<OminousVoice>; K_NUM_VOICES],

    panic_: bool,
    bypass_: bool,
    easter_egg_: bool,

    previous_gate_: bool,
    note_: [f32; K_NUM_VOICES],
    active_voice_: usize,

    silence_: [f32; K_MAX_BLOCK_SIZE],
    raw_buffer_: [f32; K_MAX_BLOCK_SIZE],
    center_buffer_: [f32; K_MAX_BLOCK_SIZE],
    sides_buffer_: [f32; K_MAX_BLOCK_SIZE],

    scaled_exciter_level_: f32,
    scaled_resonator_level_: f32,
    resonator_level_: f32,

    reverb_: Box<Reverb>,

    resonator_model_: ResonatorModel,
}

impl Part {
    /// A part as the firmware's static one is before `Init`: every field zero. Its memory, the
    /// voice's buffers and the reverb's 64 KB, is allocated here, once.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            patch_: Patch::default(),
            voice_: std::array::from_fn(|_| Voice::new()),
            ominous_voice_: std::array::from_fn(|_| OminousVoice::new()),
            panic_: false,
            bypass_: false,
            easter_egg_: false,
            previous_gate_: false,
            note_: [0.0; K_NUM_VOICES],
            active_voice_: 0,
            silence_: [0.0; K_MAX_BLOCK_SIZE],
            raw_buffer_: [0.0; K_MAX_BLOCK_SIZE],
            center_buffer_: [0.0; K_MAX_BLOCK_SIZE],
            sides_buffer_: [0.0; K_MAX_BLOCK_SIZE],
            scaled_exciter_level_: 0.0,
            scaled_resonator_level_: 0.0,
            resonator_level_: 0.0,
            reverb_: Box::default(),
            resonator_model_: RESONATOR_MODEL_MODAL,
        })
    }

    pub fn init(&mut self) {
        self.patch_.exciter_envelope_shape = 1.0;
        self.patch_.exciter_bow_level = 0.0;
        self.patch_.exciter_bow_timbre = 0.5;
        self.patch_.exciter_blow_level = 0.0;
        self.patch_.exciter_blow_meta = 0.5;
        self.patch_.exciter_blow_timbre = 0.5;
        self.patch_.exciter_strike_level = 0.8;
        self.patch_.exciter_strike_meta = 0.5;
        self.patch_.exciter_strike_timbre = 0.5;
        self.patch_.exciter_signature = 0.0;
        self.patch_.resonator_geometry = 0.2;
        self.patch_.resonator_brightness = 0.5;
        self.patch_.resonator_damping = 0.25;
        self.patch_.resonator_position = 0.3;
        self.patch_.resonator_modulation_frequency = 0.5 / crate::dsp::K_SAMPLE_RATE;
        self.patch_.resonator_modulation_offset = 0.1;
        self.patch_.reverb_diffusion = 0.625;
        self.patch_.reverb_lp = 0.7;
        self.patch_.space = 0.5;
        self.previous_gate_ = false;
        self.active_voice_ = 0;

        self.silence_.fill(0.0);
        self.note_.fill(69.0);

        for i in 0..K_NUM_VOICES {
            self.voice_[i].init();
            self.ominous_voice_[i].init();
        }

        self.reverb_.init();

        self.scaled_exciter_level_ = 0.0;
        self.scaled_resonator_level_ = 0.0;
        self.resonator_level_ = 0.0;
        self.bypass_ = false;
        self.resonator_model_ = RESONATOR_MODEL_MODAL;
    }

    /// `Seed(uint32_t* seed, size_t size)`: the module's few random differences, from the chip's
    /// serial number.
    pub fn seed(&mut self, seed: &[u32]) {
        // Scramble all bits from the serial number.
        let mut signature: u32 = 0xf0cacc1a;
        for &word in seed {
            signature ^= word;
            signature = signature.wrapping_mul(1664525).wrapping_add(1013904223);
        }

        let mut next = || {
            let x = (signature & 7) as f32 / 8.0;
            signature >>= 3;
            x
        };
        let x = next();
        self.patch_.resonator_modulation_frequency = (0.4 + 0.8 * x) / crate::dsp::K_SAMPLE_RATE;
        let x = next();
        self.patch_.resonator_modulation_offset = 0.05 + 0.1 * x;
        let x = next();
        self.patch_.reverb_diffusion = 0.55 + 0.15 * x;
        let x = next();
        self.patch_.reverb_lp = 0.7 + 0.2 * x;
        let x = next();
        self.patch_.exciter_signature = x;
    }

    #[inline]
    pub fn patch(&self) -> &Patch {
        &self.patch_
    }

    #[inline]
    pub fn mutable_patch(&mut self) -> &mut Patch {
        &mut self.patch_
    }

    /// For metering.
    #[inline]
    pub fn exciter_level(&self) -> f32 {
        self.scaled_exciter_level_
    }

    #[inline]
    pub fn resonator_level(&self) -> f32 {
        self.scaled_resonator_level_
    }

    #[inline]
    pub fn gate(&self) -> bool {
        self.previous_gate_
    }

    #[inline]
    pub fn bypass(&self) -> bool {
        self.bypass_
    }

    #[inline]
    pub fn set_bypass(&mut self, bypass: bool) {
        self.bypass_ = bypass;
    }

    #[inline]
    pub fn easter_egg(&self) -> bool {
        self.easter_egg_
    }

    #[inline]
    pub fn set_easter_egg(&mut self, easter_egg: bool) {
        self.easter_egg_ = easter_egg;
    }

    #[inline]
    pub fn resonator_model(&self) -> ResonatorModel {
        self.resonator_model_
    }

    #[inline]
    pub fn set_resonator_model(&mut self, r: ResonatorModel) {
        self.resonator_model_ = r;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        performance_state: &PerformanceState,
        blow_in: &[f32],
        strike_in: &[f32],
        main: &mut [f32],
        aux: &mut [f32],
        size: usize,
    ) {
        // Copy inputs to outputs when bypass mode is enabled.
        if self.bypass_ || self.panic_ {
            if self.panic_ {
                // If the resonator is blowing up (this has been observed once before corrective
                // action was taken), reset the state of the filters to 0 to prevent the module
                // to freeze with resonators' state blocked at NaN.
                for voice in &mut self.voice_ {
                    voice.panic();
                }
                self.resonator_level_ = 0.0;
                self.panic_ = false;
            }
            aux[..size].copy_from_slice(&blow_in[..size]);
            main[..size].copy_from_slice(&strike_in[..size]);
            return;
        }

        // When a new note is played, cycle to the next voice.
        if performance_state.gate && !self.previous_gate_ {
            self.active_voice_ += 1;
            if self.active_voice_ >= K_NUM_VOICES {
                self.active_voice_ = 0;
            }
        }

        self.previous_gate_ = performance_state.gate;
        self.note_[self.active_voice_] = performance_state.note;
        main[..size].fill(0.0);
        aux[..size].fill(0.0);

        // Compute the raw signal gain, stereo spread, and reverb parameters from the "space"
        // metaparameter.
        let mut space = if self.patch_.space >= 1.0 {
            1.0
        } else {
            self.patch_.space
        };
        let raw_gain = if space <= 0.05 {
            1.0
        } else if space <= 0.1 {
            2.0 - space * 20.0
        } else {
            0.0
        };
        space = if space >= 0.1 { space - 0.1 } else { 0.0 };
        let spread = if space <= 0.7 { space } else { 0.7 };
        let reverb_amount = if space >= 0.5 {
            1.0 * (space - 0.5)
        } else {
            0.0
        };
        let reverb_time = 0.35 + 1.2 * reverb_amount;

        // Render each voice.
        for i in 0..K_NUM_VOICES {
            let midi_pitch = self.note_[i] + performance_state.modulation;
            let active = i == self.active_voice_;
            let (blow, strike): (&[f32], &[f32]) = if active {
                (blow_in, strike_in)
            } else {
                (&self.silence_, &self.silence_)
            };
            if self.easter_egg_ {
                self.ominous_voice_[i].process(
                    &self.patch_,
                    midi_pitch,
                    performance_state.strength,
                    active && performance_state.gate,
                    blow,
                    strike,
                    &mut self.raw_buffer_,
                    &mut self.center_buffer_,
                    &mut self.sides_buffer_,
                    size,
                );
            } else {
                // Convert the MIDI pitch to a frequency.
                let mut pitch = cast_i32((midi_pitch + 48.0) * 256.0);
                if pitch < 0 {
                    pitch = 0;
                } else if pitch >= 65535 {
                    pitch = 65535;
                }
                let pitch = pitch as usize;
                self.voice_[i].set_resonator_model(self.resonator_model_);
                // Render the voice signal.
                self.voice_[i].process(
                    &self.patch_,
                    at_usize(&LUT_MIDI_TO_F_HIGH, pitch >> 8)
                        * at_usize(&LUT_MIDI_TO_F_LOW, pitch & 0xff),
                    performance_state.strength,
                    active && performance_state.gate,
                    blow,
                    strike,
                    &mut self.raw_buffer_,
                    &mut self.center_buffer_,
                    &mut self.sides_buffer_,
                    size,
                );
            }

            // Mixdown.
            for j in 0..size {
                let side = self.sides_buffer_[j] * spread;
                let r = self.center_buffer_[j] - side;
                let l = self.center_buffer_[j] + side;
                main[j] += r;
                aux[j] += l + (self.raw_buffer_[j] - l) * raw_gain;
            }
        }

        // Pre-clipping
        if !self.easter_egg_ {
            for i in 0..size {
                main[i] = soft_limit(main[i]);
                aux[i] = soft_limit(aux[i]);
            }
        }

        // Metering.
        let mut exciter_level = self.voice_[self.active_voice_].exciter_level();
        let mut resonator_level = self.resonator_level_;
        for &main in &main[..size] {
            let error = main * main - resonator_level;
            resonator_level += error * if error > 0.0 { 0.05 } else { 0.0005 };
        }
        self.resonator_level_ = resonator_level;
        if resonator_level >= 200.0 {
            self.panic_ = true;
        }

        if self.easter_egg_ {
            let l = (self.patch_.exciter_blow_level + self.patch_.exciter_strike_level) * 0.5;
            self.scaled_exciter_level_ = l * (2.0 - l);
        } else {
            exciter_level *= 16.0;
            self.scaled_exciter_level_ = if exciter_level > 0.1 {
                1.0
            } else {
                exciter_level
            };
        }

        resonator_level *= 16.0;
        self.scaled_resonator_level_ = if resonator_level < 1.0 {
            resonator_level
        } else {
            1.0
        };

        // Apply reverb.
        self.reverb_.set_amount(reverb_amount);
        self.reverb_.set_diffusion(self.patch_.reverb_diffusion);
        let freeze = self.patch_.space >= 1.75;
        if freeze {
            self.reverb_.set_time(1.0);
            self.reverb_.set_input_gain(0.0);
            self.reverb_.set_lp(1.0);
        } else {
            self.reverb_.set_time(reverb_time);
            self.reverb_.set_input_gain(0.2);
            self.reverb_.set_lp(self.patch_.reverb_lp);
        }

        self.reverb_.process(main, aux, size);
    }
}
