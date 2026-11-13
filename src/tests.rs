//! Tests

use crate::*;

#[test]
#[cfg(feature = "std")]
fn alloc_vec() -> Result<(), reserve::ReserveError<()>> {
    use std::string::ToString;
    use arena::Arena;
    use alloc_vec::FixedVec;
    let arena = Arena::new(512)?;
    let mut vec = FixedVec::with_capacity_in(6, &arena).unwrap();
    vec.push("foo".to_string());
    vec.extend_from_slice(&["bar".to_string(), "hello".to_string()]);
    vec.extend(('a'..='c').map(|c| c.to_string()));
    assert_eq!(vec, ["foo", "bar", "hello", "a", "b", "c"]);
    vec.remove(2);
    vec.insert(4, "aa".to_string());
    vec.retain_mut(|f| {
        let retain = f.contains("a");
        if retain {
            *f = f.to_uppercase();
        }
        retain
    });
    assert_eq!(vec, ["BAR", "A", "AA"]);
    Ok(())
}

#[test]
#[cfg(feature = "std")]
fn collect_in() -> Result<(), reserve::ReserveError<()>> {
    use arena::Arena;
    use iter::CollectIn;
    use alloc_vec::FixedVec;
    let arena = Arena::new(512)?;
    let mut vec: FixedVec<&str, _> = ["Hello", ",", "World"]
        .into_iter()
        .collect_in(&arena)?;
    assert_eq!(vec, ["Hello", ",", "World"]);
    vec.retain(|&s| s != ",");
    assert_eq!(vec, ["Hello", "World"]);
    Ok(())
}

#[test]
fn stack_alloc() -> Result<(), reserve::ReserveError<()>> {
    use core::{
        alloc::*,
        ptr::NonNull,
        sync::atomic::{self, AtomicUsize},
    };
    use alloc_vec::FixedVec;
    struct MyAllocator {
        data: [u8; 512],
        pos: AtomicUsize,
    }
    impl MyAllocator {

        fn new() -> Self {
            Self { data: [0; 512], pos: AtomicUsize::new(0) }
        }
    }
    unsafe impl Allocator for MyAllocator {

        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let mut aligned_start = 0;
            self.pos.try_update(
                atomic::Ordering::AcqRel,
                atomic::Ordering::Acquire,
                |pos|  {
                    let start = self.data.as_ptr() as usize + pos;
                    aligned_start = align_up!(start, layout.align());
                    let end = aligned_start + layout.size();
                    if end > self.data.as_ptr() as usize + 512 {
                        return None
                    }
                    Some(end - self.data.as_ptr() as usize)
                }
            ).map_err(|_| AllocError)?;
            unsafe {
                Ok(
                    NonNull::slice_from_raw_parts(
                        NonNull::new_unchecked(aligned_start as *mut u8),
                        layout.size(),
                    ),
                )
            }
        }

        unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
    }
    let allocator = MyAllocator::new();
    let mut vec = FixedVec::<u8, _>
        ::with_capacity_in(512, &allocator)?;
    vec.extend([0, 1, 2, 3]);
    assert_eq!(vec, [0, 1, 2, 3]);
    assert!(allocator.allocate(Layout::new::<u8>()).is_err());
    Ok(())
}
