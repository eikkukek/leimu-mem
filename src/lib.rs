//! A library providing various collections for use with [`custom allocators`][1].
//!
//! This includes vector types [`alloc_vec`], [`array_vec`] and [`slice_vec`] as well as
//! [`a slot map implementation`][3].
//!
//! In addition, this crate contains an [`arena`] allocator implementation.
//!
//! # Usage
//!```ignore
//! use leimu_mem::arena::Arena;
//! use leimu_mem::alloc_vec::FixedVec;
//! use leimu_mem::slot_map::SlotMap;
//!
//! let arena = Arena::new(512).unwrap();
//! let mut vec = FixedVec::with_capacity_in(5, &arena).unwrap();
//! vec.push(1);
//! vec.append(&[2, 3]);
//! vec.extend(4..6);
//! assert_eq!(vec, [1, 2, 3, 4, 5]);
//!
//! let mut slot_map: SlotMap<&str> = SlotMap::new();
//! let key1 = slot_map.insert("foo");
//! slot_map.remove(key1).unwrap();
//! let key2 = slot_map.insert("bar");
//! // Both keys occupy the same index, but they have different versions.
//! assert_eq!(key1.index(), key2.index());
//! assert!(slot_map.get(key1).is_err());
//! assert!(matches!(slot_map.get(key2), Ok(&"bar")));
//! ```
//!
//! [1]: alloc::LocalAlloc
//! [3]: slot_map

#![no_std]

#![warn(missing_docs)]

#[cfg(feature = "std")]
extern crate std;

pub mod conditional;
pub mod reserve;
pub mod int;
pub mod alloc_vec;
pub mod array_vec;
pub mod slice_vec;
pub mod slot_map;
pub mod alloc;
#[cfg(feature = "std")]
pub mod arena;
#[cfg(feature = "std")]
mod smallbox;
pub mod iter;
#[cfg(test)]
mod tests;

mod macros;
mod const_fn;

pub use const_fn::*;
