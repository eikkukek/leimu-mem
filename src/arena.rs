//! A simple, safe and atomic arena allocator implementing [`Allocator`].
//!
//! # Examples
//! ``` rust
//! use std::alloc::{Allocator, Layout};
//! use leimu_mem::arena::Arena;
//!
//! let arena = Arena::new(64).unwrap();
//! {
//!     let guard = arena.guard();
//!     let mut vec = Vec::with_capacity_in(5, &guard);
//!     vec.extend([0, 1, 2, 3, 4]);
//!     assert_eq!(arena.used(), vec.capacity() * size_of::<i32>());
//!     // Guard already active
//!     assert!(arena.try_guard().is_none());
//!     // Can't allocate directly while guard is active
//!     assert!(arena.allocate(Layout::new::<i32>()).is_err())
//! }
//! // Guard dropped
//! assert_eq!(arena.used(), 0);
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::all)]

mod guard;

#[cfg(test)]
mod test;

use std::alloc::{
    AllocError, Allocator, Global, Layout, LayoutError,
};

use core::{
    ptr::NonNull,
    marker::PhantomData,
};

use crate::{conditional, align_up};

use portable_atomic as atomic;

use atomic::{AtomicUsize, AtomicBool};

use conditional::{Conditional, False, True};

pub use guard::ArenaGuard;

/// Gets the alignment used when allocating an arena.
///
/// Equal to `32`.
pub const fn max_align() -> usize {
    32
}

/// The allocator implementation.
///
/// Takes in a type-level conditional indicating whether the arena has a fallback using the [`Global`]
/// allocator when it runs out of space.
///
/// Is [`Send`] and [`Sync`].
pub struct Arena<F: Conditional = False> {
    data: NonNull<[u8]>,
    pos: AtomicUsize,
    active_guard: AtomicBool,
    _marker: PhantomData<F>,
}

impl Arena {

    /// Creates a new arena with the size of `capacity` bytes.
    ///
    /// Panics if [`Global`] fails to allocate.
    pub fn new(capacity: usize) -> Result<Self, LayoutError> {
        let layout = Layout
            ::from_size_align(capacity, max_align())?;
        let ptr = Global
            .allocate(layout)
            .expect("global alloc failed");
        Ok(Self {
            data: ptr,
            pos: AtomicUsize::new(0),
            active_guard: AtomicBool::new(false),
            _marker: PhantomData
        })
    }
}

impl Arena<True> {

    /// Creates a new arena with a fallback to [`Global`] allocator.
    ///
    /// Panics if [`Global`] fails to allocate.
    pub fn with_fallback(capacity: usize) -> Result<Self, LayoutError> {
        let layout = Layout
            ::from_size_align(capacity, max_align())?;
        let data = Global
            .allocate(layout)
            .expect("global alloc a failed");
        Ok(Self {
            data,
            pos: AtomicUsize::new(0),
            active_guard: AtomicBool::new(false),
            _marker: PhantomData
        })
    }
}

impl<F: Conditional> Arena<F> { 

    /// Returns the total size of the arena.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Returns how many bytes have been already used.
    #[inline]
    pub fn used(&self) -> usize {
        self.pos.load(atomic::Ordering::Relaxed)
    }

