//! Iterator utilities.
//!
//! # New traits
//! - [`FromIteratorIn`]: [`FromIterator`] that takes in an allocator
//! - [`IteratorExt`]

use crate::{
    alloc::LocalAlloc,
    reserve::{ReserveError},
};

/// Conversion from an iterator and an allocator.
pub trait FromIteratorIn<T>: Sized
{

    /// The allocator used by self.
    type Alloc: LocalAlloc;

    /// Creates a value from an iterator and an allocator.
    fn from_iter_in<I>(
        iter: I,
        allocator: Self::Alloc,
    ) -> Result<Self, ReserveError<()>>
        where
            I: IntoIterator<Item = T>,
            I::IntoIter: ExactSizeIterator;
}

/// Provides [`collect_in`][1].
///
/// [1]: IteratorExt::collect_in
pub trait IteratorExt: Iterator {

    /// Transforms an iterator into a collection that uses `allocator`.
    fn collect_in<T>(self, allocator: T::Alloc) -> Result<T, ReserveError<()>>
        where
            T: FromIteratorIn<Self::Item>,
            Self: Sized + ExactSizeIterator,
    {
        T::from_iter_in(self, allocator)
    }
}

impl<T> IteratorExt for T
    where T: Iterator
{}
