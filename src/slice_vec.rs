//! Provides [`SliceVec`]: a wrapper around a slice of [`MaybeUninit`], which is interpreted as a
//! vector.

use core::{
    fmt::{self, Debug, Display},
    marker::PhantomData,
    mem::{MaybeUninit, ManuallyDrop},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice,
};

use crate::{
    alloc::{Layout, LocalAlloc},
    reserve::*,
    int::{FromUsize, IntoUsize},
    alloc_vec::Buffer,
};

/// A reference to a [`MaybeUninit`] slice, which is interpreted as a vector.
///
/// This can be useful when you have a raw pointer which you want to use like a vector, for example
/// with custom allocators.
///
/// # Examples
/// ``` rust
/// use std::mem::MaybeUninit;
/// use leimu_mem::slice_vec::SliceVec;
///
/// let mut array: [MaybeUninit<u32>; 4] = unsafe {
///     MaybeUninit
///         ::uninit()
///         .assume_init()
/// };
/// let mut vec = SliceVec::<_, u32>::new(&mut array);
/// vec.extend([1, 2, 3, 4]);
/// assert!(vec.is_full());
/// assert_eq!(&vec, &[1, 2, 3, 4]);
/// ```
pub struct SliceVec<'a, T, SizeType = usize>
    where 
        SizeType: IntoUsize + FromUsize,
{
    buf: Buffer<T, SizeType>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T, SizeType> SliceVec<'a, T, SizeType>
    where SizeType: IntoUsize + FromUsize
{

    /// Creates a new [`SliceVec`] from `&mut MaybeUninit<T>` slice.
    #[inline]
    pub fn new(
        slice: &'a mut [MaybeUninit<T>]
    ) -> Self
    {
        unsafe {
            Self {
                buf: Buffer::from_raw_parts(
                    slice.as_mut_ptr().cast(),
                    SizeType::ZERO,
                    SizeType::from_usize(slice.len()),
                ),
                _marker: PhantomData,
            }
        } 
    }

    /// Creates an empty [`SliceVec`].
    #[inline]
    pub fn empty() -> Self {
        Self {
            buf: Buffer::empty(),
            _marker: PhantomData,
        }
    }

    /// Creates a [`SliceVec`] with the given capacity, allocting its memory from `A`.
    ///
    /// The pointer and the used [`Layout`] can be retrieved with the [`forget`][Self::forget]
    /// method.
    ///
    /// Remember to [`clear`][Self::clear] the vector before deallocating the pointer so the vector
    /// won't use a dangling pointer when it's dropped.
    pub fn with_capacity_in<A: LocalAlloc + ?Sized>(
        capacity: SizeType,
        alloc: &'a A,
    ) -> Result<Self, ReserveError<()>>
    {
        Ok(Self {
            buf: Buffer::new(alloc.allocate(
                Layout::array::<T>(capacity.into_usize())?
            )?),
            _marker: PhantomData,
        })
    }

    /// Returns the length of the vector.
    #[inline]
    pub fn len(&self) -> SizeType {
        self.buf.len()
    }

    /// Returns the capacity of the vector.
    #[inline]
    pub fn capacity(&self) -> SizeType {
        self.buf.capacity()
    }

    /// Returns whether the vector is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == SizeType::ZERO
    }

    /// Returns whether the vector is full for its current capacity, i.e. whether its length is
    /// equal to its capacity.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Gets the pointer to the vector's data.
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.buf.as_ptr()
    }

    /// Gets a mutable pointer to the vector's data.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr()
    }

    /// Gets a [`NonNull`] pointer to the vector's data.
    #[inline]
    pub fn as_non_null(&mut self) -> NonNull<T> {
        self.buf.as_non_null()
    }

    /// Returns a slice over the contents of the vector.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe {
            slice::from_raw_parts(
                self.as_ptr(),
                self.len().into_usize(),
            )
        }
    }

    /// Returns a mutable slice over the contents of the vector.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe {
            slice::from_raw_parts_mut(
                self.as_mut_ptr(),
                self.len().into_usize()
            )
        }
    }

    /// Takes the inner mutable reference to the slice, consuming self.
    #[inline]
    pub fn take(self) -> &'a mut [MaybeUninit<T>] {
        let mut vec = ManuallyDrop::new(self);
        unsafe {
            slice::from_raw_parts_mut(
                vec.as_mut_ptr().cast(),
                vec.capacity().into_usize(),
            )
        }
    } 

    /// Decomposes a [`SliceVec`] into its raw components: (NonNull pointer, length, capacity).
    #[inline]
    pub fn into_parts(self) -> (NonNull<T>, SizeType, SizeType) {
        let mut s = ManuallyDrop::new(self);
        (
            s.as_non_null(),
            s.len(),
            s.capacity(),
        )
    }

    /// Creates a new [`SliceVec`] directly from a [`NonNull`] pointer, a length and a capacity.
    ///
    /// # Safety
    /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
    /// aren't checked:
    /// - `ptr` must point to valid currently allocated memory.
    /// - `T` needs to have the same alignment as what `ptr` was allocated with.
    /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
    ///   with.
    /// - `len` must be less than or equal to `capacity`.
    /// - The first `len` values must be properly initialized values of type `T`.
    /// - `capacity` needs to fit the layout size that the pointer was allocated with.
    /// - The allocated size in bytes must be no larger than `isize::MAX`.
    #[inline]
    pub unsafe fn from_parts(
        ptr: NonNull<T>,
        len: SizeType,
        capacity: SizeType,
    ) -> Self {
        unsafe {
            Self {
                buf: Buffer::from_parts(ptr, len, capacity),
                _marker: PhantomData,
            }
        }
    }

    /// Decomposes a [`SliceVec`] into its raw components: (pointer, length, capacity).
    #[inline]
    pub fn into_raw_parts(self) -> (*mut T, SizeType, SizeType) {
        let mut s = ManuallyDrop::new(self);
        (
            s.as_mut_ptr(),
            s.len(),
            s.capacity(),
        )
    }

    /// Creates a new [`SliceVec`] directly from a raw pointer, a length and a capacity.
    ///
    /// # Safety
    /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
    /// aren't checked:
    /// - `ptr` must point to valid currently allocated memory.
    /// - `T` needs to have the same alignment as what `ptr` was allocated with.
    /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
    ///   with.
    /// - `len` must be less than or equal to `capacity`.
    /// - The first `len` values must be properly initialized values of type `T`.
    /// - `capacity` needs to fit the layout size that the pointer was allocated with.
    /// - The allocated size in bytes must be no larger than `isize::MAX`.
    #[inline]
    pub unsafe fn from_raw_parts(
        ptr: *mut T,
        len: SizeType,
        capacity: SizeType,
    ) -> Self {
        unsafe {
            Self {
                buf: Buffer::from_raw_parts(ptr, len, capacity),
                _marker: PhantomData,
            }
        }
    }

    /// A low-level operation that forces the length of the vector to `new_len`.
    ///
    /// Panics if `new_len` exceeds `capacity` on debug builds.
    ///
    /// # Safety
    /// `new_len` must be less than or equal to the vector's capacity.
    ///
    /// If `new_len` > current length, elements with elements at indices greater than or equal to
    /// the current length are left uninitialized and if `new_len` < current length, elements at
    /// indices less than or equal to the current length are not dropped when the vector is
    /// [`cleared`][1] or [`dropped`][Drop].
    ///
    /// [1]: Self::clear
    #[inline]
    pub unsafe fn set_len(&mut self, new_len: SizeType) {
        debug_assert!(
            new_len <= self.capacity(),
            "`new_len` is larger than capacity",
        );
        unsafe {
            self.buf.set_len(new_len);
        }
    }

    /// Resizes the vector with a clonable value.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is greater than the vector's capacity.
    pub fn resize(&mut self, new_len: SizeType, value: T)
        where
            T: Clone
    {
        self.resize_with(new_len, || value.clone());
    }

    /// Resizes the vector with the given closure.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is greater than the vector's capacity.
    pub fn resize_with<F>(&mut self, new_len: SizeType, mut f: F)
        where
            F: FnMut() -> T
    {
        if new_len > self.capacity() {
            panic!("`new_len` is greater than this `self.capacity()`")
        }
        let ptr = self.buf.as_mut_ptr();
        if new_len > self.len() {
            for i in self.len().into_usize()..new_len.into_usize() {
                unsafe {
                    ptr.add(i).write(f());
                    self.buf.set_len(SizeType::from_usize(i + 1));
                }
            }
        }
        else if new_len < self.len() {
            for i in (new_len.into_usize()..self.len().into_usize()).rev() {
                unsafe {
                    self.buf.set_len(SizeType::from_usize(i));
                    ptr.add(i).drop_in_place();
                }
            }
        }
    }

    /// Tries to resize the vector with the given closure that may return an error.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This panics if `new_len` is greater than the vector's capacity.
    pub fn try_resize_with<F, E>(
        &mut self,
        new_len: SizeType,
        mut f: F,
    ) -> Result<(), E>
        where
            F: FnMut() -> Result<T, E>,
    {
        if new_len > self.capacity() {
            panic!("`new_len` is greater than `self.capacity()`")
        }
        let ptr = self.buf.as_mut_ptr();
        if new_len > self.len() {
            for i in self.len().into_usize()..new_len.into_usize() {
                unsafe {
                    ptr.add(i).write(f()?);
                    self.buf.set_len(SizeType::from_usize(i + 1));
                }
            }
        }
        else if new_len < self.len() {
            for i in (new_len.into_usize()..self.len().into_usize()).rev() {
                unsafe {
                    self.buf.set_len(SizeType::from_usize(i));
                    ptr.add(i).drop_in_place();
                }
            }
        }
        Ok(())
    }

    /// Appends an element to the end of the vector.
    ///
    /// Panics if the capacity of the vector is exceeded.
    #[inline]
    pub fn push(&mut self, value: T) {
        self.push_mut(value);
    }

    /// Appends an element to the end of the vector, returning a mutable reference to the newly
    /// written value.
    ///
    /// Panics if the capacity of the vector is exceeded.
    pub fn push_mut(&mut self, value: T) -> &mut T {
        unsafe {
            self.buf.insert(
                value,
                self.len().into_usize(),
                |_| {
                    panic!("`self.len()` exceeds `self.capacity()`")
                }
            ).unwrap_unchecked().as_mut()
        }
    }

    /// Inserts an element to the specified index.
    ///
    /// Panics if `index` is greater than the length of the vector or if the vector's capacity is
    /// exceeded.
    #[inline]
    pub fn insert(&mut self, index: usize, value: T) {
        self.insert_mut(index, value);
    }

    /// Inserts an element to the specified index, returning a mutable reference to the newly
    /// written value.
    ///
    /// Panics if `index` is greater than the length of the vector or if the vector's capacity is
    /// exceeded.
    pub fn insert_mut(&mut self, index: usize, value: T) -> &mut T {
        unsafe {
            self.buf.insert(
                value,
                index,
                |_| {
                    panic!("`self.len()` exceeds `self.capacity()`")
                }
            ).unwrap_unchecked().as_mut()
        }
    }

    /// Appends a slice to the end of the vector.
    ///
    /// If the type implements [`Copy`], consider using [`append_copy`][1]
    ///
    /// This may panics if the capacity of the vector is exceeded.
    ///
    /// [1]: Self::append_copy
    #[inline]
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
    /// This may panics if the capacity of the vector is exceeded.
    ///
    /// [1]: Copy
    /// [2]: Self::append
    pub fn append_copy(&mut self, slice: &[T])
        where T: Copy
    {
        let len = self.len().into_usize() + slice.len();
        if len > self.capacity().into_usize() {
            panic!("`self.len()` exceeds `self.capacity()`")
        }
        unsafe {
            slice.as_ptr()
                .copy_to_nonoverlapping(
                    self.as_mut_ptr().add(
                        self.len().into_usize(),
                    ),
                    slice.len()
                );
            self.set_len(SizeType::from_usize(len));
        }
    }

    /// Returns a reference to the last element of the vector, or [`None`] if the vector is empty.
    #[inline]
    pub fn last(&self) -> Option<&T> {
        (!self.is_empty()).then(|| unsafe {
            self.as_ptr()
                .add(self.len().into_usize() - 1)
                .as_ref_unchecked()
        })
    }

    /// Returns a mutable reference to the last element of the vector, or [`None`] if the vector is
    /// empty.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        (!self.is_empty()).then(|| unsafe {
            self.as_mut_ptr()
                .add(self.len().into_usize() - 1)
                .as_mut_unchecked()
        })
    }

    /// Removes an element from the specified index.
    ///
    /// Panics if `index` is greater than or equal to the length of the vector.
    #[inline]
    pub fn remove(&mut self, index: usize) -> T {
        self.buf.remove(index)
    }

    /// Removes an element from the specified index and replaces the value of that index with the
    /// last element of the vector if `index` is not the last index.
    ///
    /// This may be faster than [`remove`][1] but this doesn't preserve the order of elements.
    ///
    /// Panics if `index` is greater than or equal to the length of the vector.
    ///
    /// [1]: Self::remove
    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> T {
        self.buf.swap_remove(index)
    }

    /// Removes the last element of the vector and returns it, or [`None`] if the vector is empty.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.buf.pop()
    }

    /// Removes all elements from the vector.
    #[inline]
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Forgets the vector's data and returns a `(NonNull<u8>, Layout)` tuple containing the old
    /// pointer and a [`layout`][Layout], which may be suitable for deallocating the block of memory
    /// used by the vector.
    ///
    /// Sets the capacity and length to zero and sets the stored pointer's address to a generic
    /// dangling (and aligned) address.
    ///
    /// Note that this does **not** drop any values contained in the vector.
    #[inline]
    pub fn forget(&mut self) -> (NonNull<u8>, Layout) {
        let ptr: NonNull<u8> = self.as_non_null().cast();
        let layout = Layout::array::<T>(
            self.capacity().into_usize(),
        ).expect("layout error");
        self.buf = Buffer::empty();
        (ptr, layout)
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
    #[inline]
    pub fn retain_mut<F>(&mut self, f: F)
        where F: FnMut(&mut T) -> bool
    {
        self.buf.retain_mut(f);
    }

    /// Removes consecutive repeated elements from the vector according to a closure defined
    /// equality.
    #[inline]
    pub fn dedup_by<F>(&mut self, same_bucket: F)
        where F: FnMut(&mut T, &mut T) -> bool
    {
        self.buf.dedup_by(same_bucket);
    }

    /// Removes consecutive repeated elements from the vector according to [`PartialEq`]
    pub fn dedup(&mut self)
        where T: PartialEq
    {
        self.dedup_by(|a, b| a == b);
    }

    /// Removes consecutive repeated elements that resolve to a key implementing [`PartialEq`]. 
    pub fn dedup_by_key<F, K>(&mut self, mut key: F)
        where
            F: FnMut(&mut T) -> K,
            K: PartialEq,
    {
        self.dedup_by(|a, b| key(a) == key(b));
    }
}

