//! Provides [`AllocVec`]: a new vector type which uses [`LocalAlloc`] for its allocations.

mod buffer;

use core::{
    alloc::Layout,
    borrow::{Borrow, BorrowMut},
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice::{self, Iter, IterMut}
};

use crate::{
    reserve::*,
    int::{FromUsize, IntoUsize},
    alloc::LocalAlloc,
    iter::FromIteratorIn,
};

pub(crate) use buffer::Buffer;

/// [`ReservePolicy`] used by [`DynVec`].
pub struct DynPolicy;

unsafe impl ReservePolicy for DynPolicy {

    #[inline]
    fn grow_factor() -> usize {
        2
    }

    #[inline]
    fn grow(current: usize, required: usize) -> Result<usize, ReserveError<()>> {
        let power_of_2 = required.next_power_of_two().max(2);
        if power_of_2 < current { Ok(current) }
        else { Ok(power_of_2) }
    }

    #[inline]
    fn grow_infallible(current: usize, required: usize) -> usize {
        let power_of_2 = required.next_power_of_two().max(2);
        if power_of_2 < current { current }
        else { power_of_2 }
    }
}

/// [`ReservePolicy`] used by [`FixedVec`].
pub struct FixedPolicy;

unsafe impl ReservePolicy for FixedPolicy {

    #[inline]
    fn grow_factor() -> usize {
        0
    }

    #[inline]
    fn grow(current: usize, requested: usize) -> Result<usize, ReserveError<()>> {
        if requested <= current {
            Ok(current)
        } else {
            Err(ReserveError::max_capacity_exceeded(current, requested, ()))
        }
    }

    #[inline]
    fn grow_infallible(current: usize, required: usize) -> usize {
        if required > current.into_usize() { panic!("maximum capacity of {current} exceeded") }
        current
    }
}

/// [`ReservePolicy`] used by [`DynVec32`].
pub struct DynPolicy32;

/// [`ReservePolicy`] used by [`FixedVec32`].
pub struct FixedPolicy32;

unsafe impl ReservePolicy<u32> for DynPolicy32 {

    #[inline]
    fn grow_factor() -> u32 {
        2
    }

    #[inline]
    fn grow(current: u32, required: usize) -> Result<u32, ReserveError<()>> {
        let power_of_2 = required.next_power_of_two().max(2);
        if power_of_2 > u32::MAX as usize {
            Err(ReserveError::max_capacity_exceeded(u32::MAX as usize, power_of_2, ()))
        } else if power_of_2 <= current as usize {
            Ok(current)
        } else { Ok(power_of_2.max(2) as u32) }
    }

    #[inline]
    fn grow_infallible(current: u32, required: usize) -> u32 {
        let power_of_2 = required.next_power_of_two().max(2);
        if power_of_2 > u32::MAX as usize || power_of_2 <= current as usize {
            current
        } else {
            power_of_2.max(2) as u32
        }
    }
}

unsafe impl ReservePolicy<u32> for FixedPolicy32 {

    fn grow_factor() -> u32 {
        0
    }

    #[inline]
    fn grow(current: u32, requested: usize) -> Result<u32, ReserveError<()>> {
        if requested <= current as usize {
            Ok(current)
        } else {
            Err(ReserveError::max_capacity_exceeded(current, requested, ()))
        }
    }

    #[inline]
    fn grow_infallible(current: u32, required: usize) -> u32 {
        if required > current.into_usize() { panic!("maximum capacity of {current} exceeded") }
        current
    }

    fn min_non_zero_capacity(size: usize) -> u32 {
        if size == 1 {
            8
        } else if size <= 1024 {
            4
        } else {
            1
        }
    }
}

/// A vector type, which uses a [`local, owned allocator`][1] for its allocations.
///
/// It is recommended to use one of its derivatives ([`DynVec`], [`FixedVec`], [`DynVec32`] and
/// [`FixedVec32`]) when using it with a local allocator.
///
/// # Examples
/// ```
/// # #[cfg(feature = "std")]
/// # {
/// use leimu_mem::arena::Arena;
/// use leimu_mem::alloc_vec::FixedVec;
///
/// let arena = Arena::new(64).unwrap();
/// let mut vec = FixedVec::with_capacity_in(5, &arena).unwrap();
/// vec.push(1);
/// vec.append(&[2, 3]);
/// vec.extend(4..6);
/// assert_eq!(vec, [1, 2, 3, 4, 5]);
/// # }
/// ```
///
/// [1]: LocalAlloc
pub struct AllocVec<T, A, ReservePol, SizeType = usize>
    where
        T: Sized,
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{
    buf: Buffer<T, SizeType>,
    allocator: A,
    _markers: PhantomData<ReservePol>,
}

/// A vector using [`LocalAlloc`], which can reallocate.
///
/// See [`AllocVec`] for full description and examples.
pub type DynVec<T, A> = AllocVec<T, A, DynPolicy, usize>;
/// A vector using [`LocalAlloc`], which can reallocate.
///
/// Stores its capacity and length as [`u32`].
///
/// See [`AllocVec`] for full description and examples.
pub type DynVec32<T, A> = AllocVec<T, A, DynPolicy32, u32>;

