//! A slot map implementation that use [`Allocator`] for its allocations.
//!
//! A slot map is a data structure similar to `Vec<Option<T>>` except each element in the map is
//! associated with a version. When inserting a value to a slot map, the map returns a special index 
//! structure that contains the current version of the slot. That index may be safely used to
//! retrieve the value from the slot map as long as that index is not removed from the map. Removed
//! indices will become "permanently" invalid (as long as the version counter for that slot doesn't overflow).
//!
//! Iterating over a slot map can be expected to be about as fast as iterating over `Vec<Option<T>>`.
//!
//! # New Types
//! - [`SlotMap`]: A slot map using [`Global`][1], requires the `std` feature.
//! - [`DynSlotMap`]: A slot map using a local allocator.
//! - [`FixedSlotMap`]: [`DynSlotMap`] with a fixed-capacity.
//!
//! [1]: std::alloc::Global
//!
//! # Examples
//! ```
//! # #[cfg(feature = "std")]
//! # {
//! use leimu_mem::slot_map::SlotMap;
//!
//! let mut map = SlotMap::new();
//! let key1 = map.insert("hello");
//! let key2 = map.insert("world");
//! assert_eq!(map.get(key1).ok(), Some(&"hello"));
//! assert_eq!(map.remove(key1).ok(), Some("hello"));
//! assert_eq!(map.get(key1).ok(), None);
//! assert_eq!(map.get(key2).ok(), Some(&"world"));
//! # }
//! ```

use core::{
    alloc::{Layout, Allocator},
    error::Error,
    fmt::{self, Debug, Display},
    ptr::NonNull,
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Index, IndexMut},
    num::NonZeroU32,
};

use crate::{
    conditional::{Conditional, False, True},
    reserve::*,
    size::NonZeroSize,
};

/// An error indicating that a [`SlotIndex`] is invalid.
#[derive(Debug)]
pub enum IndexError {
    /// Indicates that the index is stale.
    StaleIndex {
        /// The index that is stale.
        index: SlotIndex<()>,
        /// The version of the slot.
        slot_version: u32,
    },
    /// Indicates that the index is out of bounds.
    IndexOutOfBounds {
        /// The index that is out of bounds.
        index: SlotIndex<()>,
        /// The capacity of the slot map.
        capacity: u32,
    },
}

impl Display for IndexError {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleIndex { index, slot_version } =>
                write!(f, "stale slot map index {index}, slot version is {slot_version}"),
            Self::IndexOutOfBounds { index, capacity } =>
                write!(f, "index {index} is out of bounds with capacity {capacity}")
        }
    }
}

impl Error for IndexError {}

use IndexError::{StaleIndex, IndexOutOfBounds};

mod policy {

    use super::*;

    pub struct Dyn;

    unsafe impl ReservePolicy<u32> for Dyn {

        #[inline]
        fn growth_factor() -> (u32, impl NonZeroSize<Base = u32>) {
            (2, NonZeroU32::ONE)
        }

        #[inline]
        fn grow(current: u32, required: usize) -> core::result::Result<u32, ReserveError<()>> {
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
            if power_of_2 <= current as usize {
                current
            } else if power_of_2 > u32::MAX as usize {
                panic!("maximum capacity {} reached", u32::MAX)
            } else {
                power_of_2.max(2) as u32
            }
        }
    }

    pub type Fixed = crate::alloc_vec::FixedPolicy32;
}

struct Slot<T> {
    value: MaybeUninit<T>,
    version: NonZeroU32,
    next_free_index: Option<Option<u32>>,
}

impl<T: Clone> Clone for Slot<T> {

    fn clone(&self) -> Self {
        Self {
            value: if self.next_free_index.is_none() {
                unsafe {
                    MaybeUninit::new(self.value.assume_init_ref().clone())
                }
            } else {
                MaybeUninit::uninit()
            },
            version: self.version,
            next_free_index: self.next_free_index,
        }
    }
}

impl<T> Slot<T> {

    fn empty(next_free_index: Option<u32>) -> Self {
        Self {
            value: MaybeUninit::uninit(),
            version: NonZeroU32::ONE,
            next_free_index: Some(next_free_index),
        }
    }
}

