//! Provides [`ArrayVec`]: a wrapper around a [`MaybeUninit`] array, which is interpreted as a
//! vector.

use core::{
    borrow::{Borrow, BorrowMut},
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    mem::{ManuallyDrop, MaybeUninit, needs_drop},
    ops::{Deref, DerefMut},
    panic,
    ptr::NonNull,
    slice,
};

use crate::reserve::ReserveError;

use crate::alloc_vec::Buffer;

struct BufferGuard<'a, T> {
    buf: Buffer<T>,
    len: &'a mut usize,
}

impl<T> Deref for BufferGuard<'_, T> {

    type Target = Buffer<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl<T> DerefMut for BufferGuard<'_, T> {

    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl<T> Drop for BufferGuard<'_, T> {

    #[inline]
    fn drop(&mut self) {
        *self.len = self.buf.len();
    }
}

/// A wrapper around an array, which is interpreted as a vector.
pub struct ArrayVec<T, const N: usize>
{
    data: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> ArrayVec<T, N>
{

    /// Creates a new empty vector.
    pub fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            len: 0,
        }
    }

    /// Creates a new vector with a length and value.
    pub fn with_len(len: usize, value: T) -> Result<Self, ReserveError<()>>
        where
            T: Clone,
    {
        if len > N {
            return Err(ReserveError::max_capacity_exceeded(N, len, ()))
        }
        let mut data: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };
        for t in data.iter_mut().take(len) {
            t.write(value.clone());
        }
        Ok(Self {
            data,
            len,
        })
    }

    /// Creates a new vector with a length and a closure which returns values.
    pub fn with_len_with<F>(len: usize, mut f: F) -> Result<Self, ReserveError<()>>
        where
            F: FnMut() -> T
    {
        if len > N {
            return Err(ReserveError::max_capacity_exceeded(N, len, ()))
        }
        let mut data: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };
        for t in data.iter_mut().take(len) {
            t.write(f());
        }
        Ok(Self {
            data,
            len,
        })
    }

    /// Maps the vector to another [`ArrayVec`] with different element type.
    pub fn map<U>(&self, f: impl FnMut(&T) -> U) -> ArrayVec<U, N>
    {
        let mut vec = ArrayVec::new();
        vec.extend(self.into_iter().map(f));
        vec
    }

    /// Returns the underlying array, assuming all elements are initialzed.
    ///
    /// # Safety
    /// Values may be unitialized if the array is not full.
    pub unsafe fn assume_init_array(self) -> [T; N] {
        let s = ManuallyDrop::new(self);
        unsafe {
            s.as_ptr()
                .cast::<[T; N]>()
                .read()
        }
    }
}

impl<T, const N: usize> ArrayVec<T, N>
{

