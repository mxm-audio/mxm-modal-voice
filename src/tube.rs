//! Ported from elements/dsp/tube.{h,cc}: simple waveguide tube.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! `Init` never clears the delay line: it starts at the static object's zero and keeps what it
//! holds across a voice's `init`, as in the C++.

use crate::cpp::zeros;
use crate::stmlib::dsp::dsp::make_integral_fractional;
use crate::stmlib::stmlib::constrain;

pub const K_TUBE_DELAY_SIZE: usize = 2048;

pub struct Tube {
    delay_ptr_: i32,
    zero_state_: f32,
    pole_state_: f32,
    delay_line_: Box<[f32; K_TUBE_DELAY_SIZE]>,
}

impl Default for Tube {
    fn default() -> Self {
        Self {
            delay_ptr_: 0,
            zero_state_: 0.0,
            pole_state_: 0.0,
            delay_line_: zeros(),
        }
    }
}

/// `(int32_t) % kTubeDelaySize`: the `int32_t` converted to `size_t` before the `%`.
#[inline]
fn wrap(index: i32) -> usize {
    (index as i64 as u64 as usize) % K_TUBE_DELAY_SIZE
}

impl Tube {
    pub fn init(&mut self) {
        self.zero_state_ = 0.0;
        self.pole_state_ = 0.0;
        self.delay_ptr_ = 0;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        frequency: f32,
        mut envelope: f32,
        mut damping: f32,
        timbre: f32,
        input_output: &mut [f32],
        gain: f32,
        size: usize,
    ) {
        let mut delay = 1.0 / frequency;
        while delay >= K_TUBE_DELAY_SIZE as f32 {
            delay *= 0.5;
        }
        let (delay_integral, delay_fractional) = make_integral_fractional(delay);

        if envelope >= 1.0 {
            envelope = 1.0;
        }
        damping = 3.6 - damping * 1.8;
        let mut lpf_coefficient = frequency * (1.0 + timbre * timbre * 256.0);
        if lpf_coefficient >= 0.995 {
            lpf_coefficient = 0.995;
        }
        let mut d = self.delay_ptr_;
        for sample in &mut input_output[..size] {
            let breath = *sample * damping + 0.8;
            let a = self.delay_line_[wrap(d.wrapping_add(delay_integral))];
            let b = self.delay_line_[wrap(d.wrapping_add(delay_integral).wrapping_add(1))];
            let in_ = a + (b - a) * delay_fractional;
            let pressure_delta = -0.95 * (in_ * envelope + self.zero_state_) - breath;
            self.zero_state_ = in_;
            let reed = pressure_delta * -0.2 + 0.8;
            let mut out = pressure_delta * reed + breath;
            constrain(&mut out, -5.0, 5.0);
            self.delay_line_[wrap(d)] = out * 0.5;
            d -= 1;
            if d < 0 {
                d = K_TUBE_DELAY_SIZE as i32 - 1;
            }
            self.pole_state_ += lpf_coefficient * (out - self.pole_state_);
            *sample += gain * envelope * self.pole_state_;
        }
        self.delay_ptr_ = d;
    }
}