/// A slot index returned by a slot map.
///
/// Contains a version and an index.
#[must_use]
pub struct SlotIndex<T>
{
    version: NonZeroU32,
    index: u32,
    _marker: PhantomData<T>,
}

impl<T> Display for SlotIndex<T>
{

    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.version, self.index)
    }
}

impl<T> SlotIndex<T>
{

    /// Gets the index part of [`SlotIndex`].
    #[inline]
    pub fn index(&self) -> u32 {
        self.index
    }
    
    /// Gets the version part of [`SlotIndex`].
    #[inline]
    pub fn version(&self) -> u32 {
        self.version.get()
    }
   
    #[inline]
    fn unit_index(self) -> SlotIndex<()> {
        SlotIndex {
            version: self.version,
            index: self.index,
            _marker: PhantomData,
        }
    }
}

unsafe impl<T> Send for SlotIndex<T> {}
unsafe impl<T> Sync for SlotIndex<T> {}

impl<T> Debug for SlotIndex<T> {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotIndex")
            .field("version", &self.version)
            .field("index", &self.index)
            .finish()
    }
}

impl<T> Default for SlotIndex<T>
{

    #[inline]
    fn default() -> Self {
        Self {
            version: NonZeroU32::ONE,
            index: u32::MAX,
            _marker: PhantomData,
        }
    }
}

impl<T> Clone for SlotIndex<T> {

    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SlotIndex<T> {}

impl<T> PartialEq for SlotIndex<T> {

    fn eq(&self, other: &Self) -> bool {
        self.version == other.version &&
        self.index == other.index
    }
}

impl<T> Eq for SlotIndex<T> {}

impl<T> core::hash::Hash for SlotIndex<T> {

    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.version.hash(state);
        self.index.hash(state);
    }
}

mod base {

    use super::*;

    pub struct Map<T, A, ReservePol>
        where
            A: Allocator,
            ReservePol: ReservePolicy<u32>,
    {
        pub(super) ptr: NonNull<Slot<T>>,
        pub(super) capacity: u32,
        pub(super) len: u32,
        pub(super) free_head: Option<u32>,
        pub(super) alloc: A,
        pub(super) _marker: PhantomData<ReservePol>,
    }
}

/// A dynamic slot map storing values of type `T`, backed by allocator `A`.
///
/// Provides stable, opaque handles for accessing values. Removal leaves versioned
/// empty slots and insertions reuse free slots.
///
/// See also [`SlotMap`] for a version using [`Global`][1].
///
/// # Type parameters
///
/// - `T`: value type
/// - `A`: allocator implementing [`Allocator`]
///
/// # Errors
///
/// Insertion and reservations return `Result` and may fail due to:
///
/// - Allocation failure
/// - Capacity exceeding [`u32::MAX`]
///
/// [1]: std::alloc::Global
///
/// # Examples
/// ```
/// # #[cfg(feature = "std")]
/// # {
/// use leimu_mem::arena::Arena;
/// use leimu_mem::slot_map::DynSlotMap;
/// let alloc = Arena::new(512).unwrap();
/// let mut map = DynSlotMap::new_in(&alloc);
/// let key = map.insert("value");
/// map.remove(key);
/// # }
/// ```
pub type DynSlotMap<T, A> = base::Map<T, A, policy::Dyn>;

/// A fixed-capacity slot map storing values of type `T`, backed by allocator `A`.
///
/// Provides stable, opaque handles for accessing values. Removal leaves versioned
/// empty slots and insertions reuse free slots.
///
/// See also [`DynSlotMap`] for a version with dynamic capacity.
///
/// # Type parameters
///
/// - `T`: value type
/// - `A`: allocator implementing [`Allocator`]
///
/// # Errors
///
/// Insertion and reservations return `Result` and may fail due to:
///
/// - Allocation failure
/// - Capacity reaching over u32::MAX
/// - Reaching fixed capacity
///
/// # Example
///
/// ```
/// # #[cfg(feature = "std")]
/// # {
/// use leimu_mem::arena::Arena;
/// use leimu_mem::slot_map::FixedSlotMap;
/// let alloc = Arena::new(512).unwrap();
/// let mut map = FixedSlotMap::with_capacity_in(8, &alloc).unwrap();
/// let key = map.insert("value");
/// map.remove(key);
/// # }
/// ```
pub type FixedSlotMap<T, A> = base::Map<T, A, policy::Fixed>;

