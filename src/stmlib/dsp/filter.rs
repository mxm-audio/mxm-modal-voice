//! Ported from stmlib/dsp/filter.h: zero-delay-feedback filters (one pole and SVF), naive SVF,
//! modified and crossover SVFs.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The template parameters are const generics: `set_f_q::<FREQUENCY_DIRTY>(f, q)`,
//! `process::<FILTER_MODE_LOW_PASS>(x)`. The C++ overloads of `Process` are named for their
//! arguments: `process` (one sample), `process2` (one sample, two outputs), `process_buffer`,
//! `process_add`, `process_stride`, `process_buffers` (two outputs). Where the C++ passes the
//! same buffer as input and output, the `_in_place` variant does the same arithmetic on one
//! slice.
//!
//! `FREQUENCY_EXACT` calls `tanf` (and `NaiveSvf`'s, `sinf`). Elements asks for it only from
//! `NaiveSvf`, in the hidden voice's `Init`, on two constants: their results are the reference
//! build's, as constants ([`two_sin_pi_f`]), so the port takes nothing from a maths library. The
//! `tanf` path is a compile-time error.
//!
//! The coefficients of the tangent approximations are the C++'s: `M_PI_POW_3` and its siblings
//! are unparenthesised macros, so `3.736e-01f * M_PI_POW_3` is
//! `((double) 3.736e-01f * M_PI) * M_PI * M_PI`, in double, rounded once to float.

use std::f64::consts::PI as M_PI;

use crate::cpp::{max, min};

pub type FilterMode = u8;
pub const FILTER_MODE_LOW_PASS: FilterMode = 0;
pub const FILTER_MODE_BAND_PASS: FilterMode = 1;
pub const FILTER_MODE_BAND_PASS_NORMALIZED: FilterMode = 2;
pub const FILTER_MODE_HIGH_PASS: FilterMode = 3;

pub type FrequencyApproximation = u8;
pub const FREQUENCY_EXACT: FrequencyApproximation = 0;
pub const FREQUENCY_ACCURATE: FrequencyApproximation = 1;
pub const FREQUENCY_FAST: FrequencyApproximation = 2;
pub const FREQUENCY_DIRTY: FrequencyApproximation = 3;

/// `M_PI_F`: `float(M_PI)`.
pub const M_PI_F: f32 = M_PI as f32;

/// `coefficient * M_PI_POW_<n>` as the macros expand: the float promoted to double, then
/// multiplied by `M_PI` `n` times from the left, rounded once to float.
const fn times_pi_pow(coefficient: f32, n: u32) -> f32 {
    let mut x = coefficient as f64;
    let mut i = 0;
    while i < n {
        x *= M_PI;
        i += 1;
    }
    x as f32
}

const DIRTY_A: f32 = times_pi_pow(3.736e-01, 3);
const FAST_A: f32 = times_pi_pow(3.260e-01, 3);
const FAST_B: f32 = times_pi_pow(1.823e-01, 5);
const ACCURATE_A: f32 = times_pi_pow(3.333314036e-01, 3);
const ACCURATE_B: f32 = times_pi_pow(1.333923995e-01, 5);
const ACCURATE_C: f32 = times_pi_pow(5.33740603e-02, 7);
const ACCURATE_D: f32 = times_pi_pow(2.900525e-03, 9);
const ACCURATE_E: f32 = times_pi_pow(9.5168091e-03, 11);

#[derive(Default)]
pub struct DCBlocker {
    pole_: f32,
    x_: f32,
    y_: f32,
}

impl DCBlocker {
    pub fn init(&mut self, pole: f32) {
        self.x_ = 0.0;
        self.y_ = 0.0;
        self.pole_ = pole;
    }

    #[inline]
    pub fn process(&mut self, in_out: &mut [f32], size: usize) {
        let mut x = self.x_;
        let mut y = self.y_;
        let pole = self.pole_;
        for sample in &mut in_out[..size] {
            let old_x = x;
            x = *sample;
            y = y * pole + x - old_x;
            *sample = y;
        }
        self.x_ = x;
        self.y_ = y;
    }
}

