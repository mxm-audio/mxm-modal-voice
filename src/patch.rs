//! Ported from elements/dsp/patch.h: the patch parameters.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).

/// `Patch`: what the module's knobs and CVs set, as `cv_scaler.cc` writes them; `Part::init`
/// gives the defaults and `Part::seed` the four the chip's serial number varies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Patch {
    pub exciter_envelope_shape: f32,
    pub exciter_bow_level: f32,
    pub exciter_bow_timbre: f32,
    pub exciter_blow_level: f32,
    pub exciter_blow_meta: f32,
    pub exciter_blow_timbre: f32,
    pub exciter_strike_level: f32,
    pub exciter_strike_meta: f32,
    pub exciter_strike_timbre: f32,
    pub exciter_signature: f32,
    pub resonator_geometry: f32,
    pub resonator_brightness: f32,
    pub resonator_damping: f32,
    pub resonator_position: f32,
    pub resonator_modulation_frequency: f32,
    pub resonator_modulation_offset: f32,
    pub reverb_diffusion: f32,
    pub reverb_lp: f32,
    pub space: f32,
    pub modulation_frequency: f32,
}
