//! Ported from elements/dsp/dsp.h: the rate and the block.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).

/// `kSampleRate`: the module's codec runs at 32 kHz, and so does the port; nothing changes it.
pub const K_SAMPLE_RATE: f32 = 32000.0;

/// `kMaxBlockSize`: the most samples a block may hold. The firmware renders 16 at a time
/// (`drivers/codec.h`'s `kAudioChunkSize`).
pub const K_MAX_BLOCK_SIZE: usize = 16;
