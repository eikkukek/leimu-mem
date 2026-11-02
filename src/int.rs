//! Traits for working with integers generically.
//!
//! # New traits
//! - [`Integer`]
//! - [`UInteger`]
//! - [`NonZeroInteger`]
//! - [`IntoUsize`]
//! - [`FromUsize`]

use core::{
    ops::{
        Range,
        Add, AddAssign, Sub, SubAssign, Mul, MulAssign,
        BitOr, BitOrAssign, BitAnd, BitAndAssign,
        BitXor, BitXorAssign,
        Shl, ShlAssign, Shr, ShrAssign,
    },
    hash::Hash,
    fmt::{Display, Debug},
    num::NonZero,
};

/// An iterator over bits.
pub struct BitIter<T: Integer> {
    num: T,
    bit: u32,
}

impl<T: Integer> Iterator for BitIter<T> {

    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let n = size_of::<T>() as u32 * 8 - 1;
        if self.bit > n {
            None
        } else {
            let mut bit = self.bit;
            while bit <= n && T::ONE << bit & self.num == T::ZERO {
                bit += 1;
            }
            self.bit = bit + 1;
            (bit <= n).then(|| T::ONE << bit)
        }
    }
}

/// A trait for working with generic integers.
pub trait Integer:
    'static + Sized + Copy + Send + Sync +
    Add<Output = Self> + AddAssign + Sub<Output = Self> + SubAssign +
    Mul<Output = Self> + MulAssign +
    PartialEq + Eq + PartialOrd + Ord + Hash +
    BitOr<Output = Self> + BitOrAssign + BitAnd<Output = Self> + BitAndAssign +
    BitXor<Output = Self> + BitXorAssign +
    Shl<Output = Self> + ShlAssign + Shr<Output = Self> + ShrAssign +
    Shl<u32, Output = Self> + ShlAssign<u32> + Shr<u32, Output = Self> + ShrAssign<u32> +
    Display + Debug
{

    /// The zero constant.
    const ZERO: Self;
    /// The one constant.
    const ONE: Self;
    /// The minimum value of self.
    const MIN: Self;
    /// The maximum value of self.
    const MAX: Self;

    /// The [`NonZero`] type of self.
    ///
    /// Since Rust's standard library doesn't expose `ZeroablePrimitive`, this is needed for using
    /// using [`NonZero`] in a generic context.
    type NonZero: NonZeroInteger<Base = Self>;
    /// Range-based iterator.
    type Iter: Iterator<Item = Self> + DoubleEndedIterator<Item = Self>;

    /// Returns a range-based iterator.
    ///
    /// Equivalent of calling a..b.
    #[must_use]
    fn iter(self, to: Self) -> Self::Iter;

    /// Returns an iterator over the bits of self.
    #[must_use]
    #[inline]
    fn bit_iter(self) -> BitIter<Self> {
        BitIter { num: self, bit: 0, }
    }

    /// Returns a range from self to `to`.
    #[must_use]
    fn range(self, to: Self) -> Range<Self>;
 
    /// Adds self to `x`, wrapping around if the value overflows.
    #[must_use]
    fn wrapping_add(self, x: Self) -> Self;

    /// Subtracts `x` from self, wrapping around if the value underflows.
    #[must_use]
    fn wrapping_sub(self, x: Self) -> Self;

    /// Negates self, wrapping around if the value overflows.
    #[must_use]
    fn wrapping_neg(self) -> Self;

    /// Subtracts `x` from self, saturating the result.
    #[must_use]
    fn saturating_sub(self, x: Self) -> Self;

    /// Creates self from a [`bool`].
    #[must_use]
    fn from_bool(x: bool) -> Self;
}

