//! Ported from stmlib/utils/random.{h,cc}: fast 16-bit pseudo random number generator.
//!
//! Copyright 2012 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ class has only static members: one generator for the whole module. Here it is a
//! value, one per voice, passed by `&mut` to whatever draws from it in the C++'s order; its
//! state starts where the static one does, at `0x21`, so a voice draws exactly what the
//! module's one voice draws.

pub struct Random {
    rng_state_: u32,
}

impl Default for Random {
    /// `uint32_t Random::rng_state_ = 0x21;`.
    fn default() -> Self {
        Self { rng_state_: 0x21 }
    }
}

impl Random {
    #[inline]
    pub fn state(&self) -> u32 {
        self.rng_state_
    }

    #[inline]
    pub fn seed(&mut self, seed: u32) {
        self.rng_state_ = seed;
    }

    /// `rng_state_ * 1664525L + 1013904223L`, modulo 2³² (`long` is 32 bits on Windows, and the
    /// result is a `uint32_t` on every platform).
    #[inline]
    pub fn get_word(&mut self) -> u32 {
        self.rng_state_ = self
            .rng_state_
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        self.state()
    }

    #[inline]
    pub fn get_sample(&mut self) -> i16 {
        (self.get_word() >> 16) as i16
    }

    /// `static_cast<float>(GetWord()) / 4294967296.0f`: the word rounds to nearest, so the
    /// result can be exactly 1.0.
    #[inline]
    pub fn get_float(&mut self) -> f32 {
        self.get_word() as f32 / 4294967296.0
    }
}
