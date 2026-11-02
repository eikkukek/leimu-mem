//! Tests

use crate::*;

#[test]
#[cfg(feature = "std")]
fn alloc_vec() -> Result<(), reserve::ReserveError<()>> {
    use arena::Arena;
    use alloc_vec::FixedVec;
    let arena = Arena::new(512)?;
    let mut vec = FixedVec::with_capacity_in(5, &arena).unwrap();
    vec.push(1);
    vec.append(&[2, 3]);
    vec.extend(4..6);
    assert_eq!(vec, [1, 2, 3, 4, 5]);
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
        ptr::NonNull,
        sync::atomic::{self, AtomicUsize},
        fmt::{self, Display},
    };
    use alloc::*;
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
    #[derive(Debug)]
    struct Error;
    impl Display for Error {

        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            "capacity of 512 exceeded".fmt(f)
        }
    }
    impl core::error::Error for Error {}
    unsafe impl LocalAlloc for MyAllocator {

        fn allocate(&self, layout: Layout) -> Result<(NonNull<u8>, usize), AllocError> {
            let mut aligned_start = 0;
            self.pos.try_update(
                atomic::Ordering::AcqRel,
                atomic::Ordering::Acquire,
                |pos|  {
                    let start = self.data.as_ptr() as usize + pos;
                    aligned_start = align_up(start, layout.align());
                    let end = aligned_start + layout.size();
                    if end > self.data.as_ptr() as usize + 512 {
                        return None
                    }
                    Some(end - self.data.as_ptr() as usize)
                }
            ).map_err(|_| AllocError::new(Error))?;
            unsafe {
                Ok((
                    NonNull::new_unchecked(aligned_start as *mut u8),
                    layout.size(),
                ))
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
