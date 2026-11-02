//! Traits and types for use in custom collections.
//!
//! Contains:
//! - [`ReservePolicy`]: A trait that controls how collections grow their capacity.
//! - [`ReserveError`]: An error caused by reservation failure.

use core::{
    alloc::{Layout, LayoutError},
    error::Error,
    fmt::{self, Debug, Display},
};

use crate::{alloc::AllocError, int::{FromUsize, IntoUsize}};

/// The type [`ReserveError`].
#[derive(Debug)]
pub enum ReserveErrorKind {
    /// Indicates an allocation error
    AllocError(AllocError),
    /// Indicates that creating a [`Layout`] failed.
    ///
    /// [1]: core::alloc::Layout
    LayoutError(LayoutError),
    /// Indicates that the maximum capacity of a collection has been reached.
    MaxCapacityExceeded {
        /// The maximum capacity.
        max_capacity: usize,
        /// The requested capacity.
        requested_capacity: usize,
    },
}

impl Display for ReserveErrorKind {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllocError(_) => write!(f, "allocation failed"),
            Self::LayoutError(_) => write!(f, "layout error"),
            Self::MaxCapacityExceeded { max_capacity, requested_capacity } => write!(f,
                "maximum capacity of {max_capacity} exceeded, requested capacity is {requested_capacity}"
            ),
        }
    }
}

impl Error for ReserveErrorKind {

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AllocError(err) => err.source(),
            Self::LayoutError(err) => Some(err),
            _ => None,
        }
    }
}

/// An error indicating reservation failure.
pub struct ReserveError<T> {
    err: ReserveErrorKind,
    value: T,
}

impl<T> Display for ReserveError<T> {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.err)
    }
}

impl<T> Debug for ReserveError<T> {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.err)
    }
}

impl<T> Error for ReserveError<T> {}

impl<T> ReserveError<T> {

    /// Creates a new error.
    #[inline]
    pub fn new(err: ReserveErrorKind, value: T) -> Self {
        Self {
            err,
            value,
        }
    }

    /// Creates a new allocation error.
    #[inline]
    pub fn alloc_error<E>(err: E, value: T) -> Self
        where E: Error + Send + Sync + 'static
    {
        Self::new(
            ReserveErrorKind::AllocError(AllocError::new(err)),
            value,
        )
    }

    /// Creates a new [`Layout`] error.
    #[inline]
    pub fn layout_error(err: LayoutError, value: T) -> Self {
        Self::new(
            ReserveErrorKind::LayoutError(err),
            value,
        )
    }

    /// Creates a new error indicating that the maximum capacity of a collection has been
    /// exceeded.
    #[inline]
    pub fn max_capacity_exceeded<SizeType: IntoUsize>(
        max_capacity: SizeType,
        requested_capacity: usize,
        value: T,
    ) -> Self {
        Self::new(
            ReserveErrorKind::MaxCapacityExceeded {
                max_capacity: max_capacity.into_usize(),
                requested_capacity,
            },
            value,
        )
    }

    /// Sets the [`recoverable`][1] value of the error.
    ///
    /// [1]: Self::recover_value
    #[inline]
    pub fn with_value<U>(self, value: U) -> ReserveError<U> {
        ReserveError {
            err: self.err,
            value,
        }
    }

    /// Recovers the value of the error.
    #[inline]
    pub fn recover_value(self) -> (T, ReserveErrorKind) {
        (self.value, self.err)
    }
}

impl From<AllocError> for ReserveError<()> {

    #[inline]
    fn from(value: AllocError) -> Self {
        Self::alloc_error(value, ())
    }
}

impl From<LayoutError> for ReserveError<()> {

    #[inline]
    fn from(value: LayoutError) -> Self {
        Self::layout_error(value, ())
    }
}

/// A trait for determining how a given collection grows.
///
/// # Safety
/// Implementing this trait is unsafe because the implementation must adhere to certain rules. For
/// example, [`ReservePolicy::grow`] must never return a value that's less than `required`.
pub unsafe trait ReservePolicy<SizeType = usize>
    where SizeType: FromUsize + IntoUsize,
{

    /// The grow factor used when reallocation is needed.
    fn grow_factor() -> SizeType;

    /// Returns whether the collection can grow.
    ///
    /// The default implementation is [`grow_factor`][1] == 0.
    ///
    /// [1]: Self::grow_factor
    fn can_grow() -> bool {
        Self::grow_factor() != SizeType::ZERO
    }

    /// Tries to grow the capacity.
    fn grow(current: SizeType, required: usize) -> Result<SizeType, ReserveError<()>>;

    /// Grows the capacity, possibly panicking if there's an error.
    fn grow_infallible(current: SizeType, required: usize) -> SizeType;

    /// Returns minimum non-zero capacity, given the size of an element.
    #[inline]
    fn min_non_zero_capacity(size: usize) -> SizeType {
        if size == 1 {
            SizeType::from_usize(8)
        } else if size <= 1024 {
            SizeType::from_usize(4)
        } else {
            SizeType::from_usize(1)
        }
    }

    /// Grows a collection by 1.
    ///
    /// You **should** not manually implement this function.
    ///
    /// You can control the behavior with [`grow_factor`][1], [`min_non_zero_capacity`][2] and
    /// [`grow`][3].
    ///
    /// [1]: Self::grow_factor
    /// [2]: Self::min_non_zero_capacity
    /// [3]: Self::grow
    #[inline]
    fn grow_one(
        current: SizeType,
        elem_layout: Layout,
    ) -> Result<SizeType, ReserveError<()>>
    {
        Ok(Self
            ::grow(
                current,
                (current.into_usize() + 1)
                    .max(current.into_usize() * Self::grow_factor().into_usize())
            )?.max(Self::min_non_zero_capacity(elem_layout.size()))
        )
    }
}
