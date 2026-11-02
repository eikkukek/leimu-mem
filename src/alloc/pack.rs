//! [`pack_alloc`] API.
//!
//! Provides the [`pack_alloc`] and [`pack_block`] functions, which can be used to allocate data as
//! contiguously as possible.

use core::{
    fmt::Display,
    error::Error,
    num::NonZero,
    ptr::NonNull,
};

use crate::align_up;

use super::*;

trait Ptr {

    fn set_addr(&mut self, addr: NonZero<usize>);
}

impl<T> Ptr for NonNull<T> {

    fn set_addr(&mut self, addr: NonZero<usize>) {
        *self = self.with_addr(addr);
    }
}

/// Indicates an error when calling [`pack_alloc`] or [`pack_block`].
#[derive(Debug)]
pub enum PackAllocError {
    /// Indicates that the [`Layout`] of the allocation or a single block is invalid i.e. overflows
    /// [`isize`].
    LayoutError(LayoutError),
    /// Indicates that the allocation called in [`pack_alloc`] failed.
    AllocError(AllocError),
}

impl Display for PackAllocError {

    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LayoutError(_) => write!(f, "layout error"),
            Self::AllocError(_) => write!(f, "alloc error"),
        }
    }
}

impl Error for PackAllocError {

    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::LayoutError(err) => Some(err),
            Self::AllocError(err) => Some(err),
        }
    }
}

impl From<LayoutError> for PackAllocError {

    #[inline]
    fn from(value: LayoutError) -> Self {
        Self::LayoutError(value)
    }
}

impl From<AllocError> for PackAllocError {

    #[inline]
    fn from(value: AllocError) -> Self {
        Self::AllocError(value)
    }
}

/// A [`pack_alloc`] block containing a mutable reference to a pointer and the required layout of
/// this block.
pub struct PackBlock<'a> {
    ptr: &'a mut dyn Ptr,
    layout: Layout,
}

/// Creates a [`PackBlock`] used by [`pack_alloc`].
///
/// # Members
/// - `ptr`: A mutable reference to a [`NonNull<T>`] where the pointer of this block is stored.
/// - `n`: The number of values of `T` to allocate.
#[inline]
pub fn pack_block<T>(
    ptr: &mut NonNull<T>,
    n: usize
) -> Result<PackBlock<'_>, PackAllocError> {
    Ok(PackBlock {
        ptr,
        layout: Layout::array::<T>(n)?,
    })
}

/// Allocates data as contiguously as possible.
///
/// On success returns a ([`NonNull<u8>`], [`Layout`]) tuple containing the base pointer and layout
/// of the allocation.
///
/// All pointers that are part of this allocation are valid as long as:
/// - `allocator` and its clones are not dropped, and
/// - the returned [`NonNull<u8>`] pointer with [`Layout`] is not [`deallocated`][1].
///
/// This function makes exactly one allocation with `allocator`.
///
/// [1]: LocalAlloc::deallocate
///
/// # Examples
/// ``` rust
///
/// use core::ptr::NonNull;
///
/// use leimu_mem::alloc::{pack_alloc, pack_block};
/// use leimu_mem::alloc::LocalAlloc;
/// 
/// let allocator: MyAlloc = ...; 
///
/// let pack1 = NonNull::dangling();
/// let pack2 = NonNull::dangling();
/// let (ptr, layout) = pack_alloc(
///     &allocator, // Can be any allocator implementing LocalAlloc.
///     [
///         pack_block(pack1, 10).unwrap(), // Allocates 10 `u8`.
///         pack_block(pack2, 1).unwrap(), // Allocates 1 `String`.
///     ],
/// ).unwrap();
/// unsafe {
///     pack2.write("hello".to_string());
///     for i in 0..10 {
///         pack1.add(i).write(i as u8 / 2);
///     }
///     pack2.drop_in_place();
///     allocator.deallocate(ptr, layout);
/// }
/// ```
pub fn pack_alloc<A, const N: usize>(
    allocator: &A,
    mut blocks: [PackBlock; N],
) -> Result<(NonNull<u8>, Layout), PackAllocError>
    where A: LocalAlloc + ?Sized,
{
    let mut align = 1;
    let mut size = 0;
    let mut pack_offs = [0usize; N];
    for i in 0..N {
        let block = unsafe {
            blocks.get_unchecked_mut(i)
        };
        let off = unsafe {
            pack_offs.get_unchecked_mut(i)
        };
        align = align.max(block.layout.align());
        size = align_up(size, block.layout.align());
        *off = size;
        size += block.layout.size();
    }
    let layout = Layout::from_size_align(
        size, align
    )?;
    let ptr = allocator.allocate(layout)?.0;
    let addr = ptr.addr().get();
    for i in 0..N {
        let block = unsafe {
            blocks.get_unchecked_mut(i)
        };
        let off = *unsafe {
            pack_offs.get_unchecked(i)
        };
        block.ptr.set_addr(unsafe {
            NonZero::new_unchecked(addr + off)
        });
    }
    Ok((ptr, layout))
}
