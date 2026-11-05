//! Elements' modal synthesis voice, ported to Rust from Emilie Gillet's MIT code
//! (`pichenettes/eurorack` `elements/dsp/` and the parts of `pichenettes/stmlib` it uses), held
//! bit for bit to that C++: see this crate's `AGENTS.md`.
//!
//! One module per C++ file, at the same path: `part` is `elements/dsp/part.{h,cc}`,
//! `fx::reverb` is `elements/dsp/fx/reverb.h`, `stmlib::dsp::filter` is `stmlib/dsp/filter.h`.
//! `cpp` holds what the port takes from C++ itself (`std::max`, `std::min`, an array read).
//!
//! The interface is the part's: [`Part::new`], [`Part::init`], its [`Patch`]
//! ([`Part::mutable_patch`]) and [`Part::process`], once per block of up to
//! [`dsp::K_MAX_BLOCK_SIZE`] samples at 32 kHz. The README's example:
//!
//! ```
//! use mxm_modal_voice::dsp::K_MAX_BLOCK_SIZE;
//! use mxm_modal_voice::{Part, PerformanceState};
//!
//! let mut part = Part::new(); // its voice and its reverb's memory, allocated once
//! part.init();
//! part.mutable_patch().exciter_strike_level = 0.8;
//! let state = PerformanceState { gate: true, note: 48.0, modulation: 0.0, strength: 0.8 };
//! let silence = [0.0; K_MAX_BLOCK_SIZE];
//! let (mut main, mut aux) = ([0.0; K_MAX_BLOCK_SIZE], [0.0; K_MAX_BLOCK_SIZE]);
//! part.process(&state, &silence, &silence, &mut main, &mut aux, K_MAX_BLOCK_SIZE);
//! // main is the module's right output, aux its left: 16 samples at 32 kHz
//! ```

#![forbid(unsafe_code)]
// Lints whose advice would have the port read unlike the C++ it is held to.
#![allow(
    // The C++'s float literals, digit for digit; Rust rounds each to f32 as MSVC does.
    clippy::excessive_precision,
    // `CONSTRAIN` and the C++'s comparisons, as written; `clamp` treats a NaN differently.
    clippy::manual_clamp,
    // One module per C++ file: `string.cc`'s `String` keeps its name.
    clippy::module_inception,
    // `Next()` is `next`, a method of the C++ class, not an iterator.
    clippy::should_implement_trait,
    // The C++'s loops over parallel buffers by index, so the port reads beside it.
    clippy::needless_range_loop,
    // `new` returns the boxed object, as a static one is: there is no stack-sized `Default` to
    // suggest.
    clippy::new_without_default
)]

pub mod cpp;
pub mod stmlib;

pub mod dsp;
pub mod exciter;
pub mod fx;
pub mod multistage_envelope;
pub mod ominous_voice;
pub mod part;
pub mod patch;
pub mod resonator;
pub mod resources;
pub mod string;
pub mod tube;
pub mod voice;

pub use ominous_voice::OminousVoice;
pub use part::{K_NUM_VOICES, Part, PerformanceState};
pub use patch::Patch;
pub use voice::{
    RESONATOR_MODEL_MODAL, RESONATOR_MODEL_STRING, RESONATOR_MODEL_STRINGS, ResonatorModel, Voice,
};