/// A reference to a [`MaybeUninit`] slice, which is interpreted as a vector.
///
/// Unlike [`SliceVec<T, usize>`], this stores its capacity and length as [`u32`].
///
/// It has the property: `size_of::<&[T]>() == size_of::<SliceVec<T>>()`
pub type SliceVec32<'a, T, > = SliceVec<'a, T, u32>;

crate::macros::impl_traits! {
    for SliceVec<'a, T, SizeType: [IntoUsize + FromUsize]>
    Drop =>

        #[inline]
        fn drop(&mut self) {
            self.clear();
        }
    ,
    Default =>

        #[inline]
        fn default() -> Self {
            Self {
                buf: Buffer::empty(),
                _marker: PhantomData,
            }
        }
    ,
    Deref =>
        type Target = [T];

        #[inline]
        fn deref(&self) -> &Self::Target {
            self.as_slice()
        }
    ,
    DerefMut =>

        #[inline]
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.as_mut_slice()
        }
    ,
    AsRef<[T]> =>

        fn as_ref(&self) -> &[T] {
            self.as_slice()
        }
    ,
    AsMut<[T]> =>

        fn as_mut(&mut self) -> &mut [T] {
            self.as_mut_slice()
        }
    ,
    IntoIterator for &'vec => 

        type Item = &'vec T;
        type IntoIter = slice::Iter<'vec, T>;

        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    ,
    IntoIterator for mut &'vec => 

        type Item = &'vec mut T;
        type IntoIter = slice::IterMut<'vec, T>;

        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    ,
}

