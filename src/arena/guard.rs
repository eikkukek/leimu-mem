use super::*;

/// An arena guard returned by [`Arena`] guard methods.
pub struct ArenaGuard<'a, F = False>
    where F: Conditional,
{
    arena: &'a Arena<F>,
    pos_rollback: usize,
}

impl<'a, F> ArenaGuard<'a, F>
    where F: Conditional,
{

    #[inline]
    pub(super) fn new(arena: &'a Arena<F>) -> Self {
        Self {
            pos_rollback: arena.pos.load(atomic::Ordering::Acquire),
            arena,
        }
    }

    /// Returns the total capacity of this guard.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.arena.capacity() - self.pos_rollback
    }

    /// Returns how many bytes has been allocated since the guard was created.
    #[inline]
    pub fn used(&self) -> usize {
        self.arena.used() - self.pos_rollback
    }

    /// Returns how many bytes remaining in the parent arena.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.arena.remaining()
    }

    /// Resets the pointer position to the guard position roll back.
    /// # Safety
    /// Any allocations still in use in the guard's region may be overwritten.
    #[inline]
    pub unsafe fn clear(&mut self) {
        self.arena.pos.store(self.pos_rollback, atomic::Ordering::Release);
    }
}

unsafe impl<'a> Allocator for ArenaGuard<'a, False>
{

    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.arena.allocate_internal(layout)
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
            self.arena.shrink_internal(ptr, old_layout, new_layout)
        }
    }
}

unsafe impl<'a> Allocator for ArenaGuard<'a, True>
{

    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.arena.allocate_internal(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            self.arena.deallocate_internal(ptr, layout);
        }
    }

    #[inline]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>
    {
        unsafe {
            self.arena.shrink_internal(ptr, old_layout, new_layout)
        }
    }
}

impl<'a, F> Drop for ArenaGuard<'a, F>
    where F: Conditional,
{

    fn drop(&mut self) {
        self.arena.pos.store(
            self.pos_rollback,
            atomic::Ordering::Release,
        );
        self.arena.active_guard.store(
            false, atomic::Ordering::Release
        );
    }
}