impl<T, A, ReservePol>
    base::Map<T, A, ReservePol>
    where
        A: Allocator,
        ReservePol: ReservePolicy<u32>,
{

    /// Creates an empty map with an [allocator][1].
    ///
    /// [1]: Allocator 
    pub fn new_in(alloc: A) -> Self {
        Self {
            ptr: NonNull::dangling(),
            capacity: 0,
            len: 0,
            free_head: None,
            alloc,
            _marker: PhantomData,
        }
    }

    /// Creates a map with `capacity` and an [allocator][1].
    ///
    /// Returns an error if the allocation fails.
    ///
    /// [1]: Allocator 
    pub fn with_capacity_in(capacity: u32, alloc: A) -> Result<Self, ReserveError<()>> {
        if capacity == 0 {
            return Ok(Self::new_in(alloc))
        }
        let mut capacity = ReservePol::grow(capacity, capacity as usize)?;
        let ptr = alloc
            .allocate(Layout::array::<Slot<T>>(capacity as usize)?)
            .map_err(|_| ReserveError::alloc_error(()))?;
        capacity = (ptr.len() / size_of::<Slot<T>>()) as u32;
        let ptr = ptr.cast();
        for i in 0..capacity  - 1 {
            unsafe {
                ptr.add(i as usize).write(Slot::empty(Some(i + 1)));
            }
        }
        unsafe { ptr.add(capacity as usize - 1).write(Slot::empty(None)) };
        Ok(Self {
            ptr,
            capacity,
            len: 0,
            free_head: Some(0),
            alloc,
            _marker: PhantomData,
        })
    }

    /// Tries to insert a value to the map, returning an error if an allocation fails.
    #[inline]
    pub fn try_insert(&mut self, value: T) -> Result<SlotIndex<T>, ReserveError<T>> {
        self.insert_internal::<fn(SlotIndex<T>) -> T>(None, || Some(value)).map_err(|err| {
            let (value, err) = err.recover_value();
            ReserveError::new(err, value.unwrap())
        })
    }
}