/// A vector using [`LocalAlloc`], which won't reallocate.
///
/// See [`AllocVec`] for full description and examples.
pub type FixedVec<T, A> = AllocVec<T, A, FixedPolicy, usize>;
/// A vector using [`LocalAlloc`], which won't reallocate.
///
/// Stores its capacity and length as [`u32`].
///
/// See [`AllocVec`] for full description and examples.
pub type FixedVec32<T, A> = AllocVec<T, A, FixedPolicy32, u32>;

impl<T, A, ReservePol, SizeType> AllocVec<T, A, ReservePol, SizeType>
    where
        T: Sized,
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize
{
    /// Creates a new empty vector with an allocator.
    pub const fn new_in(allocator: A) -> Self {
        Self {
            buf: Buffer::empty(),
            allocator,
            _markers: PhantomData,
        }
    }

    /// Creates a new empty vector with an allocator and the specified capacity.
    ///
    /// The vector can grow up to at least `capacity` number of elements before needing reallocation.
    ///
    /// This may reserve more space than `capacity` depending on the [`reservation policy`][1].
    ///
    /// [1]: ReservePolicy
    pub fn with_capacity_in(
        capacity: SizeType,
        allocator: A,
    ) -> Result<Self, ReserveError<()>> {
        if capacity == SizeType::ZERO {
            return Ok(Self::new_in(allocator))
        }
        let capacity = ReservePol::grow_infallible(capacity, capacity.into_usize());
        let layout = Layout::array::<T>(capacity.into_usize())?;
        let buf = Buffer::new(
            allocator
                .allocate(layout)?,
        );
        Ok(Self {
            buf,
            allocator,
            _markers: PhantomData,
        })
    }

    /// Creates a vector with `len` number of elements and an allocator by calling `f` `len` times.
    ///
    /// This may reserve more space than `len` depending on the [`reservation policy`][1].
    ///
    /// [1]: ReservePolicy
    pub fn with_len_with_in<F>(
        len: SizeType,
        mut f: F,
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where F: FnMut(SizeType) -> T
    {
        if len == SizeType::ZERO {
            return Ok(Self::new_in(allocator))
        }
        let capacity = ReservePol::grow_infallible(len, len.into_usize());
        let layout = Layout::array::<T>(capacity.into_usize())?;
        let mut buf = Buffer::<T, SizeType>::new(
            allocator
                .allocate(layout)?,
        );
        let ptr = buf.as_mut_ptr();
        for i in 0..len.into_usize() {
            unsafe { ptr.add(i).write(f(SizeType::from_usize(i))) };
        }
        unsafe {
            buf.set_len(len);
        }
        Ok(Self {
            buf,
            allocator,
            _markers: PhantomData,
        })
    }

    /// Creates a vector with `len` number of elements and an allocator by [`cloning`][Clone]
    /// `value` `len` times.
    ///
    /// This may reserve more space than `len` depending on the [`reservation policy`][1].
    ///
    /// [1]: ReservePolicy
    pub fn with_len_in(
        len: SizeType,
        value: T,
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where
            T: Clone
    {
        Self::with_len_with_in(len, |_| value.clone(), allocator)
    }

    /// Creates a vector from a slice over `T`. and an allocator by [`cloning`][Clone] each element
    /// in the slice.
    ///
    /// Requires that `T` implements [`Clone`].
    ///
    /// This may reserve more space than the length of the slice depending on the
    /// [`reservation policy`][1].
    ///
    /// [1]: ReservePolicy
    #[inline]
    pub fn from_slice_in(
        slice: &[T],
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where T: Clone
    {
        let mut vec = Self::with_capacity_in(
            SizeType::from_usize(slice.len()),
            allocator
        )?;
        vec.append(slice);
        Ok(vec)
    }

    /// Creates a vector from a slice over `T`. and an allocator by [`copying`][Copy] each element
    /// in the slice.
    ///
    /// Requires that `T` implements [`Copy`].
    ///
    /// This may reserve more space than the length of the slice depending on the
    /// [`reservation policy`][1].
    ///
    /// [1]: ReservePolicy
    #[inline]
    pub fn from_slice_copy_in(
        slice: &[T],
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where T: Copy
    {
        let mut vec = Self::with_capacity_in(
            SizeType::from_usize(slice.len()),
            allocator
        )?;
        vec.append_copy(slice);
        Ok(vec)
    }

    /// Creates a vector from an iterator over `T` and an allocator.
    ///
    /// This tries to first reserve space for the iterator given its [`size hint`][1].
    ///
    /// If this the size of the iterator is unknown, this may return an error if the vector has a
    /// fixed capacity.
    ///
    /// [1]: Iterator::size_hint
    pub fn from_iter_in<I>(
        iter: I,
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where I: IntoIterator<Item = T>
    {
        let iter = iter.into_iter();
        let size_hint = iter.size_hint();
        let mut vec = Self::with_capacity_in(
            SizeType::from_usize(size_hint.1.unwrap_or(size_hint.0)),
            allocator
        )?;
        vec.extend(iter);
        Ok(vec)
    }

    /// Flattens a slice over slices to a vector by [`cloning`][Clone] each element in each slice.
    ///
    /// Requires that `T` implements [`Clone`].
    pub fn flattened_in<U>(
        slices: &[U],
        allocator: A,
    ) -> Result<Self, ReserveError<()>>
        where
            U: AsRef<[T]>,
            T: Clone,
    {
        let capacity: usize = slices
            .iter()
            .map(|s| s.as_ref().len())
            .sum();
        let mut vec = Self::with_capacity_in(
            SizeType::from_usize(capacity), allocator
        )?;
        vec.extend(slices
            .iter()
            .flat_map(|s| s
                .as_ref()
                .iter()
                .cloned()
            )
        );
        Ok(vec)
    } 
}

impl<T, A, ReservePol, SizeType> AllocVec<T, A, ReservePol, SizeType>
    where
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{

    /// Creates a new [`AllocVec`] directly from a pointer, a length, a capacity and an allocator.
    ///
    /// # Safety
    /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
    /// aren't checked:
    /// - `ptr` must be currently allocated via the given `alloc`.
    /// - `T` needs to have the same alignment as what `ptr` was allocated with.
    /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
    ///   with.
    /// - `len` must be less than or equal to `capacity`.
    /// - The first `len` values must be properly initialized values of type `T`.
    /// - `capacity` needs to fit the layout size that the pointer was allocated with.
    /// - The allocated size in bytes must be no larger than `isize::MAX`.
    #[inline]
    pub const unsafe fn from_raw_parts_in(
        ptr: *mut T,
        len: SizeType,
        capacity: SizeType,
        alloc: A,
    ) -> Self {
        unsafe {
            Self {
                buf: Buffer::from_raw_parts(
                    ptr,
                    len,
                    capacity
                ),
                allocator: alloc,
                _markers: PhantomData,
            }
        } 
    }

    /// Creates a new [`AllocVec`] directly from a [`NonNull`] pointer, a length, a capacity and an allocator.
    ///
    /// # Safety
    /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
    /// aren't checked:
    /// - `ptr` must be currently allocated via the given `alloc`.
    /// - `T` needs to have the same alignment as what `ptr` was allocated with.
    /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
    ///   with.
    /// - `len` must be less than or equal to `capacity`.
    /// - The first `len` values must be properly initialized values of type `T`.
    /// - `capacity` needs to fit the layout size that the pointer was allocated with.
    /// - The allocated size in bytes must be no larger than `isize::MAX`.
    #[inline]
    pub const unsafe fn from_parts_in(
        ptr: NonNull<T>,
        len: SizeType,
        capacity: SizeType,
        alloc: A,
    ) -> Self {
        unsafe {
            Self {
                buf: Buffer::from_parts(ptr, len, capacity),
                allocator: alloc,
                _markers: PhantomData,
            }
        } 
    }

    /// Decomposes a [`AllocVec`] into its raw components: (pointer, a length, capacity, allocator).
    #[inline]
    pub const fn into_raw_parts_with_alloc(
        self,
    ) -> (*mut T, SizeType, SizeType, A) { 
        let mut vec = ManuallyDrop::new(self);
        let vec = unsafe { ::core::mem::transmute::<
            &mut ManuallyDrop<Self>,
            &mut Self
        >(&mut vec) };
        (
            vec.as_mut_ptr(),
            vec.len(),
            vec.capacity(),
            unsafe {::core::ptr::read(&vec.allocator) },
        )
    }

    /// Decomposes a [`AllocVec`] into its raw components: (pointer, a length, capacity).
    #[inline]
    pub const fn into_raw_parts(
        self,
    ) -> (*mut T, SizeType, SizeType) { 
        let mut vec = ManuallyDrop::new(self);
        let vec = unsafe { ::core::mem::transmute::<
            &mut ManuallyDrop<Self>,
            &mut Self
        >(&mut vec) };
        (
            vec.as_mut_ptr(),
            vec.len(),
            vec.capacity(),
        )
    }

    /// Decomposes a [`AllocVec`] into its raw components: `(NonNull pointer, a length, capacity, allocator)`.
    #[inline]
    pub const fn into_parts_with_alloc(
        self,
    ) -> (NonNull<T>, SizeType, SizeType, A) { 
        let mut vec = ManuallyDrop::new(self);
        let vec = unsafe { ::core::mem::transmute::<
            &mut ManuallyDrop<Self>,
            &mut Self
        >(&mut vec) };
        (
            unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) },
            vec.len(),
            vec.capacity(),
            unsafe {::core::ptr::read(&vec.allocator) },
        )
    }

    /// Decomposes a [`AllocVec`] into its raw components: (pointer, a length, capacity).
    #[inline]
    pub const fn into_parts(
        self,
    ) -> (NonNull<T>, SizeType, SizeType) { 
        let mut vec = ManuallyDrop::new(self);
        let vec = unsafe { ::core::mem::transmute::<
            &mut ManuallyDrop<Self>,
            &mut Self
        >(&mut vec) };
        (
            vec.as_non_null(),
            vec.len(),
            vec.capacity(),
        )
    }
}

/// An iterator that moves out of the vector.
pub struct IntoIter<T, SizeType: IntoUsize + FromUsize, A: LocalAlloc> {
    ptr: NonNull<T>,
    capacity: SizeType,
    start: SizeType,
    end: SizeType,
    allocator: A,
}

unsafe impl<T, SizeType, A> Send for IntoIter<T, SizeType, A> 
    where
        T: Send,
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{}

unsafe impl<T, SizeType, A> Sync for IntoIter<T, SizeType, A> 
    where
        T: Sync,
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{}

impl<T, SizeType, A> Iterator for IntoIter<T, SizeType, A>
    where 
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{

    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start != self.end {
            let t = unsafe {
                self.ptr.add(self.start.into_usize()).read()
            };
            self.start += SizeType::ONE;
            Some(t)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = (self.end - self.start).into_usize();
        (size, Some(size))
    }
}

impl<T, SizeType, A> DoubleEndedIterator for IntoIter<T, SizeType, A>
    where
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{

    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start != self.end {
            self.end -= SizeType::ONE;
            let t = unsafe {
                self.ptr.add(self.end.into_usize()).read()
            };
            Some(t)
        } else {
            None
        }
    }
}

impl<T, SizeType, A> ExactSizeIterator for IntoIter<T, SizeType, A>
    where
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{}

impl<T, SizeType, A> Drop for IntoIter<T, SizeType, A>
    where
        SizeType: IntoUsize + FromUsize,
        A: LocalAlloc,
{

    fn drop(&mut self) {
        unsafe {
            if self.capacity == SizeType::ZERO { return }
            for i in self.start.into_usize()..self.end.into_usize() {
                self.ptr
                    .add(i)
                    .drop_in_place();
            }
            self.allocator
                .deallocate(
                    self.ptr.cast(),
                    Layout::array::<T>(self.capacity.into_usize()).unwrap()
                );
        }
    }
}

impl<T, A, ReservePol, SizeType> AllocVec<T, A, ReservePol, SizeType>
    where
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{

    /// Returns a reference to the underlying [`allocator`][LocalAlloc].
    #[inline]
    pub fn allocator(&self) -> &A {
        &self.allocator
    }

    /// Returns the length of the vector.
    #[inline]
    pub const fn len(&self) -> SizeType {
        self.buf.len()
    }

    /// Returns the allocated capacity of the vector.
    #[inline]
    pub const fn capacity(&self) -> SizeType {
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

    /// Gets a pointer to the start of the array.
    #[inline]
    pub const fn as_ptr(&self) -> *const T {
        self.buf.as_ptr()
    }

    /// Gets a mutable pointer to the start of the array.
    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr()
    }
   
    /// Gets a [`non-null`][1] pointer to the start of the array.
    ///
    /// The vector's inner pointer is guaranteed to be non-bull.
    ///
    /// [1]: NonNull
    #[inline]
    pub const fn as_non_null(&mut self) -> NonNull<T> {
        self.buf.as_non_null()
    }

    /// Returns a slice over the contents of the vector.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len().into_usize()) }
    }

    /// Returns a mutable slice over the contents of the vector.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len().into_usize()) }
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
    /// indices less than the current length are not dropped when the vector is
    /// [`cleared`][1] or [`dropped`][Drop].
    ///
    /// [1]: Self::clear
    #[inline]
    pub unsafe fn set_len(&mut self, new_len: SizeType) {
        debug_assert!(
            new_len <= self.capacity(),
            "`new_len` {new_len} is larger than capacity {}",
            self.capacity()
        );
        unsafe {
            self.buf.set_len(new_len);
        }
    }

    /// Shrinks the capacity of the vector as much as possible. 
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        if self.capacity() == self.len() { return }
        let old_ptr = self.as_non_null();
        let new_layout = Layout::array::<T>(self.len().into_usize())
            .expect("layout error");
        let mut new_buf = unsafe {
            Buffer::new(
                self.allocator
                    .shrink(
                        old_ptr.cast(),
                        Layout::array::<T>(self.capacity().into_usize())
                            .expect("layout error"),
                        new_layout,
                    ).expect("alloc error"),
            )
        };
        unsafe {
            new_buf.set_len(self.len());
        }
        self.buf = new_buf;
    }

    /// Reserves space for the vector.
    ///
    /// This may speculatively reserve more space than `new_capacity` to avoid frequent reallocations.
    ///
    /// Use [`reserve_exact`][1] to allocate an exact capacity.
    ///
    /// This may panic if the vector has a fixed capacity or if an allocation fails.
    ///
    /// Use [`try_reserve`][2] if allocation failure can be handled.
    ///
    /// [1]: Self::reserve_exact
    /// [2]: Self::try_reserve
    pub fn reserve(&mut self, new_capacity: SizeType)
    {
        let capacity = ReservePol::grow_infallible(self.capacity(), new_capacity.into_usize());
        self.reserve_exact(capacity);
    }

    /// Reserves space for the vector exactly up to `new_capacity`.
    ///
    /// This may panic if the vector has a fixed capacity or if an allocation fails.
    ///
    /// Use [`try_reserve_exact`][2] if allocation failure can be handled.
    ///
    /// [1]: Self::reserve_exact
    /// [2]: Self::try_reserve
    pub fn reserve_exact(&mut self, new_capacity: SizeType) {
        self.try_reserve_exact(new_capacity)
            .expect("failed to reserve `new_capacity`")
    }
    
    /// Tries to reserve space for the vector, returning an error if the vector has a fixed capacity
    /// or if an allocation fails.
    ///
    /// This may speculatively reserve more space than `new_capacity` to avoid frequent reallocations.
    ///
    /// Use [`try_reserve_exact`][1] to allocate an exact capacity.
    ///
    /// [1]: Self::try_reserve_exact
    pub fn try_reserve(&mut self, new_capacity: SizeType) -> Result<(), ReserveError<()>> {
        let capacity = ReservePol::grow(
            self.capacity(), new_capacity.into_usize()
        )?;
        self.try_reserve_exact(capacity)
    }

    /// Tries to reserve space for the vector exactly up to `new_capacity`, returning an error if the
    /// vector has a fixed capacity or if an allocation fails.
    pub fn try_reserve_exact(&mut self, new_capacity: SizeType) -> Result<(), ReserveError<()>> {
        Self::_try_reserve_exact(
            &mut self.buf,
            new_capacity,
            &self.allocator
        )
    }

    fn _try_reserve_exact(
        buf: &mut Buffer<T, SizeType>,
        new_capacity: SizeType,
        allocator: &A,
    ) -> Result<(), ReserveError<()>> {
        let capacity = buf.capacity();
        if new_capacity <= capacity { return Ok(()) }
        if capacity != SizeType::ZERO && !ReservePol::can_grow() {
            return Err(ReserveError::max_capacity_exceeded(
                capacity, new_capacity.into_usize(), (),
            ))
        }
        debug_assert!(buf.len() <= buf.capacity());
        let (ptr, layout) = unsafe { buf.reallocate(allocator
            .allocate(Layout::array::<T>(new_capacity.into_usize())?)?
        ) };
        if layout.size() != 0 {
            unsafe { allocator.deallocate(ptr, layout); }
        }
        Ok(())
    }

    /// Resizes the vector with a clonable value.
    ///
    /// If `new_len` is less than the current length, this shrinks the vector.
    ///
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
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
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
    pub fn resize_with<F>(&mut self, new_len: SizeType, mut f: F)
        where
            F: FnMut() -> T
    {
        if new_len > self.capacity() {
            self.reserve(new_len);
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
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
    pub fn try_resize_with<F, E>(
        &mut self,
        new_len: SizeType,
        mut f: F,
    ) -> Result<(), E>
        where
            F: FnMut() -> Result<T, E>,
    {
        if new_len > self.capacity() {
            self.reserve(new_len)
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
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
    #[inline]
    pub fn push(&mut self, value: T) {
        self.push_mut(value);
    }

    /// Appends an element to the end of the vector, returning a mutable reference to the newly
    /// written value.
    ///
    /// This may panic if [`reserve'][1] fails.
    ///
    /// [1]: Self::reserve
    pub fn push_mut(&mut self, value: T) -> &mut T {
        unsafe {
            self.buf.insert(
                value,
                self.len().into_usize(),
                |buf| {
                    let new_capacity = ReservePol
                        ::grow_one(
                            buf.capacity(),
                            Layout::new::<T>()
                        ).unwrap();
                    Self::_try_reserve_exact(
                        buf,
                        new_capacity,
                        &self.allocator
                    )
                }
            ).expect("failed to reserve one")
            .as_mut()
        }
    }

    /// Inserts an element to the specified index.
    ///
    /// Panics if `index` is greater than the length of the vector.
    ///
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
    #[inline]
    pub fn insert(&mut self, index: usize, value: T) {
        self.insert_mut(index, value);
    }


    /// Inserts an element to the specified index, returning a mutable reference to the newly
    /// written value.
    ///
    /// Panics if `index` is greater than the length of the vector.
    ///
    /// This may panic if [`reserve`][1] fails.
    ///
    /// [1]: Self::reserve
    pub fn insert_mut(&mut self, index: usize, value: T) -> &mut T {
        unsafe {
            self.buf.insert(
                value,
                index,
                |buf| {
                    let new_capacity = ReservePol
                        ::grow_one(
                            buf.capacity(),
                            Layout::new::<T>()
                        ).unwrap();
                    Self::_try_reserve_exact(
                        buf,
                        new_capacity,
                        &self.allocator
                    )
                }
            ).expect("failed to reserve one")
            .as_mut()
        }
    }

    /// Appends a slice to the end of the vector.
    ///
    /// If the type implements [`Copy`], consider using [`append_copy`][1]
    ///
    /// This may panic if [`reserve`][2] fails.
    ///
    /// [1]: Self::append_copy
    /// [2]: Self::reserve
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
    /// This may panic if [`reserve`][3] fails.
    ///
    /// [1]: Copy
    /// [2]: Self::append
    /// [3]: Self::reserve
    pub fn append_copy(&mut self, slice: &[T])
        where T: Copy
    {
        let len = self.len().into_usize() + slice.len();
        let capacity = ReservePol
            ::grow(self.capacity(), len)
            .unwrap();
        self.reserve_exact(capacity);
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

    /// Removes all elements from the vector preserving its allocated capacity.
    #[inline]
    pub fn clear(&mut self) {
        self.buf.clear();
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

crate::macros::impl_traits!{
    for AllocVec<
        T: [Sized],
        A: [LocalAlloc],
        ReservePol: [ReservePolicy<SizeType>],
        SizeType: [IntoUsize + FromUsize]
    >
    Drop =>

        fn drop(&mut self) {
            if let Some((ptr, layout)) = self.buf.deallocate() {
                unsafe {
                    self.allocator
                        .deallocate(ptr, layout);
                }
            }
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
    IntoIterator for &'vec =>

        type Item = &'vec T;
        type IntoIter = Iter<'vec, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    ,
    IntoIterator for mut &'vec =>

        type Item = &'vec mut T;
        type IntoIter = IterMut<'vec, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    ,
    IntoIterator => 

        type Item = T;
        type IntoIter = IntoIter<T, SizeType, A>;

        fn into_iter(self) -> Self::IntoIter {
            unsafe {
                let mut vec = ManuallyDrop::new(self);
                IntoIter {
                    ptr: vec.as_non_null(),
                    capacity: vec.capacity(),
                    start: SizeType::ZERO,
                    end: vec.len(),
                    allocator: (::core::ptr::read(&vec.allocator)),
                }
            }
        }
    ,
    Eq where T: Eq =>
    ,
    Hash where T: Hash =>

        fn hash<H: Hasher>(&self, state: &mut H) {
            self.as_slice().hash(state)
        }
    ,
    Debug where T: Debug =>

        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_slice().fmt(f)
        }
    ,
    Display where T: Display =>

        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    ,
}

impl<T, A, ReservePol, SizeType> Extend<T>
    for AllocVec<T, A, ReservePol, SizeType>
    where
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{

    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        if let (_, Some(len)) = iter.size_hint() {
            self.reserve_exact(ReservePol::grow_infallible(
                self.capacity(),
                self.len().into_usize() + len
            ));
        }
        for item in iter {
            self.push(item);
        }
    }
}

impl<S, T, A, ReservePol, SizeType> PartialEq<S> 
    for AllocVec<T, A, ReservePol, SizeType>
    where
        S: AsRef<[T]>,
        T: PartialEq,
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{
    #[inline]
    fn eq(&self, other: &S) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T, A, ReservePol, SizeType> PartialEq<
        AllocVec<T, A, ReservePol, SizeType>
    > for [T]
    where
        T: PartialEq,
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{
    #[inline]
    fn eq(&self, other: &AllocVec<T, A, ReservePol, SizeType>) -> bool {
        self == other.as_ref()
    }
}

impl<T, A, ReservePol, SizeType> PartialEq<
        AllocVec<T, A, ReservePol, SizeType>
    > for &[T]
    where
        T: PartialEq,
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{

    #[inline]
    fn eq(&self, other: &AllocVec<T, A, ReservePol, SizeType>) -> bool {
        *self == other.as_ref()
    }
}

impl<T, A, ReservePol, SizeType> FromIteratorIn<T>
    for AllocVec<T, A, ReservePol, SizeType>
    where
        A: LocalAlloc,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
{
    type Alloc = A;

    #[inline]
    fn from_iter_in<I>(
        iter: I,
        allocator: Self::Alloc,
    ) -> Result<Self, ReserveError<()>>
        where
            I: IntoIterator<Item = T>,
            I::IntoIter: ExactSizeIterator
    {
        let iter = iter.into_iter();
        let mut vec = Self::with_capacity_in(
            SizeType::from_usize(iter.len()),
            allocator,
        )?;
        vec.extend(iter);
        Ok(vec)
    }
}

unsafe impl<
    T: Send,
    A: LocalAlloc + Send,
    ReservePol: ReservePolicy<SizeType>,
    SizeType: IntoUsize + FromUsize,
> Send for AllocVec<T, A, ReservePol, SizeType> {}

unsafe impl<
    T: Sync,
    A: LocalAlloc + Sync,
    ReservePol: ReservePolicy<SizeType>,
    SizeType: IntoUsize + FromUsize,
> Sync for AllocVec<T, A, ReservePol, SizeType> {}

#[cfg(feature = "std")]
mod std_features {

    use super::*;

    /// A vector that stores its capacity and length as [`u32`] instead of [`usize`] resulting in the
    /// struct taking only 16 bytes on the stack on 64-bit systems.
    ///
    /// On 64-bit systems the size of [`Vec32<T>`] is equal to the size of [`Box<[T]>`][2].
    ///
    /// This was mainly made for Vulkan usage, since Vulkan often uses u32 for counts.
    ///
    /// Maximum capacity is restricted to be equal to [`u32::MAX`].
    ///
    /// [1]: std::alloc::alloc
    /// [2]: std::boxed::Box
    pub type Vec32<T> = AllocVec<T, (), DynPolicy32, u32>;

    impl<
        T,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
    > Default for AllocVec<T, (), ReservePol, SizeType> {

        #[inline]
        fn default() -> Self {
            AllocVec::new()
        }
    }

    impl<
        T,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
    > Clone for AllocVec<T, (), ReservePol, SizeType>
        where T: Clone
    {

        #[inline]
        fn clone(&self) -> Self {
            self.iter()
                .cloned()
                .collect()
        }
    }

    impl<
        T,
        ReservePol: ReservePolicy<SizeType>,
        SizeType: IntoUsize + FromUsize,
    > From<&[T]> for AllocVec<T, (), ReservePol, SizeType>
        where T: Clone
    {

        #[inline]
        fn from(value: &[T]) -> Self {
            value.iter()
                .cloned()
                .collect()
        }
    }

    impl<const N: usize, T, SizeType, ReservePol> From<[T; N]>
        for AllocVec<T, (), ReservePol, SizeType>
        where
            SizeType: IntoUsize + FromUsize,
            ReservePol: ReservePolicy<SizeType>,
    {
        
        fn from(value: [T; N]) -> Self {
            let mut vec = Self::with_capacity(SizeType::from_usize(N));
            for v in value {
                vec.push(v);
            }
            vec
        }
    }

    impl<A, ReservePol, SizeType> FromIterator<A> for AllocVec<A, (), ReservePol, SizeType>
        where 
            SizeType: IntoUsize + FromUsize,
            ReservePol: ReservePolicy<SizeType>,
    {

        fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
            let mut vec = Self::new();
            vec.extend(iter);
            vec
        }
    }

    impl<T, ReservePol, SizeType> AllocVec<T, (), ReservePol, SizeType>
        where 
            SizeType: IntoUsize + FromUsize,
            ReservePol: ReservePolicy<SizeType>,
    {

        /// Creates an empty vector with zero capacity.
        #[inline]
        pub const fn new() -> Self {
            Self {
                buf: Buffer::empty(),
                allocator: (),
                _markers: PhantomData,
            }
        }

        /// Creates an empty vector with at least the given capacity.
        ///
        /// The capacity of the vector might be more than what is given. If you want a vector with an
        /// exacit capacity, use the `with_exact_capacity` instead.
        pub fn with_capacity(
            capacity: SizeType,
        ) -> Self {
            if capacity == SizeType::ZERO {
                return Default::default()
            }
            let capacity = ReservePol::grow_infallible(capacity, capacity.into_usize());
            let layout = Layout
                ::array::<T>(capacity.into_usize())
                .expect("layout error");
            let buf = Buffer::new(() 
                .allocate(
                    layout
                ).expect("global alloc failed"),
            );
            Self {
                buf,
                allocator: (),
                _markers: PhantomData,
            }
        }

        /// Creates an empty vector with at the exact capacity specified.
        pub fn with_exact_capacity(
            capacity: SizeType,
        ) -> Self {
            if capacity == SizeType::ZERO {
                return Self::new()
            }
            let layout = Layout
                ::array::<T>(capacity.into_usize())
                .expect("layout error");
            let buf = Buffer::new(()
                .allocate(
                    layout
                ).expect("global alloc failed"),
            );
            Self {
                buf,
                allocator: (),
                _markers: PhantomData,
            }
        }

        /// Creates a vector with `len` number of elements by calling `f` `len` times.
        pub fn with_len_with<F>(
            len: SizeType,
            mut f: F,
        ) -> Self
            where
                F: FnMut(usize) -> T,
        {
            if len == SizeType::ZERO {
                return Self::new();
            }
            let mut vec = Self::with_capacity(len);
            let mut i = 0;
            vec.resize_with(len, || f({ let idx = i; i += 1; idx }));
            vec
        }

        /// Creates a vector with `len` number of elements by [`cloning`][Clone] `value` `len`
        /// times.
        ///
        /// Requires that `T` implements [`Clone`].
        pub fn with_len(
            len: SizeType,
            value: T,
        ) -> Self
            where
                T: Clone
        {
            Self::with_len_with(len, |_| value.clone())
        }

        /// Creates a vector with `len` number of elements by [`cloning`][Clone] `value` `len`
        /// times.
        ///
        /// Reserves exactly `len` amount of space.
        ///
        /// Requires that `T` implements [`Clone`].
        pub fn with_len_reserve_exact(
            len: SizeType,
            value: T,
        ) -> Self
            where
                T: Clone
        {
            if len == SizeType::ZERO {
                return Self::new();
            }
            let mut vec = Self::with_exact_capacity(len);
            vec.resize(len, value);
            vec
        }

        /// Flattens a slice over slices to a vector by [`cloning`][Clone] each element in each slice.
        ///
        /// Requires that `T` implements [`Clone`].
        pub fn flattened<U>(
            slices: &[U],
        ) -> Self
            where
                U: AsRef<[T]>,
                T: Clone,
        {
            slices
                .iter()
                .flat_map(|s| s
                    .as_ref()
                    .iter()
                    .cloned()
                ).collect()
        }

        /// Creates a new [`AllocVec`] directly from a pointer, a length a capacity.
        ///
        /// # Safety
        /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
        /// aren't checked:
        /// - `ptr` must be currently allocated via the global allocator.
        /// - `T` needs to have the same alignment as what `ptr` was allocated with.
        /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
        ///   with.
        /// - `len` must be less than or equal to `capacity`.
        /// - The first `len` values must be properly initialized values of type `T`.
        /// - `capacity` needs to fit the layout size that the pointer was allocated with.
        /// - The allocated size in bytes must be no larger than `isize::MAX`.
        #[inline]
        pub const unsafe fn from_raw_parts(
            ptr: *mut T,
            len: SizeType,
            capacity: SizeType,
        ) -> Self {
            unsafe {
                Self {
                    buf: Buffer::from_raw_parts(ptr, len, capacity),
                    allocator: (),
                    _markers: PhantomData,
                }
            } 
        }

        /// Creates a new [`AllocVec`] directly from a [`NonNull`] pointer, a length a capacity.
        ///
        /// # Safety
        /// There are a multitude of reasons why this is unsafe, due to the number of invariants that
        /// aren't checked:
        /// - `ptr` must be currently allocated via the global allocator.
        /// - `T` needs to have the same alignment as what `ptr` was allocated with.
        /// - The size of `T * capacity` needs to be the same size as what the pointer was allocated
        ///   with.
        /// - `len` must be less than or equal to `capacity`.
        /// - The first `len` values must be properly initialized values of type `T`.
        /// - `capacity` needs to fit the layout size that the pointer was allocated with.
        /// - The allocated size in bytes must be no larger than `isize::MAX`.
        #[inline]
        pub const unsafe fn from_parts(
            ptr: NonNull<T>,
            len: SizeType,
            capacity: SizeType,
        ) -> Self {
            unsafe {
                Self {
                    buf: Buffer::from_parts(ptr, len, capacity),
                    allocator: (),
                    _markers: PhantomData,
                }
            } 
        }

        /// Converts the vector into [`Box<[T]>`][1].
        ///
        /// Before doing so, this removes any excess capacity via [`shrink_to_fit`][2].
        ///
        /// [1]: std::boxed::Box
        /// [2]: Self::shrink_to_fit
        #[inline]
        pub fn into_boxed_slice(
            mut self,
        ) -> std::boxed::Box<[T]> {
            unsafe {
                self.shrink_to_fit();
                let mut s = ManuallyDrop::new(self);
                std::boxed::Box::from_raw(s.as_mut_slice())
            }
        }
    }


    impl<T, A, ReservePol, SizeType> From<&AllocVec<T, A, ReservePol, SizeType>>
        for std::vec::Vec<T>
        where
            T: Clone,
            A: LocalAlloc,
            ReservePol: ReservePolicy<SizeType>,
            SizeType: IntoUsize + FromUsize,
    {

        #[inline]
        fn from(value: &AllocVec<T, A, ReservePol, SizeType>) -> Self {
            value.to_vec()
        }
    }

    impl<T, ReservePol, SizeType> From<AllocVec<T, (), ReservePol, SizeType>>
        for std::vec::Vec<T>
        where
            ReservePol: ReservePolicy<SizeType>,
            SizeType: IntoUsize + FromUsize
    {

        #[inline]
        fn from(value: AllocVec<T, (), ReservePol, SizeType>) -> Self {
            unsafe {
                let (ptr, len, cap) = value.into_raw_parts();
                // SAFETY:
                //   - This is safe because `()` implementation of LocalAlloc uses GlobalAlloc
                Self::from_raw_parts(ptr, len.into_usize(), cap.into_usize())
            }
        }
    }

    use core::num::NonZero;

    impl<ReservePol, SizeType> From<AllocVec<NonZero<u8>, (), ReservePol, SizeType>>
        for std::ffi::CString
        where
            ReservePol: ReservePolicy<SizeType>,
            SizeType: IntoUsize + FromUsize,
    {

        #[inline]
        fn from(value: AllocVec<NonZero<u8>, (), ReservePol, SizeType>) -> Self {
            unsafe {
                let v: std::vec::Vec<u8> = {
                    let (ptr, len, cap) = value.into_raw_parts();
                    std::vec::Vec::from_raw_parts(ptr.cast(), len.into_usize(), cap.into_usize())
                };
                Self::from_vec_unchecked(v)
            }
        }
    }

    impl<T, ReservePol, SizeType> From<AllocVec<T, (), ReservePol, SizeType>>
        for std::sync::Arc<[T]>
        where 
            SizeType: IntoUsize + FromUsize,
            ReservePol: ReservePolicy<SizeType>,
    {

        #[inline]
        fn from(value: AllocVec<T, (), ReservePol, SizeType>) -> Self {
            let (vec_ptr, len, cap) = value.into_raw_parts();
            let rc_ptr = (0..len.into_usize())
                .map(|i| unsafe {
                    vec_ptr.add(i).read()
                }).collect();
            let _ = unsafe {
                AllocVec::<T, (), ReservePol, SizeType>::from_raw_parts(
                    vec_ptr,
                    SizeType::ZERO,
                    cap
                )
            };
            rc_ptr
        }
    }

    impl<T, ReservePol, SizeType> From<AllocVec<T, (), ReservePol, SizeType>>
        for std::boxed::Box<[T]>
        where
            SizeType: IntoUsize + FromUsize,
            ReservePol: ReservePolicy<SizeType>
    {
        #[inline]
        fn from(value: AllocVec<T, (), ReservePol, SizeType>) -> Self {
            value.into_boxed_slice()
        }
    }

    /// Macro for creating a new [`Vec32`].
    ///
    /// # Examples
    /// ``` rust
    /// use leimu_mem::vec32;
    ///
    /// let vec1 = vec32!["foo"; 10];
    /// let vec2 = vec32![0, 1, 2, 3, 4];
    /// ```
    #[macro_export]
    macro_rules! vec32 {
        [$v:expr; $n:expr] => {
            $crate::alloc_vec::Vec32::with_len_reserve_exact($n, $v)
        };
        [$($elem:expr),* $(,)?] => {
            $crate::alloc_vec::Vec32::from([$($elem),*])
        };
    }
}

#[cfg(feature = "std")]
pub use std_features::*;
