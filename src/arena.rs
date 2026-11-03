//! Safe and atomic arena allocator implementing [`LocalAlloc`].

mod guard;

use core::{
    ptr::NonNull,
    marker::PhantomData,
    error,
    fmt::{self, Display},
};

use portable_atomic as atomic;

use atomic::{AtomicUsize, AtomicBool};

use crate::{
    align_up,
    alloc::{
        AllocError, Layout, LocalAlloc
    },
    reserve::ReserveError,
    conditional::{Conditional, False, True},
};

pub use guard::ArenaGuard;

/// The allocation error used by [`Arena`].
#[derive(Debug)]
pub enum Error {
    /// Indicates that an allocation exceeded [`max_align`].
    MaximumAlignmentExceeded {
        /// The requested alignment.
        requested: usize,
    },
    /// Indicates that the arena is full.
    ArenaFull {
        /// The capacity of the arena.
        capacity: usize,
    },
    /// Indicates invalid usage of the arena while a guard is active.
    GuardActive,
}

impl Display for Error {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaximumAlignmentExceeded { requested } =>
                write!(f, "maximum supported alignment for arenas {} exceeded with {requested}",
                    max_align()
                ),
            Self::ArenaFull { capacity } => write!(f, "arena is full with capacity {capacity}"),
            Self::GuardActive => write!(f, "a guard is active"),
        }
    }
}

impl error::Error for Error {}

/// Gets the alignment used when allocating an arena.
///
/// Equal to `32`.
pub const fn max_align() -> usize {
    32
}

/// Safe and atomic arena allocator implementing [`LocalAlloc`].
///
/// Is [`Send`] and [`Sync`].
///
/// # Examples
/// ``` rust
/// use leimu_mem::alloc::{LocalAlloc, Layout};
/// use leimu_mem::arena::Arena;
/// use leimu_mem::alloc_vec::FixedVec;
///
/// let arena = Arena::new(64).unwrap();
/// {
///     let guard = arena.guard();
///     let mut vec = FixedVec::with_capacity_in(5, &guard).unwrap();
///     vec.extend([0, 1, 2, 3, 4]);
///     assert_eq!(arena.used(), 5 * size_of::<i32>());
///     // Guard already active
///     assert!(arena.try_guard().is_none());
///     unsafe {
///         // Guard active
///         assert!(arena.allocate(Layout::new::<i32>()).is_err())
///     }
/// }
/// // Guard dropped
/// assert_eq!(arena.used(), 0);
/// ```
pub struct Arena<F: Conditional = False> {
    ptr: NonNull<u8>,
    capacity: usize,
    pos: AtomicUsize,
    active_guard: AtomicBool,
    _marker: PhantomData<F>,
}

impl Arena {

    /// Creates a new arena with the size of `capacity` bytes.
    pub fn new(capacity: usize) -> Result<Self, ReserveError<()>> {
        let layout = Layout
            ::from_size_align(capacity, max_align())?;
        let (ptr, _) = ()
            .allocate(layout)?;
        Ok(Self {
            ptr,
            capacity,
            pos: AtomicUsize::new(0),
            active_guard: AtomicBool::new(false),
            _marker: PhantomData
        })
    }
}

impl Arena<True> {

