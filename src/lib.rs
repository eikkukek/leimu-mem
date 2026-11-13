//! This crate provides a [vector] and a [slot map] implementation that use the [`Allocator`] trait for their
//! allocations and simple utility modules [`pack`] and [`iter`].
//!
//! # Features
//! ## `std` (enabled by default)
//! Provides a simple arena/bump allocator type [`Arena`][arena::Arena] and a somewhat unrelated [`smallbox`] macro.
//!
//! The [`smallbox`] macro can be used to create "small box" types, which can store `dyn` trait objects inline without
//! an needing an extra allocation up to a given number of bytes similarily to how `SmallVec` stores a given number
//! of elements inline.
//!
//! [vector]: alloc_vec
//! [slot map]: slot_map
//! [`Allocator`]: std::alloc::Allocator
//!
//! # Usage
//! ```
//! # #[cfg(feature = "std")]
//! # {
//! use leimu_mem::arena::Arena;
//! use leimu_mem::alloc_vec::FixedVec;
//! use leimu_mem::slot_map::SlotMap;
//!
//! let arena = Arena::new(512).unwrap();
//! let mut vec = FixedVec::with_capacity_in(5, &arena).unwrap();
//! vec.push(1);
//! vec.extend_from_slice_copy(&[2, 3]);
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
//! # }
//! ```

#![no_std]

#![warn(missing_docs)]
#![warn(rustdoc::all)]

#[cfg(feature = "std")]
extern crate std;

pub mod conditional;
pub mod reserve;
pub mod size;
pub mod pack;
pub mod alloc_vec;
pub mod slot_map;
#[cfg(feature = "std")]
pub mod arena;
#[cfg(feature = "std")]
mod smallbox;
pub mod iter;
#[cfg(test)]
mod tests;

mod macros;

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
#[cfg(feature = "std")]
pub struct ReadMeDoctests;
