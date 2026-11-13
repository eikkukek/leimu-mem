#[test]
fn shrink() {
    use super::*;

    let arena = Arena::new(512).unwrap();

    struct Foo{_x: u32, _y: u32}

    let _ = arena.allocate(Layout::new::<u32>()).unwrap();
    let ptr1 = arena.allocate(Layout::new::<Foo>()).unwrap();
    let ptr2 = unsafe { arena.shrink(
        ptr1.cast(),
        Layout::new::<Foo>(),
        Layout::new::<u64>(),
    ).unwrap() };
    assert!(
        ptr1.addr() != ptr2.addr() &&
        ptr2.cast::<u64>().is_aligned(),
    );
    let ptr3 = unsafe { arena.shrink(
        ptr2.cast(),
        Layout::new::<u64>(),
        Layout::new::<u32>())
    }.unwrap();
    assert_eq!(ptr2.addr(), ptr3.addr());
    let guard = arena.guard();
    assert_eq!(guard.capacity(), arena.capacity() - arena.used())
}