impl<T, A, ReservePol> base::Map<T, A, ReservePol>
    where
        A: Allocator,
        ReservePol: ReservePolicy<u32>,
{

    /// Returns a reference to the underlying [allocator][Allocator].
    #[inline]
    pub fn allocator(&self) -> &A {
        &self.alloc
    }

    /// Returns the number of elements contained in the map.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Returns whether the map is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the map's internal capacity.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Reserves more slots.
    ///
    /// Does nothing if `capacity` is less than or equal to the current capacity.
    ///
    /// This may reserve more slots than specified, see [`reserve_exact`][1] for reserving exact
    /// amount of slots.
    ///
    /// [1]: Self::reserve_exact
    #[inline]
    pub fn reserve(&mut self, capacity: u32) {
        let capacity = ReservePol
            ::grow(self.capacity, capacity as usize)
            .unwrap();
        self.reserve_internal(capacity).unwrap();
    }

    /// Reserves an exact amount of slots.
    ///
    /// Does nothing if `capacity` is less than or equal to the current capacity.
    #[inline]
    pub fn reserve_exact(&mut self, capacity: u32) {
        self.reserve_internal(capacity).unwrap();
    }

    /// Tries to reserve more slots, returning an error if maximum capacity is reached, or if an
    /// allocation fails.
    #[inline]
    pub fn try_reserve(&mut self, capacity: u32) -> Result<(), ReserveError<()>> {
        let capacity = ReservePol
            ::grow(self.capacity, capacity as usize)?;
        self.reserve_internal(capacity)
    }

    /// Tries to reserve an exact amount of slots, returning an error if maximum capacity is reached,
    /// or if an allocation fails.
    #[inline]
    pub fn try_reserve_exact(&mut self, capacity: u32) -> Result<(), ReserveError<()>> {
        self.reserve_internal(capacity)
    }

    fn reserve_internal(&mut self, new_capacity: u32) -> Result<(), ReserveError<()>> {
        if new_capacity <= self.capacity { return Ok(()) }
        let ptr = self.alloc
            .allocate(Layout::array::<Slot<T>>(new_capacity as usize)?)
            .map_err(|_| ReserveError::alloc_error(()))?;
        let capacity = (ptr.len() / size_of::<Slot<T>>()) as u32;
        let ptr = ptr.cast();
        unsafe {
            self.ptr
                .copy_to_nonoverlapping(
                    ptr,
                    self.capacity() as usize,
                );
        };
        for i in self.capacity..capacity - 1 {
            unsafe {
                ptr.add(i as usize).write(Slot::empty(Some(i + 1)));
            }
        }
        unsafe {
            ptr.add(capacity as usize - 1).write(
                Slot::empty(self.free_head)
            );
        }
        self.free_head = Some(self.capacity);
        if self.capacity != 0 {
            unsafe {
                self.alloc
                    .deallocate(
                        self.ptr.cast(),
                        Layout::array::<Slot<T>>(self.capacity as usize)?
                    );
            }
        }
        self.ptr = ptr;
        self.capacity = capacity;
        Ok(())
    }

    fn insert_internal<F: FnOnce(SlotIndex<T>) -> T>(
        &mut self,
        f: Option<F>,
        get_value: impl FnOnce() -> Option<T>,
    ) -> Result<SlotIndex<T>, ReserveError<Option<T>>>
    {
        if self.free_head.is_none() {
            let capacity = self.capacity as usize * 2;
            if capacity > u32::MAX as usize {
                return Err(ReserveError::max_capacity_exceeded(
                    u32::MAX, capacity,
                    get_value()
                ))
            }
            let capacity = match ReservePol::grow_one(
                self.capacity,
                Layout::new::<Slot<T>>())
            {
                Ok(cap) => cap,
                Err(err) => return Err(err.with_value(get_value()))
            };
            if let Err(err) = self.try_reserve(capacity) {
                return Err(err.with_value(get_value()))
            }
        }
        let index = self.free_head.unwrap();
        let free_slot = unsafe { self.ptr.add(index as usize).as_mut() };
        let index = SlotIndex {
            version: free_slot.version,
            index,
            _marker: PhantomData,
        };
        self.free_head = free_slot.next_free_index.unwrap();
        free_slot.value.write(f.map(|f| f(index)).unwrap_or_else(|| unsafe { get_value()
            .unwrap_unchecked()
        }));
        free_slot.next_free_index = None;
        self.len += 1;
        Ok(index)
    }

    /// Inserts a value to the map.
    ///
    /// May panic if an allocation fails.
    #[inline]
    pub fn insert(&mut self, value: T) -> SlotIndex<T> {
        self.insert_internal::<fn(SlotIndex<T>) -> T>(None, || Some(value)).unwrap()
    }

    /// Inserts a value to the map with closure that takes in the new index.
    ///
    /// May panic if an allocation fails.
    pub fn insert_with_index<F>(&mut self, f: F) -> SlotIndex<T>
        where F: FnOnce(SlotIndex<T>) -> T
    {
        self.insert_internal(Some(f), || None).unwrap()
    }

    /// Tries to inserts a value to the map with closure that takes in the index and may return an
    /// error.
    ///
    /// May panic if an allocation fails.
    pub fn try_insert_with_index<F, E>(&mut self, f: F) -> Result<SlotIndex<T>, E>
        where F: FnOnce(SlotIndex<T>) -> Result<T, E>
    {
        if self.free_head.is_none() {
            let capacity = ReservePol::grow_one(
                self.capacity,
                Layout::new::<Slot<T>>()
            ).expect("failed to reserve");
            self.reserve_exact(capacity);
        }
        let index = self.free_head.unwrap();
        let free_slot = unsafe { self.ptr.add(index as usize).as_mut() };
        let index = SlotIndex {
            version: free_slot.version,
            index,
            _marker: PhantomData,
        };
        self.free_head = free_slot.next_free_index.unwrap();
        free_slot.value.write(f(index)?);
        free_slot.next_free_index = None;
        self.len += 1;
        Ok(index)
    }

    /// Removes a value from the slot map, returning an error if the index is stale or otherwise
    /// invalid.
    pub fn remove(&mut self, index: SlotIndex<T>) -> Result<T, IndexError>
    {
        if index.index >= self.capacity {
            return Err(
                IndexOutOfBounds {
                    index: index.unit_index(),
                    capacity: self.capacity,
                }
            )
        }
        let ptr = unsafe { self.ptr.add(index.index as usize) };
        let mut slot = unsafe { ptr.read() };
        let index_version = index.version;
        if slot.version != index_version {
            return Err(StaleIndex {
                index: index.unit_index(),
                slot_version: slot.version.get(),
            })
        }
        let value = unsafe { slot.value.assume_init() };
        slot.version = NonZeroU32
            ::new(slot.version.get().wrapping_add(1))
            .unwrap_or(NonZeroU32::ONE);
        slot.next_free_index = Some(self.free_head);
        slot.value = MaybeUninit::uninit();
        unsafe {
            ptr.write(slot);
        }
        self.free_head = Some(index.index);
        self.len -= 1;
        Ok(value)
    }

    /// Returns whether the map contains `index`.
    pub fn contains(&self, index: SlotIndex<T>) -> bool {
        if index.index >= self.capacity {
            return false
        }
        let index_version = index.version;
        let slot = unsafe { self.ptr.add(index.index as usize).as_ref() };
        slot.version == index_version
    }

    /// Gets a reference to a value at `index`, returning an error if the index is stale or othewise
    /// invalid.
    pub fn get(&self, index: SlotIndex<T>) -> Result<&T, IndexError> {
        if index.index >= self.capacity {
            return Err(IndexOutOfBounds {
                index: index.unit_index(),
                capacity: self.capacity,
            })
        }
        let index_version = index.version;
        let slot = unsafe { self.ptr.add(index.index as usize).as_ref() };
        if slot.version != index_version {
            return Err(StaleIndex {
                index: index.unit_index(),
                slot_version: slot.version.get(),
            })
        }
        assert!(slot.next_free_index.is_none(), "invalid index");
        unsafe {
            Ok(slot.value.assume_init_ref())
        }
    }

    /// Gets a mutable reference to a value at `index`, returning an error if the index is stale or
    /// otherwise invalid.
    pub fn get_mut(&mut self, index: SlotIndex<T>) -> Result<&mut T, IndexError> {
        if index.index >= self.capacity {
            return Err(IndexOutOfBounds {
                index: index.unit_index(),
                capacity: self.capacity,
            })
        }
        let index_version = index.version;
        let slot = unsafe { self.ptr.add(index.index as usize).as_mut() };
        if slot.version != index_version {
            return Err(StaleIndex {
                index: index.unit_index(),
                slot_version: slot.version.get(),
            })
        }
        assert!(slot.next_free_index.is_none(), "invalid index");
        unsafe {
            Ok(slot.value.assume_init_mut())
        }
    }
    
    /// Gets a reference to value at `index` bypassing all validity checks including bounds checks.
    ///
    /// # Safety
    /// The index *must* be a valid index, otherwise the value might be uninitialized or out of
    /// bounds.
    #[inline]
    pub unsafe fn get_unchecked(&self, index: SlotIndex<T>) -> &T {
        unsafe {
            self.ptr.add(index.index as usize).as_ref()
            .value.assume_init_ref()
        }
    }

    /// Gets a mutable reference to value at `index` bypassing all validity checks including bounds
    /// checks.
    ///
    /// # Safety
    /// The index *must* be a valid index, otherwise the value might be uninitialized or out of
    /// bounds.
    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: SlotIndex<T>) -> &mut T {
        unsafe {
            self.ptr.add(index.index as usize).as_mut()
            .value.assume_init_mut()
        }
    }

    /// Clears all slots in the map.
    pub fn clear(&mut self) {
        for i in 0..self.capacity() {
            unsafe {
                let slot = self.ptr.add(i as usize).read();
                if slot.next_free_index.is_none() {
                    self.remove(SlotIndex {
                        version: slot.version,
                        index: i,
                        _marker: PhantomData,
                    }).unwrap();
                }
            }
        }
        self.len = 0;
    }

    /// Returns an iterator over all indices and values.
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        unsafe {
            Iter::new(self.ptr, self.ptr.add(self.capacity as usize))
        }
    }

    /// Returns an iterator over all indices.
    #[inline]
    pub fn indices(&self) -> IterIndices<'_, T> {
        unsafe {
            IterIndices::new(self.ptr, self.ptr.add(self.capacity() as usize))
        }
    }

    /// Returns an iterator over all values.
    #[inline]
    pub fn values(&self) -> IterValues<'_, T> {
        unsafe {
            IterValues::new(self.ptr, self.ptr.add(self.capacity() as usize))
        }
    }

    /// Returns an iterator over all indices and values, with the values returned as mutable
    /// references.
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        unsafe {
            IterMut::new(self.ptr, self.ptr.add(self.capacity as usize))
        }
    }

    /// Returns an iterator over all values as mutable references.
    #[inline]
    pub fn values_mut(&self) -> IterValuesMut<'_, T> {
        unsafe {
            IterValuesMut::new(self.ptr, self.ptr.add(self.capacity() as usize))
        }
    }
}