#[derive(Default)]
pub struct OnePole {
    g_: f32,
    gi_: f32,
    state_: f32,
}

impl OnePole {
    pub fn init(&mut self) {
        self.set_f::<FREQUENCY_DIRTY>(0.01);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.state_ = 0.0;
    }

    #[inline]
    pub fn tan<const APPROXIMATION: FrequencyApproximation>(f: f32) -> f32 {
        const {
            assert!(
                APPROXIMATION != FREQUENCY_EXACT,
                "FREQUENCY_EXACT needs tanf, which Elements never calls"
            )
        };
        if APPROXIMATION == FREQUENCY_DIRTY {
            // Optimized for frequencies below 8kHz.
            let a = DIRTY_A;
            f * (M_PI_F + a * f * f)
        } else if APPROXIMATION == FREQUENCY_FAST {
            // The usual tangent approximation uses 3.1755e-01 and 2.033e-01, but
            // the coefficients used here are optimized to minimize error for the
            // 16Hz to 16kHz range, with a sample rate of 48kHz.
            let a = FAST_A;
            let b = FAST_B;
            let f2 = f * f;
            f * (M_PI_F + f2 * (a + b * f2))
        } else if APPROXIMATION == FREQUENCY_ACCURATE {
            // These coefficients don't need to be tweaked for the audio range.
            let a = ACCURATE_A;
            let b = ACCURATE_B;
            let c = ACCURATE_C;
            let d = ACCURATE_D;
            let e = ACCURATE_E;
            let f2 = f * f;
            f * (M_PI_F + f2 * (a + f2 * (b + f2 * (c + f2 * (d + f2 * e)))))
        } else {
            0.0
        }
    }

    /// Set frequency and resonance from true units. Various approximations
    /// are available to avoid the cost of tanf.
    #[inline]
    pub fn set_f<const APPROXIMATION: FrequencyApproximation>(&mut self, f: f32) {
        self.g_ = Self::tan::<APPROXIMATION>(f);
        self.gi_ = 1.0 / (1.0 + self.g_);
    }

    #[inline]
    pub fn process<const MODE: FilterMode>(&mut self, in_: f32) -> f32 {
        let lp = (self.g_ * in_ + self.state_) * self.gi_;
        self.state_ = self.g_ * (in_ - lp) + lp;

        if MODE == FILTER_MODE_LOW_PASS {
            lp
        } else if MODE == FILTER_MODE_HIGH_PASS {
            in_ - lp
        } else {
            0.0
        }
    }

    #[inline]
    pub fn process_buffer<const MODE: FilterMode>(&mut self, in_out: &mut [f32], size: usize) {
        for sample in &mut in_out[..size] {
            *sample = self.process::<MODE>(*sample);
        }
    }
}

#[derive(Default)]
pub struct Svf {
    g_: f32,
    r_: f32,
    h_: f32,

    state_1_: f32,
    state_2_: f32,
}

/// The value the C++'s `if (mode == ...)` chains select, from one step's three outputs.
#[inline]
fn select<const MODE: FilterMode>(hp: f32, bp: f32, lp: f32, r: f32) -> f32 {
    if MODE == FILTER_MODE_LOW_PASS {
        lp
    } else if MODE == FILTER_MODE_BAND_PASS {
        bp
    } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
        bp * r
    } else if MODE == FILTER_MODE_HIGH_PASS {
        hp
    } else {
        0.0
    }
}

