#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate bincode;
extern crate magnetic;
extern crate trailing_cell;
extern crate byteorder;
extern crate bidir_map;

mod server;
mod api;
mod messaging;

#[cfg(test)]
mod tests;


pub use messaging::{Message};
pub use api::*;