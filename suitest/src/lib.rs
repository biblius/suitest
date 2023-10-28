#![doc = include_str!(concat!("../", std::env!("CARGO_PKG_README")))]

pub use suitest_macros::*;

pub mod internal {
    pub use futures_util;
    pub use once_cell;
    pub use once_cell::sync::OnceCell;
}
