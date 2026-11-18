//! Iterator utilities.
//!
//! # New traits
//! - [`FromIteratorIn`]: [`FromIterator`] that takes in an allocator
//! - [`CollectIn`]: An extension of [`Iterator`] that provides [`collect_in`][1].
//!
//! [1]: CollectIn::collect_in

use core::{
    alloc::{Allocator, AllocError, LayoutError},
    fmt::Display,
    error::Error,
};

#[cfg(feature = "std")]
use std::collections::TryReserveError;

use crate::reserve::ReserveError;

/// Indicates an error returned from [`FromIteratorIn::from_iter_in`].
#[derive(Debug)]
pub enum FromIteratorInError {
    /// Indicates an allocation failure.
    AllocError(AllocError),
    /// Indicates an [`Layout`][core::alloc::Layout] error.
    LayoutError(LayoutError),
    /// Indicates an error returned by collections in this crate.
    ReserveError(ReserveError<()>),
    /// Indicates an error returned from [`std::collections`] reservation.
    #[cfg(feature = "std")]
    TryReserveError(TryReserveError),
}
impl Display for FromIteratorInError {

    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AllocError(_) => "allocation error".fmt(f),
            Self::LayoutError(_) => "layout error".fmt(f),
            Self::ReserveError(_) => "reserve error".fmt(f),
            #[cfg(feature = "std")]
            Self::TryReserveError(_) => "try reserve error".fmt(f),
        }
    }
}
impl Error for FromIteratorInError {

    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::AllocError(err) => err,
            Self::LayoutError(err) => err,
            Self::ReserveError(err) => err,
            #[cfg(feature = "std")]
            Self::TryReserveError(err) => err,
        })
    }
}
impl From<AllocError> for FromIteratorInError {
    #[inline]
    fn from(value: AllocError) -> Self {
        Self::AllocError(value)
    }
}
impl From<LayoutError> for FromIteratorInError {
    #[inline]
    fn from(value: LayoutError) -> Self {
        Self::LayoutError(value)
    }
}
impl From<ReserveError<()>> for FromIteratorInError {
    #[inline]
    fn from(value: ReserveError<()>) -> Self {
        Self::ReserveError(value)
    }
}
#[cfg(feature = "std")]
impl From<TryReserveError> for FromIteratorInError {
    #[inline]
    fn from(value: TryReserveError) -> Self {
        Self::TryReserveError(value)
    }
}

/// Conversion from an iterator and an allocator.
pub trait FromIteratorIn<T>: Sized
{

    /// The allocator used by self.
    type Alloc: Allocator;

    /// Creates a value from an iterator and an allocator.
    fn from_iter_in<I>(
        iter: I,
        alloc: Self::Alloc,
    ) -> Result<Self, FromIteratorInError>
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
    fn collect_in<T>(self, alloc: T::Alloc) -> Result<T, FromIteratorInError>
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

    use std::{
        vec::Vec,
        boxed::Box,
    };

    use core::{
        alloc::Layout,
        mem::DropGuard,
        ptr::NonNull,
    };

    impl<T, A: Allocator> FromIteratorIn<T> for Vec<T, A> {

        type Alloc = A;

        fn from_iter_in<I>(
            iter: I,
            alloc: Self::Alloc,
        ) -> Result<Self, FromIteratorInError>
            where
                I: IntoIterator<Item = T>,
                I::IntoIter: ExactSizeIterator
        {
            let iter = iter.into_iter();
            let mut vec = Vec::new_in(alloc);
            vec.try_reserve(iter.len())?;
            vec.extend(iter);
            Ok(vec)
        }
    }

    impl<T, A: Allocator> FromIteratorIn<T> for Box<[T], A> {

        type Alloc = A;

        fn from_iter_in<I>(
            iter: I,
            alloc: Self::Alloc,
        ) -> Result<Self, FromIteratorInError>
            where
                I: IntoIterator<Item = T>,
                I::IntoIter: ExactSizeIterator,
        {
            let iter = iter.into_iter();
            let len = iter.len();
            let layout = Layout::array::<T>(len)?;
            let ptr = alloc
                .allocate(layout)?
                .cast::<T>();
            // iterator panic guard
            let mut guard = DropGuard::new(0, |cur_len| {
                // frees the memory even if drop panics
                let _free_guard = DropGuard::new((), |_| unsafe {
                    alloc.deallocate(ptr.cast(), layout);
                });
                for i in 0..cur_len {
                    unsafe {
                        ptr.add(i).drop_in_place();
                    }
                }
            });
            unsafe {
                for item in iter {
                    let i = *guard;
                    if i >= len {
                        break
                    }
                    ptr.add(i).write(item);
                    *guard += 1;
                }
            }
            let actual_len = *guard;
            if len != actual_len {
                panic!("`ExactSizeIterator::len` is greater than the actual number of items in `iter`");
            }
            DropGuard::dismiss(guard);
            unsafe {
                Ok(Box::from_raw_in(
                    NonNull::slice_from_raw_parts(
                        ptr,
                        len
                    ).as_ptr(),
                    alloc
                ))
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn box_from_in() -> Result<(), FromIteratorInError> {
        let alloc = crate::arena::Arena::new(32).unwrap();
        let _b: Box<[_], _> = (0u8..32u8).collect_in(&alloc)?;
        assert!(Box::from_iter_in(0..1, &alloc).is_err());
        Ok(())
    }

    #[cfg(test)]
    #[should_panic]
    #[test]
    fn box_from_in_panic() {
        struct StupidIter(core::ops::Range<u32>);
        impl Iterator for StupidIter {

            type Item = std::string::String;

            fn next(&mut self) -> Option<Self::Item> {
                use std::string::ToString;
                self.0.next().map(|i| i.to_string())
            }
        }
        impl ExactSizeIterator for StupidIter {

            #[inline]
            fn len(&self) -> usize {
                self.0.len() + 1
            }
        }
        let _b: Box<[_], _> = StupidIter(0..5).collect_in(&std::alloc::Global).unwrap();
    }
}
