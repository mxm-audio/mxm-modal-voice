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

## The bowed loops' limit (2026-11-13)

The owner heard a toy piano in a song (MXM Modal Synth, *Tin plate keys*, around D7) whistle
on at 10 to 12 kHz, and ruled: *"i feel we need to fix this at the source. How much will it change
the sound?"*, then, on the measurements below, *"Yes do it that way"*. So the fix is here, as an
addition for a host, off unless turned on, and MXM Modal Synth turns it on.

**Why it runs away.** Each of the eight bowed modes is a loop: its delay read times `0.99f`, into a
band-pass of unity gain at the mode, back into the delay. The bow table, added to the input a sample
later, feeds back what the delays held times its slope; with the bow quiet it sits at its clamp,
0.245, a feedback of `-0.245 z^-1`. At the mode the loop's gain is `0.99 |1 - 0.245 e^-jw|`, which
passes 1 above 0.2304 of the rate (7.37 kHz). A struck mode up there grows instead of decaying, until
it fills the output. The C++ does exactly this; the port, bit for bit, did too.

**The limit** divides the 0.99 by `|1 - 0.245 e^-jw|` at each bowed mode's frequency, where that is
above 1, so the loop's gain is the 0.99 the C++ means; below 7.37 kHz it is 0.99 unchanged and every
bit is the C++'s. The other ways to stop it change more: turning the bowed modes off while the bow
is silent changes every struck and blown sound (they colour the strike through the bow table,
5 to 44 dB under the sound); dropping the bowed modes above 7.4 kHz changes those notes 30 to 60 dB
under, against the limit's 53 to 93.

**Measured** (2026-11-13, MXM Modal Synth's 30 factory sounds on the modal resonator, single notes
C0 to C9 in quarter tones, 12 990 renders against the C++, the limit's cosine then the maths
library's, which the table's matches to under 10⁻⁶): unlimited, 27 of them run away at some
pitch, from E3 up in most and from F♯2 in the brightest; limited, none. Limited, 58 % of the
renders are bit for bit the C++'s, 98 % below C3 and 77 % from C3 to B4; above C6 almost every note
differs, but by 50 to 90 dB under the sound. What can be heard is one high mode near 7.5 to 8 kHz
that the C++ lets ring on for seconds, just short of running away, dying with the others instead (up
to 26 to 30 dB under the sound, on a few high notes).