mod iter {

    use super::*;

    pub struct IndexValue;
    pub struct Indices;
    pub struct Values;

    pub struct Base<'a, T, IsMut, Type>
        where
            IsMut: Conditional,
    {
        pub(super) ptr: NonNull<Slot<T>>,
        pub(super) end: NonNull<Slot<T>>,
        pub(super) index: u32,
        pub(super) _marker: PhantomData<(&'a T, IsMut, Type)>
    }

    impl<'a, T, IsMut, Type> Base<'a, T, IsMut, Type>
        where
            IsMut: Conditional,
    {

        pub(super) unsafe fn new(
            mut ptr: NonNull<Slot<T>>,
            end: NonNull<Slot<T>>
        ) ->  Self
        {
            let mut index = 0;
            unsafe {
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
            }
            Self {
                ptr,
                end,
                index,
                _marker: PhantomData,
            }
        }
    }
}

/// An iterator over indices and values.
pub type Iter<'a, T> = iter::Base<'a, T, False, iter::IndexValue>;
/// An iterator over indices.
pub type IterIndices<'a, T> = iter::Base<'a, T, False, iter::Indices>;
/// An iterator over values.
pub type IterValues<'a, T> = iter::Base<'a, T, False, iter::Values>;

/// An iterator over indices and values, where the values are mutable references.
pub type IterMut<'a, T> = iter::Base<'a, T, True, iter::IndexValue>;
/// An iterator over values as mutable references.
pub type IterValuesMut<'a, T> = iter::Base<'a, T, True, iter::Values>;

impl<'a, T> Iterator for Iter<'a, T>
{