impl<T, SizeType> Extend<T> for SliceVec<'_, T, SizeType>
    where
        SizeType: IntoUsize + FromUsize,
{

    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        for item in iter {
            self.push(item);
        }
    }
}

impl<S, T, SizeType> PartialEq<S> for SliceVec<'_, T, SizeType>
    where
        S: AsRef<[T]>,
        T: PartialEq,
        SizeType: IntoUsize + FromUsize,
{
    #[inline]
    fn eq(&self, other: &S) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T, SizeType> PartialEq<SliceVec<'_, T, SizeType>> for [T]
    where
        T: PartialEq,
        SizeType: IntoUsize + FromUsize,
{

    #[inline]
    fn eq(&self, other: &SliceVec<'_, T, SizeType>) -> bool {
        self == other.as_ref()
    }
}

impl<T, SizeType> PartialEq<SliceVec<'_, T, SizeType>> for &[T]
    where
        T: PartialEq,
        SizeType: IntoUsize + FromUsize,
{

    #[inline]
    fn eq(&self, other: &SliceVec<T, SizeType>) -> bool {
        *self == other.as_ref()
    }
}

unsafe impl<T, SizeType> Send for SliceVec<'static, T, SizeType> 
    where
        SizeType: FromUsize + IntoUsize,
        T: Send,
{}

unsafe impl<T, SizeType> Sync for SliceVec<'static, T, SizeType>
    where
        SizeType: FromUsize + IntoUsize,
        T: Sync,
{}

impl<'a, T, SizeType> Display for SliceVec<'a, T, SizeType>
    where
        SizeType: IntoUsize + FromUsize,
        T: Display,
{

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.len() == SizeType::ZERO {
            <str as Display>::fmt("[]", f)?;
            return Ok(())
        }
        <char as Display>::fmt(&'[', f)?;
        for value in &self[0..self.len().into_usize() - 1] {
            value.fmt(f)?;
            <str as Display>::fmt(", ", f)?;
        }
        self.last().unwrap().fmt(f)?;
        <char as Display>::fmt(&']', f)
    }
}

impl<'a, T, SizeType> Debug for SliceVec<'a, T, SizeType>
    where 
        SizeType: IntoUsize + FromUsize,
        T: Debug
{
    
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}
