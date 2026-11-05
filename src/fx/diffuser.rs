//! Ported from elements/dsp/fx/diffuser.h: granular diffuser.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The diffuser's engine never sets its LFOs' frequencies, so they stay at the static object's
//! zero, and `start` steps them through zeros, as in the C++.

use super::fx_engine::{DelayLine, Format32, FxEngine, TAIL, delay_lines};

type E = FxEngine<1024, Format32>;

/// `Reserve<126, Reserve<180, Reserve<269, Reserve<444>>>>`.
const MEMORY: [DelayLine; 4] = delay_lines([126, 180, 269, 444]);

#[derive(Default)]
pub struct Diffuser {
    engine_: E,
}

impl Diffuser {
    pub fn init(&mut self) {
        self.engine_.init();
    }

    pub fn process(&mut self, in_out: &mut [f32], size: usize) {
        let [ap1, ap2, ap3, ap4] = MEMORY;
        let kap = 0.625f32;
        for sample in &mut in_out[..size] {
            let mut c = self.engine_.start();
            c.read_value_once(*sample);
            c.read(ap1, TAIL, kap);
            c.write_all_pass(ap1, 0, -kap);
            c.read(ap2, TAIL, kap);
            c.write_all_pass(ap2, 0, -kap);
            c.read(ap3, TAIL, kap);
            c.write_all_pass(ap3, 0, -kap);
            c.read(ap4, TAIL, kap);
            c.write_all_pass(ap4, 0, -kap);
            c.write_value_scaled(sample, 0.0);
        }
    }
}
