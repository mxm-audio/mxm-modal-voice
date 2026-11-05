//! Ported from stmlib/dsp/cosine_oscillator.h: cosine oscillator, generating a cosine between
//! 0.0 and 1.0 with minimal CPU use.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The template mode is a const generic. `COSINE_OSCILLATOR_EXACT` calls `cosf`, which no
//! Elements code asks for (its resonator and its effects take `APPROXIMATE`) and the port does not take from a maths
//! library: instantiating it is a compile-time error.

pub type CosineOscillatorMode = u8;
pub const COSINE_OSCILLATOR_APPROXIMATE: CosineOscillatorMode = 0;
pub const COSINE_OSCILLATOR_EXACT: CosineOscillatorMode = 1;

#[derive(Default)]
pub struct CosineOscillator {
    y1_: f32,
    y0_: f32,
    iir_coefficient_: f32,
    initial_amplitude_: f32,
}

impl CosineOscillator {
    #[inline]
    pub fn init<const MODE: CosineOscillatorMode>(&mut self, frequency: f32) {
        const {
            assert!(
                MODE == COSINE_OSCILLATOR_APPROXIMATE,
                "COSINE_OSCILLATOR_EXACT needs cosf, which Elements never calls"
            )
        };
        self.init_approximate(frequency);
        self.start();
    }

    #[inline]
    pub fn init_approximate(&mut self, mut frequency: f32) {
        let mut sign = 16.0f32;
        frequency -= 0.25;
        if frequency < 0.0 {
            frequency = -frequency;
        } else if frequency > 0.5 {
            frequency -= 0.5;
        } else {
            sign = -16.0;
        }
        self.iir_coefficient_ = sign * frequency * (1.0 - 2.0 * frequency);
        self.initial_amplitude_ = self.iir_coefficient_ * 0.25;
    }

    #[inline]
    pub fn start(&mut self) {
        self.y1_ = self.initial_amplitude_;
        self.y0_ = 0.5;
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.y1_ + 0.5
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        let temp = self.y0_;
        self.y0_ = self.iir_coefficient_ * self.y0_ - self.y1_;
        self.y1_ = temp;
        temp + 0.5
    }
}
