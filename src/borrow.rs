//! An extension of [`borrow`][1].
//!
//! # New types
//! - [`CowMut`]: A new smart pointer similar to [`Cow`]. Requires the "std" feature.
//!
//! [1]: core::borrow

#[cfg(feature = "std")]
mod cow;

pub use core::borrow::*;


#[cfg(feature = "std")]
mod std_features {

    pub use std::borrow::*;

    pub use super::cow::*;
}

#[cfg(feature = "std")]
pub use std_features::*;
