# NOTES — mxm-modal-voice

The reasoning behind [AGENTS.md](AGENTS.md) and the owner's rulings, dated.

## Its own repository (2026-11-05)

The owner, on 2026-11-05: *"It could be great to do Elements the exact same way that we did
plaits. Make a plan for that. Same modules and all."* and *"And a free rust crate too"*. So the
port is its own MIT crate from the start, as mxm-macro-osc became once Plaits' was done, and an
MXM instrument plays it (MXM Modal Synth). The plan is ops' `plans/plan-elements.md`.

- **The name**: Elements is Mutable's *modal synthesizer*, as Plaits its *macro oscillator*, so
  `mxm-modal-voice` beside `mxm-macro-osc`; "Elements" and "Mutable Instruments" appear in the
  README, the notice and these docs, never in a product's name (the owner, 2026-11-05: the
  instrument is MXM Modal Synth, the crate `mxm-modal-voice`).
- **MIT**, as the code it is ported from and the MXM kit are.
- **The samples are MIT, as the code they are published with** (the owner, 2026-11-05: *"they
  are released with the projects so we considder them MIT as well."*).
- **Public since 2026-11-05**, every case passing, on the owner's word (*"Publish both"*, with MXM
  Modal Synth, which takes this crate from GitHub), as mxm-macro-osc went public once done.

## Why bit identity holds more easily than for Plaits

No maths library function runs while Elements plays: its filters take `FREQUENCY_FAST` and
`_ACCURATE`, its cosine oscillators `APPROXIMATE`. Only the hidden voice's `Init` calls `sinf`, on
two constants, and the reference's results are kept, so the claim needs no exception. And nothing
is changed: Plaits tunes to its DAC's odd rate, so its port changed one constant; Elements runs at
32 kHz exactly. Every case passed on the port's first run (2026-11-05).
