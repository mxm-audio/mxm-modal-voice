# Notice

mxm-modal-voice — Copyright (c) 2026 mxm. MIT licence: see `LICENSE`. It comes with no warranty.
The MXM name is not covered by that licence.

## The code it is ported from

**It is ported from Emilie Gillet's code for Mutable Instruments Elements** (`elements/` in
[pichenettes/eurorack](https://github.com/pichenettes/eurorack), commit
`f4cfa7697095a28d292bff758beb51deb49b5a86`) and the parts of
[pichenettes/stmlib](https://github.com/pichenettes/stmlib) it uses (commit
`e3bd7c9cc00e4364166f9905c0509b6ffd0535ec`), under the MIT licence below. Each ported file keeps
the copyright line of the file it was ported from. Mutable Instruments is a registered trademark;
Emilie Gillet has no part in this crate and does not endorse it.

**The data the exciters play is Elements'**, as Elements ships it, published with its code and
under the same licence:

- **The nine strike recordings** the Mallet control sweeps through (`elements/samples/hit_01.wav`
  to `hit_09.wav`, summed to mono), and **the noise** the blow exciter plays
  (`elements/samples/noise.wav`), as `elements/resources/samples.py` writes them into
  `elements/resources.cc`.
- **The tables** `elements/resources/lookup_tables.py` computes, as `resources.cc` holds them.

## Emilie Gillet's licence

```text
Copyright Emilie Gillet (emilie.o.gillet@gmail.com), the years in each file's header.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Its dependencies

None at run time; for its tests, `sha2` (MIT or Apache-2.0).
