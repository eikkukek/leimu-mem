use core::{
    alloc::Layout,
    fmt::Debug,
    mem::needs_drop,
    ptr::NonNull,
};

use crate::{
    size::Size,
    reserve::ReserveError,
};

#[derive(Debug)]
pub(crate) struct Buffer<T, S = usize>
    where
        T: Sized,
        S: Size
{
    ptr: NonNull<T>,
    len: S,
    capacity: S,
}

impl<T: Sized, S: Size> Buffer<T, S> { 

    #[inline]
    pub fn new(allocation: NonNull<[u8]>) -> Self {
        if size_of::<T>() == 0 {
            return Self {
                ptr: allocation.cast(),
                len: S::ZERO,
                capacity: S::ZERO,
            }
        }
        Self {
            ptr: allocation.cast(),
            capacity: S::from_usize(allocation.len() / size_of::<T>()),
            len: S::ZERO,
        }
    }

    #[inline]
    pub const unsafe fn from_raw_parts(
        ptr: *mut T,
        len: S,
        capacity: S,
    ) -> Self {
        unsafe {
            Self {
                ptr: NonNull::new_unchecked(ptr),
                len,
                capacity,
            }
        } 
    }

    #[inline]
    pub const unsafe fn from_parts(
        ptr: NonNull<T>,
        len: S,
        capacity: S,
    ) -> Self {
        Self {
            ptr,
            len,
            capacity,
        }
    }

