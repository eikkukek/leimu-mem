/// (offset + align - 1) & !(align - 1)
#[inline]
pub const fn align_up(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}

/// offset & !(align - 1)
#[inline]
pub const fn align_down(offset: usize, align: usize) -> usize {
    offset & !(align - 1)
}

/// (offset + align - 1) & !(align - 1)
#[inline]
pub const fn align_up_u64(offset: u64, align: u64) -> u64 {
    (offset + align - 1) & !(align - 1)
}

/// offset & !(align - 1)
#[inline]
pub const fn align_down_u64(offset: u64, align: u64) -> u64 {
    offset & !(align - 1)
}
