//! `set_bowed_loop_limit`, for a host and not in the C++: on, a struck mode above 0.2304 of the
//! rate, which rings on without end in the C++, dies away; below it the voice renders the C++'s
//! bits, struck and bowed; and `clear` turns it off.

use mxm_modal_voice::dsp::K_MAX_BLOCK_SIZE;
use mxm_modal_voice::{Patch, Voice};

/// Three seconds of blocks at 32 kHz.
const BLOCKS: usize = 6_000;

/// A struck plate, nearly harmonic (Geometry 0.02), bright: no bow at all.
fn plate() -> Patch {
    Patch {
        exciter_envelope_shape: 0.9995,
        exciter_strike_level: 0.8,
        exciter_strike_meta: 0.54,
        exciter_strike_timbre: 0.7,
        resonator_geometry: 0.02,
        resonator_brightness: 0.8,
        resonator_damping: 0.5,
        resonator_position: 0.25,
        resonator_modulation_frequency: 0.5 / 32000.0,
        resonator_modulation_offset: 0.1,
        reverb_diffusion: 0.625,
        reverb_lp: 0.7,
        space: 0.55,
        ..Patch::default()
    }
}

/// A MIDI note as a frequency, of the rate.
fn frequency(note: f32) -> f32 {
    440.0 / 32000.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// `BLOCKS` blocks of `note`, the gate held for the first 0.4 s: the centre output, as bits.
fn play(voice: &mut Voice, patch: &Patch, note: f32) -> Vec<u32> {
    let silence = [0.0; K_MAX_BLOCK_SIZE];
    let (mut raw, mut center, mut sides) = ([0.0; 16], [0.0; 16], [0.0; 16]);
    let mut out = Vec::with_capacity(BLOCKS * 16);
    for block in 0..BLOCKS {
        let gate = (1..800).contains(&block);
        voice.process(
            patch,
            frequency(note),
            0.5,
            gate,
            &silence,
            &silence,
            &mut raw,
            &mut center,
            &mut sides,
            16,
        );
        out.extend(center.iter().chain(&sides).map(|x| x.to_bits()));
    }
    out
}

fn voice(limit: bool) -> Box<Voice> {
    let mut voice = Voice::new();
    voice.init();
    voice.set_bowed_loop_limit(limit);
    voice
}

/// The level (dB) of the samples between two times (s).
fn level(out: &[u32], from: f32, to: f32) -> f32 {
    let (a, b) = ((from * 32000.0) as usize * 2, (to * 32000.0) as usize * 2);
    let sum: f64 = out[a..b]
        .iter()
        .map(|x| f64::from(f32::from_bits(*x)).powi(2))
        .sum();
    (10.0 * (sum / (b - a) as f64 + 1e-30).log10()) as f32
}

/// B6 on the plate: its four highest bowed modes are above the limit (7.8 to 10.6 kHz), and the
/// C++ runs away (the third second louder than the second); under the limit it dies away.
#[test]
fn a_struck_mode_above_the_limit_dies_away() {
    let note = 95.0;
    let elements = play(&mut voice(false), &plate(), note);
    let (first, last) = (level(&elements, 1.0, 2.0), level(&elements, 2.0, 3.0));
    assert!(
        last > first + 10.0,
        "the C++ at B6: {first:.1} dB, then {last:.1}"
    );
    let limited = play(&mut voice(true), &plate(), note);
    let (first, last) = (level(&limited, 1.0, 2.0), level(&limited, 2.0, 3.0));
    assert!(
        last < first - 20.0,
        "limited at B6: {first:.1} dB, then {last:.1}"
    );
}

/// Below 0.2304 of the rate the limit changes nothing: struck and bowed, every bit the C++'s.
#[test]
fn below_the_limit_every_bit_is_the_cpp() {
    let bowed = Patch {
        exciter_bow_level: 0.7,
        exciter_bow_timbre: 0.4,
        resonator_geometry: 0.35,
        ..plate()
    };
    for (patch, note) in [(plate(), 60.0), (bowed, 48.0)] {
        assert_eq!(
            play(&mut voice(true), &patch, note),
            play(&mut voice(false), &patch, note),
            "note {note}"
        );
    }
}

/// `clear` turns the limit off: a cleared voice is a new one, the C++'s.
#[test]
fn a_cleared_voice_has_no_limit() {
    let mut cleared = voice(true);
    cleared.clear();
    cleared.init();
    assert_eq!(
        play(&mut cleared, &plate(), 95.0),
        play(&mut voice(false), &plate(), 95.0)
    );
}
