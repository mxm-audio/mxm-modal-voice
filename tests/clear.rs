//! `clear`, for a host's reset: a voice played, cleared and played again renders what a new one
//! renders, to the bit, every resonator model and the hidden voice.

use mxm_modal_voice::dsp::K_MAX_BLOCK_SIZE;
use mxm_modal_voice::{OminousVoice, Patch, Voice};

const BLOCKS: usize = 400;

fn patch() -> Patch {
    Patch {
        exciter_envelope_shape: 0.3,
        exciter_bow_level: 0.6,
        exciter_bow_timbre: 0.4,
        exciter_blow_level: 0.8,
        exciter_blow_meta: 0.3,
        exciter_blow_timbre: 0.6,
        exciter_strike_level: 0.7,
        exciter_strike_meta: 0.2,
        exciter_strike_timbre: 0.5,
        resonator_geometry: 0.35,
        resonator_brightness: 0.6,
        resonator_damping: 0.5,
        resonator_position: 0.3,
        resonator_modulation_frequency: 0.5 / 32000.0,
        resonator_modulation_offset: 0.1,
        reverb_diffusion: 0.625,
        reverb_lp: 0.7,
        space: 0.5,
        ..Patch::default()
    }
}

/// `BLOCKS` blocks of a gate held for half of them, through `process`, as bits.
fn play(mut process: impl FnMut(bool, &mut [f32; 48])) -> Vec<u32> {
    let mut out = Vec::new();
    for block in 0..BLOCKS {
        let mut buffers = [0.0f32; 48];
        process(block < BLOCKS / 2, &mut buffers);
        out.extend(buffers.iter().map(|x| x.to_bits()));
    }
    out
}

fn voice_play(voice: &mut Voice, model: u8) -> Vec<u32> {
    voice.init();
    voice.set_resonator_model(model);
    let silence = [0.0; K_MAX_BLOCK_SIZE];
    let patch = patch();
    play(|gate, b| {
        let (raw, rest) = b.split_at_mut(16);
        let (center, sides) = rest.split_at_mut(16);
        voice.process(
            &patch, 0.01, 0.6, gate, &silence, &silence, raw, center, sides, 16,
        );
    })
}

#[test]
fn a_cleared_voice_is_a_new_voice() {
    for model in 0..3 {
        let fresh = voice_play(&mut Voice::new(), model);
        let mut voice = Voice::new();
        voice_play(&mut voice, model);
        voice.clear();
        assert_eq!(voice_play(&mut voice, model), fresh, "model {model}");
    }
}

#[test]
fn a_cleared_hidden_voice_is_a_new_one() {
    let run = |voice: &mut OminousVoice| {
        voice.init();
        let silence = [0.0; K_MAX_BLOCK_SIZE];
        let patch = patch();
        play(|gate, b| {
            let (raw, rest) = b.split_at_mut(16);
            let (center, sides) = rest.split_at_mut(16);
            voice.process(
                &patch, 48.0, 0.6, gate, &silence, &silence, raw, center, sides, 16,
            );
        })
    };
    let fresh = run(&mut OminousVoice::new());
    let mut voice = OminousVoice::new();
    run(&mut voice);
    voice.clear();
    assert_eq!(run(&mut voice), fresh);
}
