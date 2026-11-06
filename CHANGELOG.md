# Cangelog
All changes to this project will be documented in this file
## unreleased
### Changed
- Small internal stuff.
## [0.4.2] - 2026-11-05
### Changed
- Improved docs.
## [0.4.1] - 2026-11-05
### Fixed
- Docs.
## [0.4.0] - 2026-11-05

### Removed (breaking)
- `SliceVec` and `ArrayVec`.
### Added
- New trait implementations for `AllocVec`.
### Renamed
- `ReservePolicy::grow_factor` to `growth_factor`.
### Fixed
- Docs
## [0.3.2] - 2026-11-03
### Changed
- Use `portable-atomic` instead of `core::sync::atomic` in `Arena` implementation.

## [0.3.1] - 2026-11-02
### Fixed
- License format
## [0.3.0] - 2026-11-02

### Changed
- Improved local alloc dyn compatibility by removing generic Error type
- Breaking changes in LocalAlloc trait functions
- Improved vector types
- Exposed AllocVec
- Breaking changes to AllocVec
- New methods `dedup_by` and `retain_mut` for Pointer
- Renamed `AllocVec` construction methods to be suffixed `_in`, e.g. `with_capacity_in`
- New methods for `grow_factor`, `min_non_zero_capacity` and `grow_one` ReservePolicy
- AllocVec `new` and `new_in` to const functions
- Renamed `NonNullVec` to `SliceVec`

### Added
- Simple generic support for smallbox
- LocalAlloc implementation for ()
- Better documentation for LocalAlloc
- Explicit impl of LocalAlloc for `Arc<T: LocalAlloc>`

### Removed
- StdAlloc in favor of GlobalAlloc implementation for the unit type `()`
- StdVecBase
- LocalAlloc impl for all `T: Deref<Target = impl LocalAlloc>`
- `vec` module in favor of `alloc_vec`, `array_vec` and `slice_vec` modules
### Fixed
- Tests

## [0.2.4] - 2026-05-06

### Changed
- Exposed StdVecBase
- Added push_mut to AllocVec and ArrayVec
- `AllocVec` and `SliceVec` always taking indices as `usize`

## [0.2.3] - 2026-04-26

### Added
- insert_with_index and try_insert_with_index for SlotMap
- assume_init_array for ArrayVec
## [0.2.2] - 2026-04-25
### Added

- License
## [0.2.1] - 2026-04-11

### Fix
- Docs
## [0.2.0] - 2026-04-11
### Changes
- Name change to leimu-mem

## [0.1.1] - 2026-04-10
### Changes
- Updated docs

## [0.1.0] - 2026-04-10
### Added
- Initial crate release
- Custom containers
- Arena allocator implementation

[0.4.2]: https://crates.io/crates/leimu-mem/0.4.2
[0.4.1]: https://crates.io/crates/leimu-mem/0.4.1
[0.4.0]: https://crates.io/crates/leimu-mem/0.4.0
[0.3.2]: https://crates.io/crates/leimu-mem/0.3.2
[0.3.1]: https://crates.io/crates/leimu-mem/0.3.1
[0.3.0]: https://crates.io/crates/leimu-mem/0.3.0
[0.2.4]: https://crates.io/crates/leimu-mem/0.2.4
[0.2.3]: https://crates.io/crates/leimu-mem/0.2.3
[0.2.2]: https://crates.io/crates/leimu-mem/0.2.2
[0.2.1]: https://crates.io/crates/leimu-mem/0.2.1
[0.2.0]: https://crates.io/crates/leimu-mem/0.2.0
[0.1.1]: https://crates.io/crates/nox-mem/0.1.1
[0.1.0]: https://crates.io/crates/nox-mem/0.1.0
