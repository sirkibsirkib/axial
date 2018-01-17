#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate bincode;
extern crate magnetic;
extern crate byteorder;
extern crate bidir_map;


mod server;
mod client;
mod messaging;
mod common;
mod coupler;

#[cfg(test)]
mod tests;


pub use common::{
    Message, Clientward, Serverward, ClientId,
    Authenticator, AuthenticationError,
};
pub use client::{client_start, ClientStartError, RemoteServerwardSender, ServerwardSender};
pub use server::{server_start, ServerStartError, RemoteClientwardSender, ClientwardSender, Signed};
pub use coupler::{coupler_start, LocalServerwardSender, LocalClientwardSender};