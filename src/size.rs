//! Traits for working with "size types" generically.
//!
//! # New traits
//! - [`Size`]
//! - [`NonZeroSize`]

use core::{
    ops::{
        Add, AddAssign, Sub, SubAssign, Mul, MulAssign,
        BitOr, BitOrAssign, BitAnd, BitAndAssign,
        BitXor, BitXorAssign,
        Shl, ShlAssign, Shr, ShrAssign,
    },
    hash::Hash,
    fmt::{Display, Debug},
    num::NonZero,
};

/// A trait for working with unsigned "size types" generically.
///
/// # Safety
/// This trait should **not** be implemented by the user. It is used by collections in this crate,
/// which expect the types implementing this trait to behave exactly like integers.
pub unsafe trait Size:
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
    type NonZero: NonZeroSize<Base = Self>;
 
    /// Adds self to `x`, wrapping around if the value overflows.
    #[must_use]
    fn wrapping_add(self, x: Self) -> Self;

    /// Converts self to [`usize`] with the `as` operator.
    fn into_usize(self) -> usize;

    /// Converts [`usize`] to `self` with the `as` operator.
    fn from_usize(size: usize) -> Self;
}

macro_rules! impl_integers {
    ($($num:ty),*) => {
        $(
            unsafe impl Size for $num {

                const ZERO: Self = 0;
                const ONE: Self = 1;
                const MIN: Self = Self::MIN;
                const MAX: Self = Self::MAX;

                type NonZero = NonZero<$num>;

                #[inline]
                fn wrapping_add(self, x: Self) -> Self {
                    self.wrapping_add(x)
                }


                /// Converts self to [`usize`].
                fn into_usize(self) -> usize {
                    self as usize
                }

                /// Converts self to [`usize`].
                fn from_usize(size: usize) -> Self {
                    size as Self
                }
            }
        )*
    };
}

impl_integers!(u8, u16, u32, usize);

/// A trait for [`NonZero`] [`Size`] types.
pub trait NonZeroSize:
    Clone + Copy + PartialEq + Eq + Hash +
    Debug + Display
{

    /// The internal [`Size`] type.
    type Base: Size;

    /// The constant `1` of this type.
    const ONE: Self;

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
        impl NonZeroSize for core::num::NonZero<$t> {

            type Base = $t;

            const ONE: Self = unsafe { Self::new_unchecked(1) };

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

impl_non_zero_integer!(u8, u16, u32, usize);