    /// Returns the length of the vector.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the vector is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gets the pointer to the vector's data.
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.data.as_ptr() as *const T
    }

    /// Gets a mutable pointer to the vector's data.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.data.as_mut_ptr() as *mut T
    }

    /// Gets a [`NonNull`] pointer to the vector's data.
    #[inline]
    pub fn as_non_null(&mut self) -> NonNull<T> {
        unsafe {
            NonNull::new_unchecked(self.as_mut_ptr())
        }
    }

    /// Returns a slice over the contents of the vector.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const T, self.len) }
    }

    /// Returns a mutable slice over the contents of the vector.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.data.as_ptr() as *mut T, self.len) }
    }

    #[inline]
    fn as_buffer(&mut self) -> BufferGuard<'_, T> {
        unsafe {
            BufferGuard {
                buf: Buffer::from_raw_parts(
                    self.as_mut_ptr(),
                    self.len(),
                    N
                ),
                len: &mut self.len,
            } 
        }
    }

    /// A low-level operation that forces the length of the vector to `new_len`.
    ///
    /// Panics if `new_len` exceeds `capacity` on debug builds.
    ///
    /// # Safety
    /// If `new_len` > current length, elements with elements at indices greater than or equal to
    /// the current length are left uninitialized and if `new_len` < current length, elements at
    /// indices less than or equal to the current length are not dropped when the vector is
    /// [`cleared`][1] or [`dropped`][Drop].
    ///
    /// [1]: Self::clear
    #[inline]
    pub unsafe fn set_len(&mut self, len: usize) {
        #[cfg(debug_assertions)]
        if len > N { panic!("len was larger than capacity") }
        self.len = len;
    } 

    /// Resizes the vector with a clonable value.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is larger than `N`.
    pub fn resize(&mut self, new_len: usize, value: T)
        where
            T: Clone
    {
        self.resize_with(new_len, || value.clone());
    }

    /// Resizes the vector with the given closure.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is larger than `N`.
    pub fn resize_with<F>(&mut self, new_len: usize, mut f: F)
        where
            F: FnMut() -> T
    {
        if new_len > N {
            panic!("max capacity {N} exceeded, with `new_len` {new_len}")
        }
        let ptr = self.as_non_null();
        if new_len > self.len() {
            for i in self.len()..new_len {
                unsafe {
                    ptr.add(i).write(f());
                    self.set_len(i + 1);
                }
            }
        }
        else if new_len < self.len() {
            for i in (new_len..self.len()).rev() {
                unsafe {
                    self.set_len(i);
                    ptr.add(i).drop_in_place();
                }
            }
        }
    }

    /// Tries to resize the vector with the given closure that may return an error.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is larger than `N`.
    pub fn try_resize_with<F, E>(
        &mut self,
        new_len: usize,
        mut f: F,
    ) -> Result<(), E>
        where
            F: FnMut() -> Result<T, E>,
    {
        if new_len > N {
            panic!("max capacity {N} exceeded, with `new_len` {new_len}")
        }
        let ptr = self.as_non_null();
        if new_len > self.len() {
            for i in self.len()..new_len {
                unsafe {
                    ptr.add(i).write(f()?);
                    self.set_len(i + 1);
                }
            }
        }
        else if new_len < self.len() {
            for i in (new_len..self.len()).rev() {
                unsafe {
                    self.set_len(i);
                    ptr.add(i).drop_in_place();
                }
            }
        }
        Ok(())
    }

    /// Appends an element to the end of the vector.
    ///
    /// This panics if the length of the vector exceeds `N`.
    pub fn push(&mut self, value: T) {
        if self.len >= N {
            panic!("maximum capacity of {N} exceeded")
        }
        let ptr = unsafe { self.as_mut_ptr().add(self.len) };
        unsafe { ptr.write(value); };
        self.len += 1;
    }

    /// Appends an element to the end of the vector, returning a mutable reference to the newly
    /// written value.
    ///
    /// This panics if the length of the vector exceeds `N`.
    pub fn push_mut(&mut self, value: T) -> &mut T {
        if self.len >= N {
            panic!("maximum capacity of {N} exceeded")
        }
        unsafe {
            let ptr = self.as_mut_ptr().add(self.len);
            ptr.write(value);
            self.len += 1;
            &mut *ptr
        }
    }

    /// Inserts an element to the specified index.
    ///
    /// Panics if `index` is greater than the length of the vector or if the length of the vector
    /// exceeds `N`.
    pub fn insert(&mut self, index: usize, value: T) {
        if index >= self.len {
            panic!("index {} was out of bounds with len {} when inserting", index, self.len)
        }
        self.as_buffer()
            .insert(value, index, |_| panic!("maximum capacity of {N} exceeded"))
            .ok();
        self.len += 1;
    }

    /// Appends a slice to the end of the vector.
    ///
    /// If the type implements [`Copy`], consider using [`append_copy`][1]
    ///
    /// This panics if the length of the vector exceeds `N`.
    ///
    /// [1]: Self::append_copy
    pub fn append(&mut self, slice: &[T])
        where
            T: Clone
    {
        self.extend(slice.iter().cloned());
    }

    /// Appends a slice to the end of the vector by [`copying`][1] the values.
    ///
    /// This may be faster than [`append`][2] for types implementing [`Copy`].
    ///
    /// This panics if the length of the vector exceeds `N`.
    ///
    /// [1]: Copy
    /// [2]: Self::append
    pub fn append_copy(&mut self, slice: &[T])
        where T: Copy
    {
        let len = self.len + slice.len();
        if len > N {
            panic!("maximum capacity of {N} exceeded with len {len}")
        }
        unsafe {
            slice.as_ptr()
                .copy_to_nonoverlapping(
                    self.as_mut_ptr().add(self.len()),
                    slice.len()
                );
            self.set_len(len);
        }
    } 

    /// Returns a reference to the last element of the vector, or [`None`] if the vector is empty.
    #[inline]
    pub fn last(&self) -> Option<&T> {
        (!self.is_empty()).then(|| unsafe {
            self.as_ptr()
                .add(self.len() - 1)
                .as_ref_unchecked()
        })
    }

    /// Returns a mutable reference to the last element of the vector, or [`None`] if the vector is
    /// empty.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        (!self.is_empty()).then(|| unsafe {
            self.as_mut_ptr()
                .add(self.len() - 1)
                .as_mut_unchecked()
        })       
    }

    /// Removes an element from the specified index.
    ///
    /// Panics if `index` is greater than or equal to the length of the vector.
    pub fn remove(&mut self, index: usize) -> T {
        self.as_buffer()
            .remove(index)
    }

    /// Removes an element at `index` and swaps it with the last element, if any.
    ///
    /// Panics if the index is out of bounds.
    pub fn swap_remove(&mut self, index: usize) -> T {
        self.as_buffer()
            .swap_remove(index)
    }

    /// Removes the last element of the vector and returns it, or [`None`] if the vector is empty.
    pub fn pop(&mut self) -> Option<T> {
        self.as_buffer()
            .pop()
    }

    /// Removes all elements from the vector.
    pub fn clear(&mut self) {
        self.as_buffer()
            .clear();
    }

    /// Retains all elements that satisfy the predicate, preserving the order of elements.
    #[inline]
    pub fn retain<F>(&mut self, mut f: F)
        where F: FnMut(&T) -> bool
    {
        self.retain_mut(|x| f(x));
    }

    /// Retains all elements that satisfy the predicate that takes a mutable reference, preserving
    /// the order of elements
    pub fn retain_mut<F>(&mut self, f: F)
        where F: FnMut(&mut T) -> bool
    {
        self.as_buffer()
            .retain_mut(f);
    }

    /// Removes consecutive repeated elements from the vector according to a closure defined
    /// equality.
    pub fn dedup_by<F>(&mut self, same_bucket: F)
        where F: FnMut(&mut T, &mut T) -> bool
    {
        self.as_buffer()
            .dedup_by(same_bucket);
    }

    /// Removes consecutive repeated elements from the vector according to [`PartialEq`]
    #[inline]
    pub fn dedup(&mut self)
        where T: PartialEq
    {
        self.dedup_by(|a, b| a == b);
    }

    /// Removes consecutive repeated elements that resolve to a key implementing [`PartialEq`]. 
    #[inline]
    pub fn dedup_by_key<F, K>(&mut self, mut key: F)
        where
            F: FnMut(&T) -> K,
            K: PartialEq,
    {
        self.dedup_by(|a, b| key(a) == key(b));
    }

    /// Returns an iterator over the elements of the vector.
    #[inline]
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Returns a mutable iterator over the elements of the vector.
    #[inline]
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
        self.as_mut_slice().iter_mut()
    }
}

