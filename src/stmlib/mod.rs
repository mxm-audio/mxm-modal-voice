//! The parts of `pichenettes/stmlib` Elements uses: `dsp/` (cosine oscillator, delay line, DSP
//! routines, filters, parameter interpolator, units), `utils/random` and `stmlib.h`. Not ported:
//! `system/` (the hardware's flash, storage and clock), `ui/` and `utils/ring_buffer`, which
//! only the firmware around the part includes.

pub mod dsp;
pub mod stmlib;
pub mod utils;
