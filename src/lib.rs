#![doc = include_str!("../readme.md")]

mod access;
mod error;
mod shape;
mod stream;
mod template;

pub use access::JsonGet;
pub use error::{Error, JsonKind, ParseStringErrorKind, Result};
pub use stream::*;
pub use template::{format_value, parse_string};

#[cfg(test)]
mod tests;