/// An iterator that moves out of the vector.
pub struct IntoIter<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    start: usize,
    end: usize,
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {

    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start != self.end {
            let t = unsafe {
                self.data
                    .as_ptr()
                    .add(self.start)
                    .read()
                    .assume_init_read()
            };
            self.start += 1;
            Some(t)
        } else {
            None
        }
    } 

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.end - self.start;
        (size, Some(size))
    }
}

impl<T, const N: usize> DoubleEndedIterator for IntoIter<T, N> {

    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start != self.end {
            self.end -= 1;
            let t = unsafe {
                self.data
                    .as_ptr()
                    .add(self.end)
                    .read()
                    .assume_init_read()
            };
            Some(t)
        } else {
            None
        }
    }
}

impl<T, const N: usize> ExactSizeIterator for IntoIter<T, N> {}

impl<T, const N: usize> Drop for IntoIter<T, N> {

    fn drop(&mut self) {
        unsafe {
            if needs_drop::<T>() {
                for i in self.start..self.end {
                    self.data
                        .as_mut_ptr()
                        .add(i)
                        .drop_in_place();
                }
            }
        }
    }
}

crate::macros::impl_traits! {
    for ArrayVec<T, N: [usize] [const]>
    Drop =>

        fn drop(&mut self) -> () {
            self.as_buffer().clear();
        }
    ,
    Default =>
        
        fn default() -> Self {
            Self::new()
        }
    ,
    AsRef<[T]> =>

        #[inline]
        fn as_ref(&self) -> &[T] {
            self.as_slice()
        }
    ,
    AsMut<[T]> =>

        #[inline]
        fn as_mut(&mut self) -> &mut [T] {
            self.as_mut_slice()
        }
    ,
    Borrow<[T]> =>

        #[inline]
        fn borrow(&self) -> &[T] {
            self.as_slice()
        }
    ,
    BorrowMut<[T]> =>

        #[inline]
        fn borrow_mut(&mut self) -> &mut [T] {
            self.as_mut_slice()
        }
    ,
    From<&[T]> where T: Clone =>
        
        fn from(slice: &[T]) -> Self {
            let mut vec = Self::new();
            vec.append(slice);
            vec
        }
    ,
    Deref =>

        type Target = [T];

        #[inline]
        fn deref(&self) -> &Self::Target {
            self.as_ref()
        }
    ,
    DerefMut =>

        #[inline]
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.as_mut()
        }
    ,
    IntoIterator for &'vec =>

        type Item = &'vec T;
        type IntoIter = slice::Iter<'vec, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    ,
    IntoIterator for mut &'vec =>

        type Item = &'vec mut T;
        type IntoIter = slice::IterMut<'vec, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    ,
    IntoIterator =>

        type Item = T;
        type IntoIter = IntoIter<T, N>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            unsafe {
                let s = MaybeUninit::new(self);
                let data: *const [_; N] = &s.assume_init_ref().data;
                IntoIter {
                    data: data.read(),
                    start: 0,
                    end: s.assume_init_ref().len,
                }
            }
        }
    ,
    Clone where T: Clone =>
    
        #[inline]
        fn clone(&self) -> Self {
            let mut vec = ArrayVec::new();
            vec.append(self);
            vec
        }
    ,
    Eq where T: Eq =>,
    Debug where T: Debug =>

        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            self.as_slice().fmt(f)
        }
    ,
    Display where T: Display =>

        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            if self.len == 0 {
                <str as Display>::fmt("[]", f)?;
                return Ok(())
            }
            <char as Display>::fmt(&'[', f)?;
            for value in &self.as_slice()[0..self.len - 1] {
                value.fmt(f)?;
                <str as Display>::fmt(", ", f)?;
            }
            self.last().unwrap().fmt(f)?;
            <char as Display>::fmt(&']', f)
        }
    ,
    Hash where T: Hash =>

        #[inline]
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.as_slice().hash(state)
        }
    ,
}

impl<T, A, const N: usize> PartialEq<T> for ArrayVec<A, N>
    where
        T: AsRef<[A]>,
        A: PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<A, const N: usize> Extend<A> for ArrayVec<A, N> {

    #[inline]
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        for item in iter {
            self.push(item);
        }
    }
}

impl<T, const N: usize> PartialEq<ArrayVec<T, N>> for [T]
    where
        T: PartialEq
{

    fn eq(&self, other: &ArrayVec<T, N>) -> bool {
        self == other.as_ref()
    }
}

impl<T, const N: usize> PartialEq<ArrayVec<T, N>> for &[T]
    where
        T: PartialEq
{

    fn eq(&self, other: &ArrayVec<T, N>) -> bool {
        *self == other.as_ref()
    }
}

impl<const N: usize, T> From<[T; N]> for ArrayVec<T, N> {
    
    fn from(value: [T; N]) -> Self {
        let mut vec = ArrayVec::new();
        vec.extend(value);
        vec
    }
}

impl<const N: usize, A> FromIterator<A> for ArrayVec<A, N> {

    #[inline]
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut vec = ArrayVec::new();
        vec.extend(iter);
        vec
    }
} 
