# [leimu-mem]
A library providing a custom allocator API through the `LocalAlloc` trait; a stable Rust version
of the still unstable `std::alloc::Allocator` trait.

This crate provides a vector and a slot map implementation that use the `LocalAlloc` trait for its
allocations as well as utilities like `alloc::pack`, which allocates arbitrarily aligned consecutive
blocks of memory using `LocalAlloc`.

##  Features
### `std` (enabled by default)
Provides an implementation of `LocalAlloc` for the unit type `()` that uses
`std::alloc::alloc` and `std::alloc::dealloc` for allocations and deallocations, and an
`arena` allocator implementation, which can be used by these custom types.

When this feature is enabled, the `slot_map` and `alloc_vec` modules provide type aliases
that uses this unit type implementation of `LocalAlloc` for its allocations.

The `std` feature also provides a somewhat unrelated `smallbox` macro, which can be used to
create "small box" types, which can store `dyn` trait objects inline without an additional heap
allocation up to a given number of bytes similarily to how `SmallVec` stores a given number of elements
inline.

## Usage
``` rust
use leimu_mem::arena::Arena;
use leimu_mem::alloc_vec::FixedVec;
use leimu_mem::slot_map::SlotMap;
let arena = Arena::new(512).unwrap();
let mut vec = FixedVec::with_capacity_in(5, &arena).unwrap();
vec.push(1);
vec.extend_from_slice_copy(&[2, 3]);
vec.extend(4..6);
assert_eq!(vec, [1, 2, 3, 4, 5]);

let mut slot_map: SlotMap<&str> = SlotMap::new();
let key1 = slot_map.insert("foo");
slot_map.remove(key1).unwrap();
let key2 = slot_map.insert("bar");
// Both keys occupy the same index, but they have different versions.
assert_eq!(key1.index(), key2.index());
assert!(slot_map.get(key1).is_err());
assert!(matches!(slot_map.get(key2), Ok(&"bar")));
```
[leimu-mem]: https://crates.io/crates/leimu-mem