    type Item = (SlotIndex<T>, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        }
        else {
            unsafe {
                let item = self.ptr.as_ref();
                let current_idx = self.index;
                let mut ptr = self.ptr.add(1);
                let end = self.end;
                let mut index = self.index + 1;
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
                self.ptr = ptr;
                self.index = index;
                Some((
                    SlotIndex {
                        version: item.version,
                        index: current_idx,
                        _marker: PhantomData,
                    },
                    item.value.assume_init_ref(),
                ))
            }
        }
    }
}

impl<'a, T> Iterator for IterIndices<'a, T>
{

    type Item = SlotIndex<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        }
        else {
            unsafe {
                let item = self.ptr.as_ref();
                let current_idx = self.index;
                let mut ptr = self.ptr.add(1);
                let end = self.end;
                let mut index = self.index + 1;
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
                self.ptr = ptr;
                self.index = index;
                Some(
                    SlotIndex {
                        version: item.version,
                        index: current_idx,
                        _marker: PhantomData,
                    },
                )
            }
        }
    }
}

impl<'a, T> Iterator for IterValues<'a, T>
{

    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        }
        else {
            unsafe {
                let item = self.ptr.as_ref();
                let mut ptr = self.ptr.add(1);
                let end = self.end;
                let mut index = self.index + 1;
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
                self.ptr = ptr;
                self.index = index;
                Some(
                    item.value.assume_init_ref(),
                )
            }
        }
    }
}

