//! Iterator utilities.
//!
//! # New traits
//! - [`FromIteratorIn`]: [`FromIterator`] that takes in an allocator
//! - [`CollectIn`]: An extension of [`Iterator`] that provides [`collect_in`][1].
//!
//! [1]: CollectIn::collect_in

use core::alloc::{Allocator, AllocError};

/// Conversion from an iterator and an allocator.
pub trait FromIteratorIn<T>: Sized
{

    /// The allocator used by self.
    type Alloc: Allocator;

    /// Creates a value from an iterator and an allocator.
    fn from_iter_in<I>(
        iter: I,
        alloc: Self::Alloc,
    ) -> Result<Self, AllocError>
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
    /// use core::alloc::Allocator;
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
    fn collect_in<T>(self, alloc: T::Alloc) -> Result<T, AllocError>
        where
            T: FromIteratorIn<Self::Item>,
            Self: Sized + ExactSizeIterator,
    {
        T::from_iter_in(self, alloc)
    }
}

impl<T> CollectIn for T
    where T: Iterator
{}

#[cfg(feature = "std")]
mod std_impl {

    use super::*;

    use std::vec::Vec;

    impl<T, A: Allocator> FromIteratorIn<T> for Vec<T, A> {

        type Alloc = A;

        fn from_iter_in<I>(
            iter: I,
            alloc: Self::Alloc,
        ) -> Result<Self, AllocError>
            where
                I: IntoIterator<Item = T>,
                I::IntoIter: ExactSizeIterator
        {
            let iter = iter.into_iter();
            let mut vec = Vec::new_in(alloc);
            vec.try_reserve(iter.len()).map_err(|_| AllocError)?;
            vec.extend(iter);
            Ok(vec)
        }
    }
}
