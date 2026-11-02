//! Simple type-level conditionals.
//!
//! # New types
//! - [`True`] and [`False`]: Represent `true` and `false` at a type level.
//!
//! # New traits
//! - [`Conditional`]: A trait for working with conditionals at a type level.

/// A trait for working with conditionals at a type level.
///
/// See [`True`] and [`False`].
pub trait Conditional: 'static + Copy {
    
    /// The constant value of the type.
    const VALUE: bool;
}

/// Represents `true` at a type level.
#[derive(Clone, Copy)]
pub struct True;

impl Conditional for True {

    const VALUE: bool = true;
}

/// Represents `false` at a type level.
#[derive(Clone, Copy)]
pub struct False;

impl Conditional for False {

    const VALUE: bool = false;
}