impl<'a, T> Iterator for IterMut<'a, T>
{

    type Item = (SlotIndex<T>, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        }
        else {
            unsafe {
                let item = self.ptr.as_mut();
                let current_idx = self.index;
                let mut ptr = self.ptr.add(1);
                let end = self.end;
                let mut index = self.index + 1;
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
                self.ptr = ptr;
                self.index = index;
                Some((
                    SlotIndex {
                        version: item.version,
                        index: current_idx,
                        _marker: PhantomData,
                    },
                    item.value.assume_init_mut(),
                ))
            }
        }
    }
}

impl<'a, T> Iterator for IterValuesMut<'a, T>
{

    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        }
        else {
            unsafe {
                let item = self.ptr.as_mut();
                let mut ptr = self.ptr.add(1);
                let end = self.end;
                let mut index = self.index + 1;
                while ptr != end {
                    if ptr.as_ref().next_free_index.is_none() {
                        break
                    }
                    ptr = ptr.add(1);
                    index += 1;
                }
                self.ptr = ptr;
                self.index = index;
                Some(
                    item.value.assume_init_mut(),
                )
            }
        }
    }
}

/// An iterator that moves out of the slot map.
pub struct IntoIter<T, A, ReservePol>
    where
        A: Allocator,
        ReservePol: ReservePolicy<u32>,
{
    slot_map: base::Map<T, A, ReservePol>,
    off: u32,
}

impl<T, A, ReservePol> Iterator for IntoIter<T, A, ReservePol>
    where
        A: Allocator,
        ReservePol: ReservePolicy<u32>,
{

    type Item = (SlotIndex<T>, T);

    fn next(&mut self) -> Option<Self::Item> {
        let mut iter = unsafe { IterMut::new(
            self.slot_map.ptr.add(self.off as usize),
            self.slot_map.ptr.add(self.slot_map.capacity as usize)
        )};
        iter.index += self.off;
        self.off = iter.index;
        let index = iter.next()?.0;
        Some((index, self.slot_map.remove(index).unwrap()))
    }
}

use base::Map;

