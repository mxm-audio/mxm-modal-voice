//! Ported from stmlib/dsp/delay_line.h: delay line.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The template's element type is `float` wherever Elements instantiates it, so the port is for
//! `f32` alone; `max_delay` is a const generic. The C++ keeps the line inside the object; here it
//! is on the heap, allocated once when the line is made, zero as a static object's is. The C++
//! overloads of `Read` are named for their arguments: `read` (the set delay), `read_at` (a
//! `size_t` delay), `read_f` (a fractional one).
//!
//! The index arithmetic is the C++'s: `write_ptr_` is a `size_t`, and an `int32_t` added to it
//! is converted to `size_t` first (a negative one wraps), before the `%`.

use super::dsp::make_integral_fractional;
use crate::cpp::zeros;

pub struct DelayLine<const MAX_DELAY: usize> {
    write_ptr_: usize,
    delay_: usize,
    line_: Box<[f32; MAX_DELAY]>,
}

impl<const MAX_DELAY: usize> Default for DelayLine<MAX_DELAY> {
    fn default() -> Self {
        Self {
            write_ptr_: 0,
            delay_: 0,
            line_: zeros(),
        }
    }
}

/// `size_t + int32_t`, as C++ converts the `int32_t`.
#[inline]
fn offset(base: usize, by: i32) -> usize {
    base.wrapping_add(by as i64 as u64 as usize)
}

impl<const MAX_DELAY: usize> DelayLine<MAX_DELAY> {
    pub fn init(&mut self) {
        self.reset();
    }

    pub fn reset(&mut self) {
        self.line_.fill(0.0);
        self.delay_ = 1;
        self.write_ptr_ = 0;
    }

    #[inline]
    pub fn set_delay(&mut self, delay: usize) {
        self.delay_ = delay;
    }

    #[inline]
    pub fn write(&mut self, sample: f32) {
        self.line_[self.write_ptr_] = sample;
        self.write_ptr_ = self.write_ptr_.wrapping_sub(1).wrapping_add(MAX_DELAY) % MAX_DELAY;
    }

    #[inline]
    pub fn allpass(&mut self, sample: f32, delay: usize, coefficient: f32) -> f32 {
        let read = self.line_[self.write_ptr_.wrapping_add(delay) % MAX_DELAY];
        let write = sample + coefficient * read;
        self.write(write);
        -write * coefficient + read
    }

    #[inline]
    pub fn write_read(&mut self, sample: f32, delay: f32) -> f32 {
        self.write(sample);
        self.read_f(delay)
    }

    #[inline]
    pub fn read(&self) -> f32 {
        self.line_[self.write_ptr_.wrapping_add(self.delay_) % MAX_DELAY]
    }

    #[inline]
    pub fn read_at(&self, delay: usize) -> f32 {
        self.line_[self.write_ptr_.wrapping_add(delay) % MAX_DELAY]
    }

    #[inline]
    pub fn read_f(&self, delay: f32) -> f32 {
        let (delay_integral, delay_fractional) = make_integral_fractional(delay);
        let a = self.line_[offset(self.write_ptr_, delay_integral) % MAX_DELAY];
        let b = self.line_[offset(self.write_ptr_, delay_integral).wrapping_add(1) % MAX_DELAY];
        a + (b - a) * delay_fractional
    }

    #[inline]
    pub fn read_hermite(&self, delay: f32) -> f32 {
        let (delay_integral, delay_fractional) = make_integral_fractional(delay);
        // `int32_t t = (write_ptr_ + delay_integral + max_delay);`: the size_t sum, truncated.
        let t = offset(self.write_ptr_, delay_integral).wrapping_add(MAX_DELAY) as i32;
        // `(t - 1) % max_delay`: the int32_t converted to size_t before the `%`.
        let at = |i: i32| self.line_[(i as i64 as u64 as usize) % MAX_DELAY];
        let xm1 = at(t.wrapping_sub(1));
        let x0 = at(t);
        let x1 = at(t.wrapping_add(1));
        let x2 = at(t.wrapping_add(2));
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b_neg = w + a;
        let f = delay_fractional;
        (((a * f) - b_neg) * f + c) * f + x0
    }
}
