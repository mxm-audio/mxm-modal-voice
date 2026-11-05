//! Ported from elements/dsp/fx/fx_engine.h: the base the reverb and the diffuser are built on.
//!
//! Copyright 2014 Emilie Gillet. MIT licence (`NOTICE.md`).
//!
//! The C++ builds each delay line's place in memory with templates (`Reserve`, `DelayLine`);
//! here [`DelayLine`] is a pair of constants, made by [`delay_lines`] from the same list of
//! lengths: each line starts one past the end of the one before. The format is a type
//! ([`Format16`], [`Format32`], [`Format12`]) as `DataType<format>` is. The engine owns its
//! memory, which the C++ is handed (`elements.cc`'s `reverb_buffer`, the voice's
//! `diffuser_buffer_`), zero as those static buffers are.
//!
//! A [`Context`] is made by [`FxEngine::start`] for each sample, as the C++'s `Start` resets every
//! field of the one it is given; it borrows the engine's memory while it lives. The C++
//! overloads are named for their arguments: `read_value` (`Read(float, float)`),
//! `read_value_once` (`Read(float)`), `write_value` (`Write(float&)`), `write_value_scaled`
//! (`Write(float&, float)`), and for a delay line `read`, `write` and `write_all_pass`, each
//! with its offset, [`TAIL`] for the line's last sample.

use crate::stmlib::dsp::cosine_oscillator::{COSINE_OSCILLATOR_APPROXIMATE, CosineOscillator};
use crate::stmlib::dsp::dsp::{cast_i32, clip16, make_integral_fractional};

/// `TAIL`: `, -1`, the offset of a line's last sample.
pub const TAIL: i32 = -1;

pub const LFO_1: usize = 0;
pub const LFO_2: usize = 1;

/// `DataType<format>`: how a sample is kept in the engine's memory.
pub trait DataType {
    type T: Copy + Default;
    fn decompress(value: Self::T) -> f32;
    fn compress(value: f32) -> Self::T;
}

/// `FORMAT_12_BIT`.
pub struct Format12;
/// `FORMAT_16_BIT`.
pub struct Format16;
/// `FORMAT_32_BIT`.
pub struct Format32;

impl DataType for Format12 {
    type T = u16;
    #[inline]
    fn decompress(value: u16) -> f32 {
        f32::from(value as i16) / 4096.0
    }
    #[inline]
    fn compress(value: f32) -> u16 {
        clip16(cast_i32(value * 4096.0)) as u16
    }
}

impl DataType for Format16 {
    type T = u16;
    #[inline]
    fn decompress(value: u16) -> f32 {
        f32::from(value as i16) / 32768.0
    }
    #[inline]
    fn compress(value: f32) -> u16 {
        clip16(cast_i32(value * 32768.0)) as u16
    }
}

impl DataType for Format32 {
    type T = f32;
    #[inline]
    fn decompress(value: f32) -> f32 {
        value
    }
    #[inline]
    fn compress(value: f32) -> f32 {
        value
    }
}

/// `DelayLine<Memory, index>`: where a line starts in the engine's memory, and how long it is.
#[derive(Clone, Copy)]
pub struct DelayLine {
    pub base: i32,
    pub length: i32,
}

/// The lines of a `Reserve<l0, Reserve<l1, …>>` list, in order: each `base` is the one before's
/// `base + length + 1`.
pub const fn delay_lines<const N: usize>(lengths: [i32; N]) -> [DelayLine; N] {
    let mut lines = [DelayLine { base: 0, length: 0 }; N];
    let mut base = 0;
    let mut i = 0;
    while i < N {
        lines[i] = DelayLine {
            base,
            length: lengths[i],
        };
        base += lengths[i] + 1;
        i += 1;
    }
    lines
}

pub struct FxEngine<const SIZE: usize, F: DataType> {
    write_ptr_: i32,
    buffer_: Box<[F::T]>,
    lfo_: [CosineOscillator; 2],
}

impl<const SIZE: usize, F: DataType> Default for FxEngine<SIZE, F> {
    fn default() -> Self {
        Self {
            write_ptr_: 0,
            buffer_: vec![F::T::default(); SIZE].into_boxed_slice(),
            lfo_: [CosineOscillator::default(), CosineOscillator::default()],
        }
    }
}

impl<const SIZE: usize, F: DataType> FxEngine<SIZE, F> {
    /// `MASK`: `size - 1`.
    const MASK: i32 = SIZE as i32 - 1;

    pub fn init(&mut self) {
        self.buffer_.fill(F::T::default());
        self.write_ptr_ = 0;
    }

    /// Every field as the static object's zero, the LFOs too, allocating nothing: for a host,
    /// not in the C++.
    pub fn clear(&mut self) {
        self.init();
        self.lfo_ = [CosineOscillator::default(), CosineOscillator::default()];
    }

    #[inline]
    pub fn set_lfo_frequency(&mut self, index: usize, frequency: f32) {
        self.lfo_[index].init::<COSINE_OSCILLATOR_APPROXIMATE>(frequency * 32.0);
    }

