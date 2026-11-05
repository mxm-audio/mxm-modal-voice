# The reference cases

Each `.case` file drives one Elements part from its first block; `../digests.txt` holds the
SHA-256 of what Elements' own C++ renders for it (built with MSVC, x64, `/O2 /fp:precise`,
flush-to-zero on; nothing changed). For each case two renders: `<case>.f32`, `Part::Process`'s
two outputs as they come, interleaved main and aux, little-endian `f32`; and `<case>.raw`, the
module's frames made from them as `elements.cc` makes them, interleaved left `SoftConvert(aux)`
and right `SoftConvert(main)`, little-endian `int16`. One second (2000 blocks of 16 at 32 kHz) a
case unless it says otherwise. The C++ part is static, so every field its `Init` leaves unset is
zero, and starts each case fresh, its random state at its initial value.

## A case

Text, one directive a line, `#` to the line's end a comment:

| Directive | Meaning |
|---|---|
| `blocks <n>` | Render *n* blocks |
| `set <field> <value>` | The field takes the value at once, after `Part::Init` |
| `at <block> <field> <value>` | The field takes the value at the start of that block |
| `ramp <b0> <b1> <field> <v0> <v1>` | At the start of each block *b* from *b0* to *b1*, the field takes `v0 + (v1 − v0) × t` with `t = (b − b0) as f32 / (b1 − b0) as f32`, all in f32 |
| `seed <w0> <w1> <w2>` | `Part::Seed` with these three words, at once |
| `input <block> <which> <kind> <amp> <period>` | From that block, Blow in (`blow`) or Strike in (`strike`) plays `off`, `noise` or `pulse` |

Fields are `Patch`'s members as `patch.<name>`, `PerformanceState`'s as `perf.<name>`, and
`part.model` (the resonator model, 0 modal, 1 string, 2 strings, truncated to an integer) and
`part.easter_egg` (the hidden voice), the part's setters; a bool is true unless the value is 0.
Values are parsed as f32 (C's `%f`). `set`, `seed` and the setters act in the order they are
written; `perf` starts at zero. Each block, the `at` and `ramp` directives that cover it act in
the order they are written, then the inputs that start at it, then the block renders.

An input's `noise` is its own generator, `state = state × 1664525 + 1013904223` (Blow's from 1,
Strike's from 2), each sample `(int16)(state >> 16) / 32768 × amp`; its `pulse` is `amp` at every
sample index (counted from the case's start) that is a multiple of `period`, and 0 elsewhere.

## The cases

For each resonator model `M` from 0 to 2:

| Case | Covers |
|---|---|
| `mM-strike` | struck at four notes, `Init`'s patch |
| `mM-bow`, `mM-blow` | bowed, blown, two notes held 0.4 s each |
| `mM-all` | bowed, blown and struck at once |
| `mM-notes`, `mM-strength` | struck, the note from 24 to 96; Strength from 0 to 1 |
| `mM-fm` | bowed and held, the modulation from −24 to 24 |
| `mM-edges` | every control at 0, then at its ceiling |
| `mM-inputs` | no exciter: Strike in pulsed and Blow in noise, held |
| `mM-retrigger` | struck fast, a gate every 40 blocks |
| `mM-<control>` | each control swept from its floor to its ceiling, gated: contour, the bow's timbre, flow, the blow's timbre, mallet, the strike's timbre and level, geometry, brightness, damping, position, space (to 2, past the freeze), signature, the resonator's modulation frequency and offset, the reverb's diffusion and colour |

For the hidden voice, `egg-held`, `-notes`, `-fm`, `-inputs`, `-edges` and twelve control sweeps.
And `model-change` (a held note while the model steps 0, 1, 2, 0), `egg-change` (the hidden voice
turned on and off under held notes), `freeze` (Space past 1.75 and back), `seed-a`, `-b`, `-c`
(`Part::Seed` with three made serials), `tail` (four seconds, the reverb's long tail) and `panic`
(a Strike in too loud for the resonator: `Part::Process` resets it, again and again). 106 cases.
The values a case gives a control stay in the ranges the firmware's `cv_scaler.cc` gives it.
