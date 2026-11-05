//! Ported from elements/dsp/fx/reverb.h: the reverb.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `lp_decay_1_` and `lp_decay_2_`, which `Init` never sets, start at the static object's zero.
//! `amount_`, `input_gain_` and `reverb_time_` are set by the part before every block.

use super::fx_engine::{DelayLine, Format16, FxEngine, LFO_1, LFO_2, TAIL, delay_lines};

type E = FxEngine<32768, Format16>;

/// `Reserve<150, … Reserve<6312>>`: the four input allpasses, then each half of the loop's two
/// allpasses and its delay.
const MEMORY: [DelayLine; 10] =
    delay_lines([150, 214, 319, 527, 2182, 2690, 4501, 2525, 2197, 6312]);

#[derive(Default)]
pub struct Reverb {
    engine_: E,
    amount_: f32,
    input_gain_: f32,
    reverb_time_: f32,
    diffusion_: f32,
    lp_: f32,
    lp_decay_1_: f32,
    lp_decay_2_: f32,
}

impl Reverb {
    pub fn init(&mut self) {
        self.engine_.init();
        self.engine_.set_lfo_frequency(LFO_1, 0.5 / 32000.0);
        self.engine_.set_lfo_frequency(LFO_2, 0.3 / 32000.0);
        self.lp_ = 0.7;
        self.diffusion_ = 0.625;
    }

    /// Every field as the static object's zero, its memory too, allocating nothing: for a host,
    /// not in the C++. `init` after it is the reverb as `Part::Init` leaves it.
    pub fn clear(&mut self) {
        self.engine_.clear();
        self.amount_ = 0.0;
        self.input_gain_ = 0.0;
        self.reverb_time_ = 0.0;
        self.diffusion_ = 0.0;
        self.lp_ = 0.0;
        self.lp_decay_1_ = 0.0;
        self.lp_decay_2_ = 0.0;
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], size: usize) {
        // This is the Griesinger topology described in the Dattorro paper (4 AP diffusers on the
        // input, then a loop of 2x 2AP+1Delay). Modulation is applied in the loop of the first
        // diffuser AP for additional smearing; and to the two long delays for a slow
        // shimmer/chorus effect.
        let [ap1, ap2, ap3, ap4, dap1a, dap1b, del1, dap2a, dap2b, del2] = MEMORY;

        let kap = self.diffusion_;
        let klp = self.lp_;
        let krt = self.reverb_time_;
        let amount = self.amount_;
        let gain = self.input_gain_;

        let mut lp_1 = self.lp_decay_1_;
        let mut lp_2 = self.lp_decay_2_;

        for (left, right) in left[..size].iter_mut().zip(&mut right[..size]) {
            let mut wet = 0.0f32;
            let mut apout = 0.0f32;
            let mut c = self.engine_.start();

            // Smear AP1 inside the loop.
            c.interpolate_lfo(ap1, 10.0, LFO_1, 80.0, 1.0);
            c.write(ap1, 100, 0.0);

            c.read_value(*left + *right, gain);

            // Diffuse through 4 allpasses.
            c.read(ap1, TAIL, kap);
            c.write_all_pass(ap1, 0, -kap);
            c.read(ap2, TAIL, kap);
            c.write_all_pass(ap2, 0, -kap);
            c.read(ap3, TAIL, kap);
            c.write_all_pass(ap3, 0, -kap);
            c.read(ap4, TAIL, kap);
            c.write_all_pass(ap4, 0, -kap);
            c.write_value(&mut apout);

            // Main reverb loop.
            c.load(apout);
            c.interpolate_lfo(del2, 6211.0, LFO_2, 100.0, krt);
            c.lp(&mut lp_1, klp);
            c.read(dap1a, TAIL, -kap);
            c.write_all_pass(dap1a, 0, kap);
            c.read(dap1b, TAIL, kap);
            c.write_all_pass(dap1b, 0, -kap);
            c.write(del1, 0, 2.0);
            c.write_value_scaled(&mut wet, 0.0);

            *left += (wet - *left) * amount;

            c.load(apout);
            c.read(del1, TAIL, krt);
            c.lp(&mut lp_2, klp);
            c.read(dap2a, TAIL, kap);
            c.write_all_pass(dap2a, 0, -kap);
            c.read(dap2b, TAIL, -kap);
            c.write_all_pass(dap2b, 0, kap);
            c.write(del2, 0, 2.0);
            c.write_value_scaled(&mut wet, 0.0);

            *right += (wet - *right) * amount;
        }

        self.lp_decay_1_ = lp_1;
        self.lp_decay_2_ = lp_2;
    }

    #[inline]
    pub fn set_amount(&mut self, amount: f32) {
        self.amount_ = amount;
    }

    #[inline]
    pub fn set_input_gain(&mut self, input_gain: f32) {
        self.input_gain_ = input_gain;
    }

    #[inline]
    pub fn set_time(&mut self, reverb_time: f32) {
        self.reverb_time_ = reverb_time;
    }

    #[inline]
    pub fn set_diffusion(&mut self, diffusion: f32) {
        self.diffusion_ = diffusion;
    }

    #[inline]
    pub fn set_lp(&mut self, lp: f32) {
        self.lp_ = lp;
    }
}
