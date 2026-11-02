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
