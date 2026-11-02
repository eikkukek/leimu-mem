//! Iterator utilities.
//!
//! # New traits
//! - [`FromIteratorIn`]: [`FromIterator`] that takes in an allocator
//! - [`CollectIn`]: An extension of [`Iterator`] that provides [`collect_in`][1].
//!
//! [1]: CollectIn::collect_in

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
/// [1]: CollectIn::collect_in
pub trait CollectIn: Iterator {

    /// Transforms an iterator into a collection that uses `allocator`.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use leimu_mem::arena::Arena;
    /// use leimu_mem::alloc_vec::FixedVec;
    /// use leimu_mem::iter::CollectIn;
    ///
    /// let arena = Arena::new(512).unwrap();
    /// let vec: FixedVec<u32, &Arena> = (0..5)
    ///     .collect_in(&arena)
    ///     .unwrap();
    /// assert_eq!(vec, [0, 1, 2, 3, 4]);
    /// # }
    /// ```
    fn collect_in<T>(self, allocator: T::Alloc) -> Result<T, ReserveError<()>>
        where
            T: FromIteratorIn<Self::Item>,
            Self: Sized + ExactSizeIterator,
    {
        T::from_iter_in(self, allocator)
    }
}

impl<T> CollectIn for T
    where T: Iterator
{}