    /// Creates a new arena with a fallback to [`GlobalAlloc`][1].
    ///
    /// [1]: std::alloc::alloc
    pub fn with_fallback(capacity: usize) -> Result<Self, ReserveError<()>> {
        let layout = Layout
            ::from_size_align(capacity, max_align())?;
        let (ptr, _) = ()
            .allocate(layout)?;
        Ok(Self {
            ptr,
            capacity,
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
        self.capacity
    }

    /// Returns how many bytes have been already used.
    #[inline]
    pub fn used(&self) -> usize {
        self.pos.load(atomic::Ordering::Relaxed)
    }

    /// Returns how much space is remaining.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity - self.pos.load(atomic::Ordering::Relaxed)
    }

    fn is_within_arena(&self, ptr: NonNull<u8>) -> bool {
        let ptr_addr = ptr.as_ptr() as usize;
        let arena_addr = self.ptr.as_ptr() as usize;
        ptr_addr >= arena_addr || ptr_addr < arena_addr + self.capacity
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
    pub fn into_raw_parts(self) -> (NonNull<u8>, usize) {
        let s = core::mem::ManuallyDrop::new(self);
        (s.ptr, s.capacity)
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
    ) -> Option<(NonNull<u8>, usize)> {
        if layout.size() == 0 {
            return Some((
                self.ptr,
                0,
            ))
        }
        let mut aligned_start = 0;
        self.pos.fetch_update(
            atomic::Ordering::AcqRel,
            atomic::Ordering::Acquire,
            |pos|  {
                let start = self.ptr.as_ptr() as usize + pos;
                aligned_start = align_up(start, layout.align());
                let end = aligned_start + layout.size();
                if end > self.ptr.as_ptr() as usize + self.capacity {
                    return None
                }
                Some(end - self.ptr.as_ptr() as usize)
            }
        ).ok()?;
        unsafe {
            Some((
                NonNull::new_unchecked(aligned_start as *mut u8),
                layout.size(),
            ))
        }
    }
}

impl Arena { 

    fn allocate_internal(
        &self,
        layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError>
    {
        if layout.align() > max_align() {
            Err(AllocError::new(Error::MaximumAlignmentExceeded { requested: layout.align() }))
        } else {
            unsafe {
                self.allocate_unhecked(layout)
                    .ok_or_else(||
                        AllocError::new(Error::ArenaFull { capacity: self.capacity })
                    )
            }
        }
    }

    unsafe fn shrink_internal(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        if !self.is_within_arena(ptr) {
            unsafe {
                ().shrink(ptr, old_layout, new_layout)
            }
        } else {
            if old_layout.align() < new_layout.align() {
                let (new_ptr, bytes) = self.allocate_internal(new_layout)?;
                unsafe {
                    ptr.copy_to_nonoverlapping(new_ptr, old_layout.size());
                }
                Ok((new_ptr, bytes))
            } else {
                Ok((ptr, new_layout.size()))
            }
        }
    }
}

impl Arena<True> {

    fn allocate_internal(
        &self,
        layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError>
    {
        if layout.align() > max_align() {
            ().allocate(layout)
        } else {
            let ptr = unsafe { self.allocate_unhecked(layout) };
            if let Some(ptr) = ptr {
                Ok(ptr)
            } else {
                ().allocate(layout)
            }
        }
    }

    unsafe fn deallocate_internal(&self, ptr: NonNull<u8>, layout: Layout) {
        if !self.is_within_arena(ptr) {
            unsafe {
                ().deallocate(ptr, layout);
            }
        }
    }

    unsafe fn shrink_internal(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<(NonNull<u8>, usize), AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        debug_assert!(
            self.is_within_arena(ptr),
            "`ptr` not allocated from arena"
        );
        if old_layout.align() < new_layout.align() {
            let (new_ptr, bytes) = self.allocate_internal(new_layout)?;
            unsafe {
                ptr.copy_to_nonoverlapping(new_ptr, old_layout.size());
            }
            Ok((new_ptr, bytes))
        } else {
            Ok((ptr, new_layout.size()))
        }
    }
}

unsafe impl LocalAlloc for Arena {

    #[inline]
    fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError> {
        if self.active_guard.load(atomic::Ordering::Acquire) {
            return Err(AllocError::new(Error::GuardActive))
        }
        self.allocate_internal(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

unsafe impl LocalAlloc for Arena<True> {

    #[inline]
    fn allocate(
        &self,
        layout: Layout
    ) -> Result<(NonNull<u8>, usize), AllocError>
    {
        if self.active_guard.load(atomic::Ordering::Acquire) {
            return Err(AllocError::new(Error::GuardActive))
        }
        self.allocate_internal(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            self.deallocate_internal(ptr, layout);
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
                    self.capacity, max_align()
                );
            ().deallocate(self.ptr, layout);
        }
    }
}
