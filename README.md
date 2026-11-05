# mxm-modal-voice

Elements' modal synthesis voice, its exciters, its three resonators, its reverb and its hidden
second voice, ported to Rust from Emilie Gillet's MIT code for Mutable Instruments Elements, and
**bit identical to the original**: every sample this crate renders equals, to the bit, the sample
Elements' own C++ renders from the same input.

This is not "sounds the same" or "within a tolerance". The test suite drives the Rust part and the
C++ part through 106 reference cases and compares both outputs, every 32-bit float as it leaves
the part, and the 16-bit frames the module makes of them, by SHA-256. Every case matches.
`cargo test` checks all 106 of them again.

## What bit identical means here

- **The original** is Elements' DSP as Emilie Gillet wrote it:
  [`pichenettes/eurorack`](https://github.com/pichenettes/eurorack) `elements/dsp/` at commit
  `f4cfa769`, with the parts of [`pichenettes/stmlib`](https://github.com/pichenettes/stmlib) it
  uses, at `e3bd7c9c`. It was built for x86-64 with MSVC (`/O2 /fp:precise`, flush-to-zero on)
  and run, untouched, by a harness that renders each case block by block through
  `Part::Process`.
- **A sample** is what `Part::Process` writes, main and aux, as `f32`: the module's voice mixed to
  stereo, soft-limited and through its reverb. The cases also compare the module's frames, the
  16-bit values `elements.cc` sends its codec (`SoftConvert`), so the port is held to the
  module's output too.
- **The cases** cover each resonator model (modal, string, strings): struck at four notes, bowed
  and blown with notes held, all three exciters at once, the note across its range, Strength,
  the modulation, every control swept from its floor to its ceiling, every control at its ends,
  both audio inputs, fast retriggers. The hidden voice has its own, and a few cross them: the
  model changing under a held note, the hidden voice turned on and off, the reverb frozen, three
  made serial numbers through `Part::Seed`, a four-second tail, and an input so loud the
  resonator blows up and the part's panic resets it. [`tests/cases/README.md`](tests/cases/README.md)
  has the format, and [`tests/digests.txt`](tests/digests.txt) the C++'s digests.
- **Nothing is changed, not even the rate.** Elements runs at 32 kHz, the rate its codec runs at,
  and so does this port: the harness builds Elements as it is.
- **Even what the C++ leaves unset is kept.** The firmware's part is a static object, so a field
  its `Init` never sets starts at zero, and carries on across an `Init`: the port starts it at
  zero too. Float-to-integer conversions behave as MSVC's do, `std::max` and `std::min` as C++
  evaluates them with a NaN, and every expression in the C++'s order and precision.
- **Each voice has its own random stream**, starting where Elements' one generator starts, so a
  voice draws exactly what the module's one voice draws.

The cases have been checked on Windows, x86-64. The port calls no floating-point library function
but `abs`, which is exact: the two `sinf` results the hidden voice's `Init` needs are the reference
build's own, kept as constants. So other platforms should give the same bits, but they have not
been checked yet. To render the reference's bits, run with flush-to-zero on, as the reference did.
The tests set it on x86-64. Without it the sound is the same to the ear, but a sample that passes
through a denormal can differ in its last bit.

## Using it

```rust
use mxm_modal_voice::dsp::K_MAX_BLOCK_SIZE;
use mxm_modal_voice::{Part, PerformanceState};

let mut part = Part::new(); // its voice and its reverb's memory, allocated once
part.init();
part.mutable_patch().exciter_strike_level = 0.8;
let state = PerformanceState { gate: true, note: 48.0, modulation: 0.0, strength: 0.8 };
let silence = [0.0; K_MAX_BLOCK_SIZE];
let (mut main, mut aux) = ([0.0; K_MAX_BLOCK_SIZE], [0.0; K_MAX_BLOCK_SIZE]);
part.process(&state, &silence, &silence, &mut main, &mut aux, K_MAX_BLOCK_SIZE);
// main is the module's right output, aux its left: 16 samples at 32 kHz
```

`Patch` and `PerformanceState` are Elements' own, field for field: what the knobs and the CVs
set, as the firmware's `cv_scaler.cc` writes them, and the gate, the note, the modulation and the
strength. The two input slices are Blow in and Strike in. `set_resonator_model` picks the
resonator (`RESONATOR_MODEL_MODAL`, `_STRING`, `_STRINGS`) and `set_easter_egg` the hidden voice,
as the module's hidden panel gestures do. Call `process` once per block of 16 samples, as the
module does. At other rates, resample.

- **Realtime safe:** no allocation after `Part::new`, no locks, no file IO, and no panic on any
  input; `#![forbid(unsafe_code)]`.
- **No dependencies.**
- **Polyphony** is many `Voice`s, each with its own random stream: the crate exposes `Voice`,
  `OminousVoice` and the reverb for a host that builds its own part. `Part` itself has one voice,
  as Elements does.
- Builds for WebAssembly (`wasm32-unknown-unknown`), so it can run in a web browser.
  WebAssembly has no flush-to-zero, so the last-bit caveat above applies there.

## What is in it

| | |
|---|---|
| Exciters | the bow; the blow, granular noise into a tube and a diffuser; the strike, sweeping through nine recorded hits, a mallet, a plectrum and particles; one envelope (Contour) over them |
| Resonators | modal (52 band-pass modes, eight of them bowed), string (one, with dispersion), strings (five, tuned to eleven chords) |
| Part | the stereo mix (Space: dry, spread, reverb, freeze), the soft limit, the reverb, the panic that resets a resonator that blows up, `Seed` |
| Hidden voice | `OminousVoice`, two FM oscillators through their filters into a spatializer |

The data the exciters play ships as Elements ships it: the nine hits, the noise and the tables.
Not ported: everything of the hardware (its panel, calibration, settings and drivers), and the
firmware's knob and CV scaling, which is a host's.

## How it is organised

One Rust module per C++ file, at the same path and in the same order of statements, so the port
reads line for line beside the original: `src/part.rs` is `elements/dsp/part.{h,cc}`,
`src/fx/reverb.rs` is `elements/dsp/fx/reverb.h`. The tables and samples are generated from
Elements' own compiled values by `gen/tables.py`, never recomputed.
[`AGENTS.md`](AGENTS.md) has the porting rules that keep it bit identical.

## Licence

MIT; see [`LICENSE`](LICENSE). The code it is ported from is Emilie Gillet's, under the MIT licence
in [`NOTICE.md`](NOTICE.md), and each ported file keeps its original's copyright line. Emilie Gillet
has no part in this crate and does not endorse it. Mutable Instruments is a registered trademark.
The MXM name is not covered by the licence.

Contributions are welcome; see [`CONTRIBUTING.md`](CONTRIBUTING.md).