    /// Returns how much space is remaining.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity() - self.pos.load(atomic::Ordering::Relaxed)
    }

    fn is_within_arena(&self, ptr: NonNull<u8>) -> bool {
        let ptr_addr = ptr.as_ptr() as usize;
        let arena_addr = self.data.as_ptr().cast::<u8>() as usize;
        ptr_addr >= arena_addr || ptr_addr < arena_addr + self.capacity()
    }

    /// Resets the arena's internal pointer to zero.
    ///
    /// This will panic if there's a [`guard`][1] active.
    ///
    /// # Safety
    /// Previous allocations may be overwritten after this.
    ///
    /// [1]: ArenaGuard
    #[inline]
    pub unsafe fn clear(&self) {
        assert!(
            !self.active_guard.load(atomic::Ordering::Acquire),
            "attempting to clear when a guard is active"
        );
        self.pos.store(0, atomic::Ordering::Release);
    }

    /// Consumes self, and returns the inner raw pointer and size of the arena.
    #[inline]
    pub fn into_raw_parts(self) -> NonNull<[u8]> {
        let s = core::mem::ManuallyDrop::new(self);
        s.data
    }

    /// Creates an [`arena guard`][1], which resets the position to the current position when it's
    /// dropped.
    ///
    /// Once a guard is active, all allocations made directly from this arena will return an error.
    ///
    /// Only one guard can be active at a time and this will panic if a guard is already active.
    ///
    /// [1]: ArenaGuard
    #[inline]
    pub fn guard(&self) -> ArenaGuard<'_, F> {
        self.try_guard().expect("guard already active")
    }

    /// Tries to creates an [`arena guard`][1], which resets the position to the current position
    /// when it's dropped.
    ///
    /// Once a guard is active, all allocations made directly from this arena will return an error.
    ///
    /// Only one guard can be active at a time and this will return [`None`] if a guard is already
    /// active.
    ///
    /// [1]: ArenaGuard
    #[inline]
    pub fn try_guard(&self) -> Option<ArenaGuard<'_, F>> {
        (!self.active_guard.swap(true, atomic::Ordering::AcqRel)).then(||
            ArenaGuard::new(self)
        )
    }

    unsafe fn allocate_unhecked(
        &self,
        layout: Layout,
    ) -> Option<NonNull<[u8]>> {
        if layout.size() == 0 {
            return Some(self.data)
        }
        let mut aligned_start = 0;
        self.pos.fetch_update(
            atomic::Ordering::AcqRel,
            atomic::Ordering::Acquire,
            |pos|  {
                let start = self.data.as_ptr().addr() + pos;
                aligned_start = align_up!(start, layout.align());
                let end = aligned_start + layout.size();
                if end > self.data.as_ptr().addr() + self.capacity() {
                    return None
                }
                Some(end - self.data.as_ptr().addr())
            }
        ).ok()?;
        unsafe {
            Some(
                NonNull::slice_from_raw_parts(
                    NonNull::new_unchecked(aligned_start as *mut u8),
                    layout.size(),
                ),
            )
        }
    }
}

impl Arena { 

    fn allocate_internal(
        &self,
        layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        if layout.align() > max_align() {
            Err(AllocError)
        } else {
            unsafe {
                self.allocate_unhecked(layout)
                    .ok_or(AllocError)
            }
        }
    }

    unsafe fn shrink_internal(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        if !self.is_within_arena(ptr) {
            unsafe {
                Global.shrink(ptr, old_layout, new_layout)
            }
        } else {
            if old_layout.align() < new_layout.align() {
                let new_ptr = self.allocate_internal(new_layout)?;
                unsafe {
                    ptr.copy_to_nonoverlapping(new_ptr.cast(), new_layout.size());
                }
                Ok(new_ptr)
            } else {
                Ok(NonNull::slice_from_raw_parts(ptr.cast(), new_layout.size()))
            }
        }
    }
}

impl Arena<True> {

    fn allocate_internal(
        &self,
        layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        if layout.align() > max_align() {
            Global.allocate(layout)
        } else {
            let ptr = unsafe { self.allocate_unhecked(layout) };
            if let Some(ptr) = ptr {
                Ok(ptr)
            } else {
                Global.allocate(layout)
            }
        }
    }

    unsafe fn deallocate_internal(&self, ptr: NonNull<u8>, layout: Layout) {
        if !self.is_within_arena(ptr) {
            unsafe {
                Global.deallocate(ptr, layout);
            }
        }
    }

    unsafe fn shrink_internal(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        debug_assert!(
            self.is_within_arena(ptr),
            "`ptr` not allocated from arena"
        );
        if old_layout.align() < new_layout.align() {
            let new_ptr = self.allocate_internal(new_layout)?;
            unsafe {
                ptr.copy_to_nonoverlapping(new_ptr.cast(), new_layout.size());
            }
            Ok(new_ptr)
        } else {
            Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
        }
    }
}

unsafe impl Allocator for Arena {

    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if self.active_guard.load(atomic::Ordering::Acquire) {
            return Err(AllocError)
        }
        self.allocate_internal(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        unsafe {
            self.shrink_internal(ptr, old_layout, new_layout)
        }
    }
}

unsafe impl Allocator for Arena<True> {

    #[inline]
    fn allocate(
        &self,
        layout: Layout
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        if self.active_guard.load(atomic::Ordering::Acquire) {
            return Err(AllocError)
        }
        self.allocate_internal(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            self.deallocate_internal(ptr, layout);
        }
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        unsafe {
            self.shrink_internal(ptr, old_layout, new_layout)
        }
    }
}

unsafe impl<F: Conditional> Send for Arena<F> {}
unsafe impl<F: Conditional> Sync for Arena<F> {}

impl<F: Conditional> Drop for Arena<F> {

    fn drop(&mut self) {
        unsafe {
            let layout = Layout
                ::from_size_align_unchecked(
                    self.capacity(), max_align()
                );
            Global.deallocate(self.data.cast(), layout);
        }
    }
}