    #[inline]
    pub fn start(&mut self) -> Context<'_, SIZE, F> {
        self.write_ptr_ -= 1;
        if self.write_ptr_ < 0 {
            self.write_ptr_ += SIZE as i32;
        }
        let lfo_value_ = if (self.write_ptr_ & 31) == 0 {
            [self.lfo_[0].next(), self.lfo_[1].next()]
        } else {
            [self.lfo_[0].value(), self.lfo_[1].value()]
        };
        Context {
            accumulator_: 0.0,
            previous_read_: 0.0,
            lfo_value_,
            buffer_: &mut self.buffer_,
            write_ptr_: self.write_ptr_,
        }
    }
}

pub struct Context<'a, const SIZE: usize, F: DataType> {
    accumulator_: f32,
    previous_read_: f32,
    lfo_value_: [f32; 2],
    buffer_: &'a mut [F::T],
    write_ptr_: i32,
}

impl<const SIZE: usize, F: DataType> Context<'_, SIZE, F> {
    const MASK: i32 = SIZE as i32 - 1;

    /// `(write_ptr_ + offset) & MASK`, in `int32_t`.
    #[inline]
    fn index(&self, offset: i32) -> usize {
        (self.write_ptr_.wrapping_add(offset) & Self::MASK) as usize
    }

    #[inline]
    pub fn load(&mut self, value: f32) {
        self.accumulator_ = value;
    }

    #[inline]
    pub fn read_value(&mut self, value: f32, scale: f32) {
        self.accumulator_ += value * scale;
    }

    #[inline]
    pub fn read_value_once(&mut self, value: f32) {
        self.accumulator_ += value;
    }

    #[inline]
    pub fn write_value(&mut self, value: &mut f32) {
        *value = self.accumulator_;
    }

    #[inline]
    pub fn write_value_scaled(&mut self, value: &mut f32, scale: f32) {
        *value = self.accumulator_;
        self.accumulator_ *= scale;
    }

    #[inline]
    pub fn write(&mut self, d: DelayLine, offset: i32, scale: f32) {
        debug_assert!(d.base + d.length <= SIZE as i32, "delay memory full");
        let w = F::compress(self.accumulator_);
        let at = if offset == -1 {
            self.index(d.base + d.length - 1)
        } else {
            self.index(d.base.wrapping_add(offset))
        };
        self.buffer_[at] = w;
        self.accumulator_ *= scale;
    }

    #[inline]
    pub fn write_all_pass(&mut self, d: DelayLine, offset: i32, scale: f32) {
        self.write(d, offset, scale);
        self.accumulator_ += self.previous_read_;
    }

    #[inline]
    pub fn read(&mut self, d: DelayLine, offset: i32, scale: f32) {
        debug_assert!(d.base + d.length <= SIZE as i32, "delay memory full");
        let r = if offset == -1 {
            self.buffer_[self.index(d.base + d.length - 1)]
        } else {
            self.buffer_[self.index(d.base.wrapping_add(offset))]
        };
        let r_f = F::decompress(r);
        self.previous_read_ = r_f;
        self.accumulator_ += r_f * scale;
    }

    #[inline]
    pub fn lp(&mut self, state: &mut f32, coefficient: f32) {
        *state += coefficient * (self.accumulator_ - *state);
        self.accumulator_ = *state;
    }

    #[inline]
    pub fn hp(&mut self, state: &mut f32, coefficient: f32) {
        *state += coefficient * (self.accumulator_ - *state);
        self.accumulator_ -= *state;
    }

    #[inline]
    pub fn interpolate(&mut self, d: DelayLine, offset: f32, scale: f32) {
        debug_assert!(d.base + d.length <= SIZE as i32, "delay memory full");
        let (offset_integral, offset_fractional) = make_integral_fractional(offset);
        self.interpolate_at(d, offset_integral, offset_fractional, scale);
    }

    /// `Interpolate(D& d, float offset, LFOIndex index, float amplitude, float scale)`.
    #[inline]
    pub fn interpolate_lfo(
        &mut self,
        d: DelayLine,
        mut offset: f32,
        index: usize,
        amplitude: f32,
        scale: f32,
    ) {
        debug_assert!(d.base + d.length <= SIZE as i32, "delay memory full");
        offset += amplitude * self.lfo_value_[index];
        let (offset_integral, offset_fractional) = make_integral_fractional(offset);
        self.interpolate_at(d, offset_integral, offset_fractional, scale);
    }

    #[inline]
    fn interpolate_at(&mut self, d: DelayLine, integral: i32, fractional: f32, scale: f32) {
        let a = F::decompress(self.buffer_[self.index(integral.wrapping_add(d.base))]);
        let b =
            F::decompress(self.buffer_[self.index(integral.wrapping_add(d.base).wrapping_add(1))]);
        let x = a + (b - a) * fractional;
        self.previous_read_ = x;
        self.accumulator_ += x * scale;
    }
}

const _: () = {
    // MASK is only a mask for a power of two, as every Elements engine is.
    assert!(FxEngine::<1024, Format32>::MASK == 1023);
    assert!(FxEngine::<32768, Format16>::MASK == 32767);
};