impl Svf {
    pub fn init(&mut self) {
        self.set_f_q::<FREQUENCY_DIRTY>(0.01, 100.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.state_1_ = 0.0;
        self.state_2_ = 0.0;
    }

    /// Copy settings from another filter.
    #[inline]
    pub fn set(&mut self, f: &Svf) {
        self.g_ = f.g();
        self.r_ = f.r();
        self.h_ = f.h();
    }

    /// Set all parameters from LUT.
    #[inline]
    pub fn set_g_r_h(&mut self, g: f32, r: f32, h: f32) {
        self.g_ = g;
        self.r_ = r;
        self.h_ = h;
    }

    /// Set frequency and resonance coefficients from LUT, adjust remaining
    /// parameter.
    #[inline]
    pub fn set_g_r(&mut self, g: f32, r: f32) {
        self.g_ = g;
        self.r_ = r;
        self.h_ = 1.0 / (1.0 + self.r_ * self.g_ + self.g_ * self.g_);
    }

    /// Set frequency from LUT, resonance in true units, adjust the rest.
    #[inline]
    pub fn set_g_q(&mut self, g: f32, resonance: f32) {
        self.g_ = g;
        self.r_ = 1.0 / resonance;
        self.h_ = 1.0 / (1.0 + self.r_ * self.g_ + self.g_ * self.g_);
    }

    /// Set frequency and resonance from true units. Various approximations
    /// are available to avoid the cost of tanf.
    #[inline]
    pub fn set_f_q<const APPROXIMATION: FrequencyApproximation>(&mut self, f: f32, resonance: f32) {
        self.g_ = OnePole::tan::<APPROXIMATION>(f);
        self.r_ = 1.0 / resonance;
        self.h_ = 1.0 / (1.0 + self.r_ * self.g_ + self.g_ * self.g_);
    }

    #[inline]
    pub fn process<const MODE: FilterMode>(&mut self, in_: f32) -> f32 {
        let hp =
            (in_ - self.r_ * self.state_1_ - self.g_ * self.state_1_ - self.state_2_) * self.h_;
        let bp = self.g_ * hp + self.state_1_;
        self.state_1_ = self.g_ * hp + bp;
        let lp = self.g_ * bp + self.state_2_;
        self.state_2_ = self.g_ * bp + lp;

        select::<MODE>(hp, bp, lp, self.r_)
    }

    #[inline]
    pub fn process2<const MODE_1: FilterMode, const MODE_2: FilterMode>(
        &mut self,
        in_: f32,
        out_1: &mut f32,
        out_2: &mut f32,
    ) {
        let hp =
            (in_ - self.r_ * self.state_1_ - self.g_ * self.state_1_ - self.state_2_) * self.h_;
        let bp = self.g_ * hp + self.state_1_;
        self.state_1_ = self.g_ * hp + bp;
        let lp = self.g_ * bp + self.state_2_;
        self.state_2_ = self.g_ * bp + lp;

        *out_1 = select::<MODE_1>(hp, bp, lp, self.r_);
        *out_2 = select::<MODE_2>(hp, bp, lp, self.r_);
    }

    /// One step of the block loops, on local copies of the state as the C++ keeps them.
    #[inline]
    fn step(&self, in_: f32, state_1: &mut f32, state_2: &mut f32) -> (f32, f32, f32) {
        let hp = (in_ - self.r_ * *state_1 - self.g_ * *state_1 - *state_2) * self.h_;
        let bp = self.g_ * hp + *state_1;
        *state_1 = self.g_ * hp + bp;
        let lp = self.g_ * bp + *state_2;
        *state_2 = self.g_ * bp + lp;
        (hp, bp, lp)
    }

    /// `Process<mode>(const float* in, float* out, size_t size)`.
    #[inline]
    pub fn process_buffer<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            let (hp, bp, lp) = self.step(*x, &mut state_1, &mut state_2);
            *y = select::<MODE>(hp, bp, lp, self.r_);
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    /// `Process<mode>(buffer, buffer, size)`, the input and the output one buffer.
    #[inline]
    pub fn process_buffer_in_place<const MODE: FilterMode>(
        &mut self,
        in_out: &mut [f32],
        size: usize,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for sample in &mut in_out[..size] {
            let (hp, bp, lp) = self.step(*sample, &mut state_1, &mut state_2);
            *sample = select::<MODE>(hp, bp, lp, self.r_);
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    /// `ProcessAdd<mode>(const float* in, float* out, size_t size, float gain)`.
    #[inline]
    pub fn process_add<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        gain: f32,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            let (hp, bp, lp) = self.step(*x, &mut state_1, &mut state_2);
            let value = select::<MODE>(hp, bp, lp, self.r_);
            *y += gain * value;
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    /// `Process<mode>(const float* in, float* out, size_t size, size_t stride)`: `size`
    /// samples, every `stride`-th of `in` and of `out`.
    #[inline]
    pub fn process_stride<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        stride: usize,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for i in 0..size {
            let (hp, bp, lp) = self.step(in_[i * stride], &mut state_1, &mut state_2);
            out[i * stride] = select::<MODE>(hp, bp, lp, self.r_);
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    /// `Process<mode>(buffer, buffer, size, stride)`, the input and the output one buffer.
    #[inline]
    pub fn process_stride_in_place<const MODE: FilterMode>(
        &mut self,
        in_out: &mut [f32],
        size: usize,
        stride: usize,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for i in 0..size {
            let (hp, bp, lp) = self.step(in_out[i * stride], &mut state_1, &mut state_2);
            in_out[i * stride] = select::<MODE>(hp, bp, lp, self.r_);
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    #[inline]
    fn multimode_gains(mode: f32) -> (f32, f32, f32) {
        let hp_gain = if mode < 0.5 {
            -mode * 2.0
        } else {
            -2.0 + mode * 2.0
        };
        let lp_gain = if mode < 0.5 { 1.0 - mode * 2.0 } else { 0.0 };
        let bp_gain = if mode < 0.5 { 0.0 } else { mode * 2.0 - 1.0 };
        (hp_gain, bp_gain, lp_gain)
    }

    #[inline]
    fn multimode_lp_to_hp_gains(mode: f32) -> (f32, f32, f32) {
        let hp_gain = min(-mode * 2.0 + 1.0, 0.0);
        let bp_gain = 1.0 - 2.0 * (mode - 0.5).abs();
        let lp_gain = max(1.0 - mode * 2.0, 0.0);
        (hp_gain, bp_gain, lp_gain)
    }

    #[inline]
    fn multimode(&mut self, in_: &[f32], out: &mut [f32], size: usize, gains: (f32, f32, f32)) {
        let (hp_gain, bp_gain, lp_gain) = gains;
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            let (hp, bp, lp) = self.step(*x, &mut state_1, &mut state_2);
            *y = hp_gain * hp + bp_gain * bp + lp_gain * lp;
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    #[inline]
    fn multimode_in_place(&mut self, in_out: &mut [f32], size: usize, gains: (f32, f32, f32)) {
        let (hp_gain, bp_gain, lp_gain) = gains;
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for sample in &mut in_out[..size] {
            let (hp, bp, lp) = self.step(*sample, &mut state_1, &mut state_2);
            *sample = hp_gain * hp + bp_gain * bp + lp_gain * lp;
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    #[inline]
    pub fn process_multimode(&mut self, in_: &[f32], out: &mut [f32], size: usize, mode: f32) {
        self.multimode(in_, out, size, Self::multimode_gains(mode));
    }

    #[inline]
    pub fn process_multimode_in_place(&mut self, in_out: &mut [f32], size: usize, mode: f32) {
        self.multimode_in_place(in_out, size, Self::multimode_gains(mode));
    }

    #[inline]
    pub fn process_multimode_lp_to_hp(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        mode: f32,
    ) {
        self.multimode(in_, out, size, Self::multimode_lp_to_hp_gains(mode));
    }

    #[inline]
    pub fn process_multimode_lp_to_hp_in_place(
        &mut self,
        in_out: &mut [f32],
        size: usize,
        mode: f32,
    ) {
        self.multimode_in_place(in_out, size, Self::multimode_lp_to_hp_gains(mode));
    }

    /// `Process<mode>(const float* in, float* out_1, float* out_2, size_t size, float gain_1,
    /// float gain_2)`: adds to both outputs.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn process_buffers<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out_1: &mut [f32],
        out_2: &mut [f32],
        size: usize,
        gain_1: f32,
        gain_2: f32,
    ) {
        let mut state_1 = self.state_1_;
        let mut state_2 = self.state_2_;
        for i in 0..size {
            let (hp, bp, lp) = self.step(in_[i], &mut state_1, &mut state_2);
            let value = select::<MODE>(hp, bp, lp, self.r_);
            out_1[i] += value * gain_1;
            out_2[i] += value * gain_2;
        }
        self.state_1_ = state_1;
        self.state_2_ = state_2;
    }

    #[inline]
    pub fn g(&self) -> f32 {
        self.g_
    }

    #[inline]
    pub fn r(&self) -> f32 {
        self.r_
    }

    #[inline]
    pub fn h(&self) -> f32 {
        self.h_
    }
}

/// `2.0f * sinf(M_PI_F * f)`, for the two `f` Elements gives `NaiveSvf::set_f_q<FREQUENCY_EXACT>`,
/// both constants in the hidden voice's `Init`: 0.05 (its spatializers' behind filters) and
/// `1.0f / kOversamplingUp * 0.8f`, 0.1 (its downsamplers' IIRs). The values are the reference
/// build's own, read off the filters after `Part::Init` (the harness's `--constants`), so the
/// port calls no `sinf`.
fn two_sin_pi_f(f: f32) -> f32 {
    match f.to_bits() {
        0x3d4c_cccd => f32::from_bits(0x3ea0_305c),
        0x3dcc_cccd => f32::from_bits(0x3f1e_377a),
        _ => unreachable!("Elements calls set_f_q<FREQUENCY_EXACT> only with 0.05 and 0.1"),
    }
}

/// Naive Chamberlin SVF.
#[derive(Default)]
pub struct NaiveSvf {
    f_: f32,
    damp_: f32,
    lp_: f32,
    bp_: f32,
}

impl NaiveSvf {
    pub fn init(&mut self) {
        self.set_f_q::<FREQUENCY_DIRTY>(0.01, 100.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.lp_ = 0.0;
        self.bp_ = 0.0;
    }

    /// Set frequency and resonance from true units. Various approximations
    /// are available to avoid the cost of sinf.
    #[inline]
    pub fn set_f_q<const APPROXIMATION: FrequencyApproximation>(
        &mut self,
        mut f: f32,
        resonance: f32,
    ) {
        if APPROXIMATION == FREQUENCY_EXACT {
            f = if f < 0.497 { f } else { 0.497 };
            self.f_ = two_sin_pi_f(f);
        } else {
            f = if f < 0.158 { f } else { 0.158 };
            self.f_ = 2.0 * M_PI_F * f;
        }
        self.damp_ = 1.0 / resonance;
    }

    #[inline]
    pub fn process<const MODE: FilterMode>(&mut self, in_: f32) -> f32 {
        let bp_normalized = self.bp_ * self.damp_;
        let notch = in_ - bp_normalized;
        self.lp_ += self.f_ * self.bp_;
        let hp = notch - self.lp_;
        self.bp_ += self.f_ * hp;

        if MODE == FILTER_MODE_LOW_PASS {
            self.lp_
        } else if MODE == FILTER_MODE_BAND_PASS {
            self.bp_
        } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
            bp_normalized
        } else if MODE == FILTER_MODE_HIGH_PASS {
            hp
        } else {
            0.0
        }
    }

    #[inline]
    pub fn lp(&self) -> f32 {
        self.lp_
    }

    #[inline]
    pub fn bp(&self) -> f32 {
        self.bp_
    }

    /// One step of the block loops, on local copies of the state.
    #[inline]
    fn step(&self, in_: f32, lp: &mut f32, bp: &mut f32) -> (f32, f32) {
        let bp_normalized = *bp * self.damp_;
        let notch = in_ - bp_normalized;
        *lp += self.f_ * *bp;
        let hp = notch - *lp;
        *bp += self.f_ * hp;
        (hp, bp_normalized)
    }

    #[inline]
    fn select<const MODE: FilterMode>(lp: f32, bp: f32, bp_normalized: f32, hp: f32) -> f32 {
        if MODE == FILTER_MODE_LOW_PASS {
            lp
        } else if MODE == FILTER_MODE_BAND_PASS {
            bp
        } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
            bp_normalized
        } else if MODE == FILTER_MODE_HIGH_PASS {
            hp
        } else {
            0.0
        }
    }

    /// `Process<mode>(const float* in, float* out, size_t size)`.
    #[inline]
    pub fn process_buffer<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        let mut lp = self.lp_;
        let mut bp = self.bp_;
        for (x, y) in in_[..size].iter().zip(&mut out[..size]) {
            let (hp, bp_normalized) = self.step(*x, &mut lp, &mut bp);
            *y = Self::select::<MODE>(lp, bp, bp_normalized, hp);
        }
        self.lp_ = lp;
        self.bp_ = bp;
    }

    /// `Process<mode>(buffer, buffer, size)`, the input and the output one buffer.
    #[inline]
    pub fn process_buffer_in_place<const MODE: FilterMode>(
        &mut self,
        in_out: &mut [f32],
        size: usize,
    ) {
        let mut lp = self.lp_;
        let mut bp = self.bp_;
        for sample in &mut in_out[..size] {
            let (hp, bp_normalized) = self.step(*sample, &mut lp, &mut bp);
            *sample = Self::select::<MODE>(lp, bp, bp_normalized, hp);
        }
        self.lp_ = lp;
        self.bp_ = bp;
    }

    #[inline]
    pub fn split(&mut self, in_: &[f32], low: &mut [f32], high: &mut [f32], size: usize) {
        let mut lp = self.lp_;
        let mut bp = self.bp_;
        for i in 0..size {
            let (hp, _) = self.step(in_[i], &mut lp, &mut bp);
            low[i] = lp;
            high[i] = hp;
        }
        self.lp_ = lp;
        self.bp_ = bp;
    }

    /// `Process<mode>(const float* in, float* out, size_t size, size_t decimate)`: writes one
    /// output every `decimate` samples, the first at sample 0.
    #[inline]
    pub fn process_decimate<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
        decimate: usize,
    ) {
        let mut lp = self.lp_;
        let mut bp = self.bp_;
        let mut n = decimate.wrapping_sub(1);
        let mut written = 0;
        for x in &in_[..size] {
            let (hp, bp_normalized) = self.step(*x, &mut lp, &mut bp);

            n = n.wrapping_add(1);
            if n == decimate {
                out[written] = Self::select::<MODE>(lp, bp, bp_normalized, hp);
                written += 1;
                n = 0;
            }
        }
        self.lp_ = lp;
        self.bp_ = bp;
    }
}

/// Modified Chamberlin SVF (Duane K. Wise)
/// <http://www.dafx.ca/proceedings/papers/p_053.pdf>
#[derive(Default)]
pub struct ModifiedSvf {
    f_: f32,
    fq_: f32,
    x_: f32,
    lp_: f32,
    bp_: f32,
}

impl ModifiedSvf {
    pub fn init(&mut self) {
        self.reset();
    }

    pub fn reset(&mut self) {
        self.lp_ = 0.0;
        self.bp_ = 0.0;
    }

    #[inline]
    pub fn set_f_fq(&mut self, f: f32, fq: f32) {
        self.f_ = f;
        self.fq_ = fq;
        self.x_ = 0.0;
    }

    #[inline]
    pub fn process_buffer<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        let mut lp = self.lp_;
        let mut bp = self.bp_;
        let mut x = self.x_;
        let fq = self.fq_;
        let f = self.f_;
        for (input, y) in in_[..size].iter().zip(&mut out[..size]) {
            lp += f * bp;
            bp += -fq * bp - f * lp + *input;
            if MODE == FILTER_MODE_BAND_PASS || MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp += x;
            }
            x = *input;

            *y = if MODE == FILTER_MODE_LOW_PASS {
                lp * f
            } else if MODE == FILTER_MODE_BAND_PASS {
                bp * f
            } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp * fq
            } else if MODE == FILTER_MODE_HIGH_PASS {
                x - lp * f - bp * fq
            } else {
                0.0
            };
        }
        self.lp_ = lp;
        self.bp_ = bp;
        self.x_ = x;
    }
}

/// Two passes of modified Chamberlin SVF with the same coefficients -
/// to implement Linkwitz–Riley (Butterworth squared) crossover filters.
#[derive(Default)]
pub struct CrossoverSvf {
    f_: f32,
    fq_: f32,
    x_: [f32; 2],
    lp_: [f32; 2],
    bp_: [f32; 2],
}

impl CrossoverSvf {
    pub fn init(&mut self) {
        self.reset();
    }

    pub fn reset(&mut self) {
        self.lp_[0] = 0.0;
        self.bp_[0] = 0.0;
        self.lp_[1] = 0.0;
        self.bp_[1] = 0.0;
        self.x_[0] = 0.0;
        self.x_[1] = 0.0;
    }

    #[inline]
    pub fn set_f_fq(&mut self, f: f32, fq: f32) {
        self.f_ = f;
        self.fq_ = fq;
    }

    #[inline]
    pub fn process_buffer<const MODE: FilterMode>(
        &mut self,
        in_: &[f32],
        out: &mut [f32],
        size: usize,
    ) {
        let mut lp_1 = self.lp_[0];
        let mut bp_1 = self.bp_[0];
        let mut lp_2 = self.lp_[1];
        let mut bp_2 = self.bp_[1];
        let mut x_1 = self.x_[0];
        let mut x_2 = self.x_[1];
        let fq = self.fq_;
        let f = self.f_;
        for (input, output) in in_[..size].iter().zip(&mut out[..size]) {
            lp_1 += f * bp_1;
            bp_1 += -fq * bp_1 - f * lp_1 + *input;
            if MODE == FILTER_MODE_BAND_PASS || MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp_1 += x_1;
            }
            x_1 = *input;

            let y = if MODE == FILTER_MODE_LOW_PASS {
                lp_1 * f
            } else if MODE == FILTER_MODE_BAND_PASS {
                bp_1 * f
            } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp_1 * fq
            } else if MODE == FILTER_MODE_HIGH_PASS {
                x_1 - lp_1 * f - bp_1 * fq
            } else {
                0.0
            };

            lp_2 += f * bp_2;
            bp_2 += -fq * bp_2 - f * lp_2 + y;
            if MODE == FILTER_MODE_BAND_PASS || MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp_2 += x_2;
            }
            x_2 = y;

            *output = if MODE == FILTER_MODE_LOW_PASS {
                lp_2 * f
            } else if MODE == FILTER_MODE_BAND_PASS {
                bp_2 * f
            } else if MODE == FILTER_MODE_BAND_PASS_NORMALIZED {
                bp_2 * fq
            } else if MODE == FILTER_MODE_HIGH_PASS {
                x_2 - lp_2 * f - bp_2 * fq
            } else {
                0.0
            };
        }
        self.lp_[0] = lp_1;
        self.bp_[0] = bp_1;
        self.lp_[1] = lp_2;
        self.bp_[1] = bp_2;
        self.x_[0] = x_1;
        self.x_[1] = x_2;
    }
}
