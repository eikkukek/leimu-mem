//! Custom allocator API.
//!
//! This module will most likely be deprecated when the [`Allocator`][1] trait is stabilized.
//!
//! # New traits
//! - [`LocalAlloc`]: A trait for local, owned allocators.
//!
//! # Re-exports from the [`std::alloc`] module
//! - [`Layout`]
//! - [`LayoutError`]
//!
//! [1]: std::alloc::Allocator

pub mod pack;

pub use core::alloc::{Layout, LayoutError};

use core::ptr::NonNull;

pub use pack::{pack_block, pack_alloc};

#[cfg(not(feature = "std"))]
mod no_std {

    use core::{
        error::Error,
        fmt::{self, Display, Debug},
    };

    /// Indicates an allocation error.
    #[derive(Debug)]
    pub struct AllocError;

    impl AllocError {
        /// Creates a new allocation error.
        #[inline]
        pub fn new<E>(_err: E) -> Self
            where E: Error + Send + Sync + 'static
        {
            Self
        }
    }

    impl Display for AllocError {

        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "allocation error")
        }
    }

    impl Error for AllocError {}
}

#[cfg(feature = "std")]
mod std_features {

    use std::alloc::{alloc, dealloc};

    use std::boxed::Box;
    use core::{
        ops::Deref,
        error::Error,
        fmt::{self, Display, Debug},
    };

    use super::*;

    /// Indicates an allocation error.
    #[derive(Debug)]
    pub struct AllocError(Box<dyn Error + Send + Sync>);

    impl AllocError {
        /// Creates a new allocation error.
        #[inline]
        pub fn new<E>(err: E) -> Self
            where E: Error + Send + Sync + 'static
        {
            Self(Box::new(err))
        }
    }

    impl Display for AllocError {

        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for AllocError {

        #[inline]
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(self.0.deref())
        }
    }

    unsafe impl<T> LocalAlloc for std::sync::Arc<T>
        where T: ?Sized + LocalAlloc,
    {

        #[inline]
        fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError> {
            self.deref().allocate(layout)
        }

        #[inline]
        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            unsafe {
                self.deref().deallocate(ptr, layout)
            }
        }
    }

    unsafe impl LocalAlloc for () {

        #[inline]
        fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError> {
            if layout.size() == 0 {
                return Ok((
                    NonNull
                        ::new(layout.align() as *mut u8)
                        .expect("`layout.align()` is zero"),
                    0,
                ))
            }
            #[derive(Debug)]
            struct GlobalAllocError {
                layout: Layout,
            }

            impl Display for GlobalAllocError {

                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "GlobalAlloc failed for layout {:?}", self.layout)
                }
            }
            impl Error for GlobalAllocError {}
            let ptr = unsafe { alloc(layout) };

            NonNull
                ::new(ptr)
                .map(|ptr| (ptr, layout.size()))
                .ok_or_else(|| AllocError::new(GlobalAllocError { layout }))
        }

        #[inline]
        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            if layout.size() != 0 {
                unsafe { dealloc(ptr.as_ptr(), layout) }
            }
        }
    }
}

#[cfg(feature = "std")]
pub use std_features::AllocError;

#[cfg(not(feature = "std"))]
pub use no_std::AllocError;

/// A trait for local, owned allocators.
///
/// If the "std" feature is enabled, this is implemented for `()` which uses [`GlobalAlloc`][2] for all
/// allocations.
///
/// # Safety
/// It has to be ensured that the allocations made by allocators implementing this trait return
/// valid, aligned pointers that remain valid until either the memory is [`deallocated`][1] or the
/// allocator is dropped.
///
/// [1]: LocalAlloc::deallocate
/// [2]: std::alloc::GlobalAlloc
pub unsafe trait LocalAlloc
{
 
    /// Attempts to allocate memory as described by the given layout.
    ///
    /// On success, this returns a ([`NonNull`], [`usize`]) tuple containing a pointer meeting the
    /// size and alignment guarantees of `layout` and the actual size of the allocated block, which
    /// may be more than the size of `layout`.
    ///
    /// The returned block of memory remains valid as long as it is currently allocated (i.e. not [`deallocated`][1])
    /// and as long as the allocator is not dropped.
    ///
    /// The allocated block of memory may or may not be initialized.
    ///
    /// [`Err`] is returned if either memory is exhausted or `layout` does not meet the allocator's
    /// size or alignment constraints.
    ///
    /// [1]: LocalAlloc::deallocate
    fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError>;

    /// Deallocates a previously allocated block at `ptr` with the given `layout`.
    ///
    /// # Safety
    /// - `ptr` must be a block of memory currently allocated via this allocator
    /// - `layout` must fit that block of memory
    ///
    /// Otherwise the behavior is undefined.
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);

    /// Attempts to shrink the memory block.
    ///
    /// Returns a new ([`NonNull`], [`usize`]) tuple containing a pointer and the actual size of the
    /// allocated block. The pointer is suitable for holding data described by `new_layout`.
    ///
    /// If this returns [`Ok`], the ownership of the block of memory referenced by `ptr` has been
    /// returned to this allocator and any access to the old `ptr` is undefined behaviour.
    ///
    /// If this method returns [`Err`], the ownership of the memory block remains unchanged and the
    /// contents memory block are unchanged.
    ///
    /// # Safety
    /// - `ptr` must point to a currently allocated block via this allocator
    /// - `old_layout` must fit that block of memory
    /// - `new_layout.size()` must be smaller than or equal to `old_layout.size()`.
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        let new_ptr = self.allocate(new_layout)?;
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                ptr.as_ptr(),
                new_ptr.0.as_ptr(),
                new_layout.size()
            );
            self.deallocate(ptr, old_layout);
        }
        Ok(new_ptr)
    }
}

unsafe impl<T> LocalAlloc for &T
    where
        T: LocalAlloc + ?Sized
{
    fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError> {
        (*self).allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            (*self).deallocate(ptr, layout);
        }
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError>
    {
        unsafe {
            (*self).shrink(ptr, old_layout, new_layout)
        }
    }
}