crate::macros::impl_traits!(
    for Map<T, A: [Allocator], ReservePol: [ReservePolicy<u32>]>
    Index<SlotIndex<T>> =>

        type Output = T;

        fn index(&self, index: SlotIndex<T>) -> &Self::Output {
            if index.index >= self.capacity {
                panic!("index {} out of bounds with capacity {}",
                    index.index, self.capacity
                )
            }
            let index_version = index.version;
            let slot = unsafe { self.ptr.add(index.index as usize).as_ref() };
            if slot.version != index_version {
                panic!("stale index: slot version {}, index version {}",
                    slot.version, index_version,
                );
            }
            assert!(slot.next_free_index.is_none(), "invalid index");
            unsafe {
                slot.value.assume_init_ref()
            }
        }
    ,
    IndexMut<SlotIndex<T>> =>

        fn index_mut(&mut self, index: SlotIndex<T>) -> &mut Self::Output {
            if index.index >= self.capacity {
                panic!("index {} out of bounds with capacity {}", index.index, self.capacity)
            }
            let index_version = index.version;
            let slot = unsafe { self.ptr.add(index.index as usize).as_mut() };
            if slot.version != index_version {
                panic!("stale index: slot version {}, index version {}", 
                    slot.version, index_version,
                );
            }
            assert!(slot.next_free_index.is_none(), "invalid index");
            unsafe {
                slot.value.assume_init_mut()
            }
        }
    ,
    IntoIterator for &'map =>

        type Item = (SlotIndex<T>, &'map T);
        type IntoIter = Iter<'map, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    ,
    IntoIterator for mut &'map =>

        type Item = (SlotIndex<T>, &'map mut T);
        type IntoIter = IterMut<'map, T>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    ,
    IntoIterator =>

        type Item = (SlotIndex<T>, T);
        type IntoIter = IntoIter<T, A, ReservePol>;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            IntoIter {
                slot_map: self,
                off: 0,
            }
        }
    ,
    Drop =>

        fn drop(&mut self) {
            self.clear();
            if self.capacity() != 0 {
                unsafe {
                    self.alloc
                        .deallocate(
                            self.ptr.cast(),
                            Layout::array::<Slot<T>>(
                                self.capacity as usize
                            ).unwrap_unchecked()
                        );
                }
            }
        }
    ,
);

unsafe impl<
    T: Send,
    A: Allocator + Send,
    ReservePol: ReservePolicy<u32>,
> Send for base::Map<T, A, ReservePol> {}

unsafe impl<
    T: Sync,
    A: Allocator + Sync,
    ReservePol: ReservePolicy<u32>,
> Sync for base::Map<T, A, ReservePol> {}

#[cfg(feature = "std")]
mod std_features {

    use super::*;

    use std::alloc::Global;

    /// A dynamic slot map storing values of type `T`, backed by [`Global`][1].
    ///
    /// # Type parameters
    ///
    /// - `T`: the value type
    ///
    /// [1]: std::alloc::Global
    ///
    /// # Example
    ///
    /// ``` rust
    /// use leimu_mem::slot_map::SlotMap;
    ///
    /// let mut map = SlotMap::new();
    /// let key1 = map.insert("hello");
    /// let key2 = map.insert("world");
    /// assert_eq!(map.get(key1).ok(), Some(&"hello"));
    /// assert_eq!(map.remove(key1).ok(), Some("hello"));
    /// assert_eq!(map.get(key1).ok(), None);
    /// assert_eq!(map.get(key2).ok(), Some(&"world"));
    /// ```
    pub type SlotMap<T = u32> = base::Map<T, Global, policy::Dyn>;

    impl<T> SlotMap<T>
    {

        /// Creates an empty map.
        pub fn new() -> Self {
            Self {
                ptr: NonNull::dangling(),
                capacity: 0,
                len: 0,
                free_head: None,
                alloc: Global,
                _marker: PhantomData,
            }
        }

        /// Creates a new map with `capacity`.
        pub fn with_capacity(capacity: u32) -> Self {
            if capacity == 0 {
                return Self::new()
            }
            let mut capacity = policy::Dyn::grow(0, capacity as usize).unwrap();
            let ptr = Global
                .allocate(Layout
                    ::array::<Slot<T>>(capacity as usize)
                    .expect("layout error")
                ).expect("global alloc failed");
            capacity = (ptr.len() / size_of::<Slot<T>>()) as u32;
            let ptr = ptr.cast();
            for i in 0..capacity - 1 {
                unsafe {
                    ptr.add(i as usize).write(Slot::empty(Some(i + 1)));
                }
            }
            unsafe { ptr.add(capacity as usize - 1).write(Slot::empty(None)) };
            Self {
                ptr,
                capacity,
                len: 0,
                free_head: Some(0),
                alloc: Global,
                _marker: PhantomData,
            }
        }
    }

    impl<T> Clone for SlotMap<T>
        where
            T: Clone,
    {

        fn clone(&self) -> Self {
            if self.capacity == 0 {
                return Self::new()
            }
            // Ignore any extra bytes allocated
            let ptr = Global
                .allocate(unsafe { Layout
                    ::array::<Slot<T>>(self.capacity as usize)
                    .unwrap_unchecked()
                }).expect("global alloc failed");
            let ptr = ptr.cast();
            for i in 0..self.capacity as usize {
                unsafe {
                    ptr.add(i).write(
                        self.ptr.add(i).as_ref().clone()
                    );
                }
            }
            Self {
                ptr,
                capacity: self.capacity,
                len: self.len,
                free_head: self.free_head,
                alloc: Global,
                _marker: PhantomData,
            }
        }
    }

    impl<T> Default for SlotMap<T> {

        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "std")]
pub use std_features::*;