    #[inline]
    pub const fn empty() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: S::ZERO,
            capacity: S::ZERO,
        }
    }

    #[inline]
    pub const fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[inline]
    pub const fn as_non_null(&mut self) -> NonNull<T> {
        self.ptr
    }

    #[inline]
    pub const fn capacity(&self) -> S {
        self.capacity
    }

    #[inline]
    pub const fn len(&self) -> S {
        self.len
    }

    #[inline]
    pub const unsafe fn set_len(&mut self, len: S) {
        self.len = len;
    }
   
    #[inline] 
    pub unsafe fn reallocate(
        &mut self,
        new: NonNull<[u8]>,
    ) -> (NonNull<u8>, Layout) {
        let ptr: NonNull<T> = new.cast();
        let capacity = S::from_usize(new.len() / size_of::<T>());
        debug_assert!(
            capacity >= self.len,
            "new buffer's capacity {} doesn't fit this buffer's length {}",
            capacity, self.len
        );
        unsafe {
            self.as_ptr().copy_to_nonoverlapping(
                ptr.as_ptr(),
                self.len.into_usize()
            );
        }
        let old_ptr = self.ptr;
        let old_cap = self.capacity;
        self.ptr = ptr;
        self.capacity = capacity;
        unsafe {
            (old_ptr.cast(), Layout::array::<T>(old_cap.into_usize()).unwrap_unchecked())
        }
    }

    #[inline] 
    pub fn deallocate(&mut self) -> Option<(NonNull<u8>, Layout)> {
        self.clear();
        (self.capacity != S::ZERO).then(|| unsafe {
            let cap = self.capacity();
            self.capacity = S::ZERO;
            (
                self.as_non_null()
                    .cast(),
                Layout::array::<T>(cap.into_usize())
                    .unwrap_unchecked()
        )})
    }

    #[inline]
    pub fn insert<F>(
        &mut self, 
        value: T,
        index: usize,
        reserve: F,
    ) -> Result<NonNull<T>, ReserveError<T>>
        where
            F: FnOnce(&mut Self) -> Result<(), ReserveError<()>>
    {
        if index > self.len.into_usize() {
            panic!("index {} was out of bounds with length {} when inserting", index, self.len)
        }
        if self.len >= self.capacity &&
            let Err(err) = reserve(self)
        {
            return Err(err.with_value(value))
        }
        unsafe {
            let res = self.ptr.add(index);
            res.copy_to(res.add(1), self.len.into_usize() - index);
            res.write(value);
            self.len += S::ONE;
            Ok(res)
        }
    }

    pub fn remove(
        &mut self,
        index: usize
    ) -> T {
        if index >= self.len().into_usize() {
            panic!("`index` {} was out of bounds with len {} when attempting to remove an element",
                index, self.len
            );
        }
        let removed = unsafe { self.ptr.add(index).read() };
        unsafe {
            self.ptr.add(index + 1)
                .copy_to(
                    self.ptr.add(index),
                    self.len().into_usize() - index - 1
                );
        }
        self.len -= S::ONE;
        removed
    }

    pub fn swap_remove(
        &mut self,
        index: usize
    ) -> T {
        if index >= self.len.into_usize() {
            panic!("`index` {} was out of bounds with len {} when attempting to remove an element",
                index, self.len
            )
        }
        let removed = unsafe { self.ptr.add(index).read() };
        self.len -= S::ONE;
        if index != self.len.into_usize() {
            unsafe { self.ptr.add(index).write(
                self.ptr.add(self.len.into_usize()).read()
            )}
        }
        removed
    }

    /// Deduplicates consecutive elements.
    #[inline]
    pub fn dedup_by<F>(&mut self, mut same_bucket: F)
        where F: FnMut(&mut T, &mut T) -> bool
    {
        let len = self.len.into_usize();
        if len <= 1 {
            return
        }
        let start = self.as_mut_ptr();
        let mut first_dup = 1;
        while first_dup != len {
            unsafe {
                if same_bucket(&mut *start.add(first_dup), &mut *start.add(first_dup - 1)) {
                    break
                }
            }
            first_dup += 1;
        }
        if first_dup == len {
            return
        }
        struct Guard<'a, T: Sized, S: Size> {
            read: usize,
            write: usize,
            ptr: NonNull<T>,
            out_len: &'a mut S,
        }
        impl<'a, T: Sized, S: Size> Drop for Guard<'a, T, S> {

            fn drop(&mut self) {
                unsafe {
                    let ptr = self.ptr.as_ptr();
                    let len = self.out_len.into_usize();
                    let left = len - self.read;
                    let dropped_ptr = ptr.add(self.write);
                    let valid_ptr = ptr.add(self.read);
                    ::core::ptr::copy_nonoverlapping(valid_ptr, dropped_ptr, left);
                    *self.out_len = S::from_usize(self.write + left + 1);
                }
            }
        }
        let mut guard = Guard {
            read: first_dup,
            write: first_dup - 1,
            ptr: self.ptr,
            out_len: &mut self.len,
        };
        unsafe {
            while guard.read < len {
                let read_ptr = start.add(guard.read);
                let write_ptr = start.add(guard.write);
                if same_bucket(&mut *read_ptr, &mut *write_ptr) {
                    guard.read += 1;
                    read_ptr.drop_in_place();
                } else {
                    let write_ptr = start.add(guard.write + 1);
                    ::core::ptr::copy(read_ptr, write_ptr, 1);
                    guard.read += 1;
                    guard.write += 1;
                }
            }
        }
        *guard.out_len = S::from_usize(guard.write + 1);
        ::core::mem::forget(guard);
    }

    /// Retains elements satisfying the given predicate.
    #[inline]
    pub fn retain_mut<F>(&mut self, mut f: F)
        where F: FnMut(&mut T) -> bool
    {
        let len = self.len.into_usize();
        if len == 0 {
            return
        }
        struct Guard<'a, T, S: Size> {
            read: usize,
            write: usize,
            ptr: NonNull<T>,
            out_len: &'a mut S,
        }
        impl<T, S: Size> Drop for Guard<'_, T, S> {

            fn drop(&mut self) {
                unsafe {
                    let ptr = self.ptr.as_ptr();
                    let len = self.out_len.into_usize();
                    let remaining = len - self.read;
                    let read_ptr = ptr.add(self.read);
                    let write_ptr = ptr.add(self.write);
                    ::core::ptr::copy(read_ptr, write_ptr, remaining);
                    *self.out_len = S::from_usize(self.write + remaining);
                }
            }
        }
        let mut guard = Guard {
            read: 0,
            write: 0,
            ptr: self.ptr,
            out_len: &mut self.len,
        };
        let start = guard.ptr.as_ptr();
        unsafe {
            while guard.read < len {
                let read_ptr = start.add(guard.read);
                if !f(&mut *read_ptr) {
                    guard.read += 1;
                    read_ptr.drop_in_place();
                } else {
                    let write_ptr = start.add(guard.write);
                    ::core::ptr::copy(read_ptr, write_ptr, 1);
                    guard.read += 1;
                    guard.write += 1;
                }
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == S::ZERO { return None }
        unsafe {
            self.set_len(self.len - S::ONE);
        }
        let ptr = unsafe { self.as_ptr().add(self.len().into_usize()) };
        Some(unsafe { ptr.read() })
    }

    #[inline]
    pub fn clear(&mut self) {
        unsafe {
            debug_assert!(self.len() <= self.capacity());
            if needs_drop::<T>() && self.capacity() == S::ZERO {
                for i in (0..self.len.into_usize()).rev() {
                    self.set_len(S::from_usize(i));
                    self.ptr.add(i).drop_in_place();
                }
            }
            self.set_len(S::ZERO);
        }
    }
}