macro_rules! impl_integers {
    ($($num:ty),*) => {
        $(
            impl Integer for $num {

                const ZERO: Self = 0;
                const ONE: Self = 1;
                const MIN: Self = Self::MIN;
                const MAX: Self = Self::MAX;

                type NonZero = NonZero<$num>;
                type Iter = Range<Self>;

                #[inline]
                fn iter(self, to: Self) -> Self::Iter {
                    self..to
                }

                #[inline]
                fn range(self, to: Self) -> Range<Self> {
                    self..to
                }

                #[inline]
                fn wrapping_add(self, x: Self) -> Self {
                    self.wrapping_add(x)
                }

                #[inline]
                fn wrapping_sub(self, x: Self) -> Self {
                    self.wrapping_sub(x)
                }

                #[inline]
                fn wrapping_neg(self) -> Self {
                    self.wrapping_neg()
                }

                #[inline]
                fn saturating_sub(self, x: Self) -> Self {
                    self.saturating_sub(x)
                }

                #[inline]
                fn from_bool(x: bool) -> Self {
                    x as Self
                }
            }
        )*
    };
}

impl_integers!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

/// A trait for [`NonZero`] types.
pub trait NonZeroInteger:
    Clone + Copy + PartialEq + Eq + Hash +
    Debug + Display
{

    /// The internal [`Integer`] type.
    type Base: Integer;

    /// Creates a new [`NonZero`], returning [`None`] if `value` is zero.
    fn new(value: Self::Base) -> Option<Self>;
    
    /// Creates a new [`NonZero`] without checking if the value is zero.
    ///
    /// # Safety
    /// If the value is zero, this will produce and invalid value.
    ///
    unsafe fn new_unchecked(value: Self::Base) -> Self;

    /// Gets the inner value.
    fn get(self) -> Self::Base;
}

macro_rules! impl_non_zero_integer {
    ($($t:ty),*) => {
    
        $(
        impl NonZeroInteger for core::num::NonZero<$t> {

            type Base = $t;

            #[inline]
            fn new(value: $t) -> Option<Self> {
                Self::new(value)
            }

            #[inline]
            unsafe fn new_unchecked(value: $t) -> Self {
                unsafe {
                    Self::new_unchecked(value)
                }
            }

            #[inline]
            fn get(self) -> $t {
                self.get()
            }
        }
        )*
    };
}

impl_non_zero_integer!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

/// A trait for working with generic unsigned integers.
pub trait UInteger: Integer {}

impl UInteger for u8 {}
impl UInteger for u16 {}
impl UInteger for u32 {}
impl UInteger for u64 {}
impl UInteger for u128 {}
impl UInteger for usize {}

/// A trait for [`Integer`] types, which can be converted into [`usize`] without overflowing.
pub trait IntoUsize: Integer {

    /// Converts self to [`usize`].
    fn into_usize(self) -> usize;
}

macro_rules! impl_into_usize {
    ($($num:ty),*) => {
        $(
            impl IntoUsize for $num {
                
                #[inline]
                fn into_usize(self) -> usize {
                    self as usize
                }
            }
        )*
    };
}

impl_into_usize!(u8, u16, u32, usize, i32);

/// A trait for [`Integer`]s, which can be converted from [`usize`].
pub trait FromUsize: Integer {

    /// Converts [`usize`] to self.
    fn from_usize(value: usize) -> Self;
}

impl FromUsize for usize {

    #[inline]
    fn from_usize(value: usize) -> Self {
        value
    }
}

impl FromUsize for u32 {

    #[inline]
    fn from_usize(value: usize) -> Self {
        value as u32
    }
}

/// An extension trait for [`NonZero`] [`Option`].
pub trait NonZeroOption<T>
    where T: NonZeroInteger
{

    /// Unwraps the option, or returns a sentinel value.
    fn unwrap_or_sentinel(self, x: T::Base) -> T::Base;

    /// Unwraps the option, or returns a sentinel value from a closure.
    fn unwrap_or_sentinel_with<F>(self, f: F) -> T::Base
        where F: FnOnce() -> T::Base;
}

impl<T> NonZeroOption<T> for Option<T>
    where T: NonZeroInteger
{

    #[inline]
    fn unwrap_or_sentinel(self, x: T::Base) -> T::Base {
        match self {
            Some(value) => value.get(),
            None => x,
        }
    }

    #[inline]
    fn unwrap_or_sentinel_with<F>(self, f: F) -> <T as NonZeroInteger>::Base
        where F: FnOnce() -> T::Base
    {
        match self {
            Some(value) => value.get(),
            None => f(),
        }
    }
}
