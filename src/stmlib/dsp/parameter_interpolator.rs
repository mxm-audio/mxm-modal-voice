//! Ported from stmlib/dsp/parameter_interpolator.h: linear interpolation of parameters in
//! rendering loops.
//!
//! Copyright 2015 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ keeps a pointer to the state and writes the accumulated value back in its destructor;
//! here it borrows the state and writes it back when dropped, at the end of its scope, as the
//! C++ destructor runs. So the state is the value reached after the last `next()`, not the
//! target.

pub struct ParameterInterpolator<'a> {
    state_: &'a mut f32,
    value_: f32,
    increment_: f32,
}

impl<'a> ParameterInterpolator<'a> {
    /// `ParameterInterpolator(float* state, float new_value, size_t size)`.
    #[inline]
    pub fn new(state: &'a mut f32, new_value: f32, size: usize) -> Self {
        let value_ = *state;
        let increment_ = (new_value - *state) / size as f32;
        Self {
            state_: state,
            value_,
            increment_,
        }
    }

    /// `ParameterInterpolator(float* state, float new_value, float step)`.
    #[inline]
    pub fn with_step(state: &'a mut f32, new_value: f32, step: f32) -> Self {
        let value_ = *state;
        let increment_ = (new_value - *state) * step;
        Self {
            state_: state,
            value_,
            increment_,
        }
    }

    /// `Init(float* state, float new_value, size_t size)`: points the interpolator at another
    /// state, without writing the old one back, as the C++ does.
    #[inline]
    pub fn init(&mut self, state: &'a mut f32, new_value: f32, size: usize) {
        self.value_ = *state;
        self.increment_ = (new_value - *state) / size as f32;
        self.state_ = state;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        self.value_ += self.increment_;
        self.value_
    }

    #[inline]
    pub fn subsample(&self, t: f32) -> f32 {
        self.value_ + self.increment_ * t
    }
}

impl Drop for ParameterInterpolator<'_> {
    /// `~ParameterInterpolator() { *state_ = value_; }`.
    #[inline]
    fn drop(&mut self) {
        *self.state_ = self.value_;
    }
}
