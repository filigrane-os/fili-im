//! Conversations (fili-im): Filigrane's instant messaging. The daemon owns the store and the connectors; the window
//! and the CLI are its clients.
pub mod client;
pub mod ctl;
pub mod daemon;
pub mod paths;
pub mod protocol;
pub mod store;
pub mod window;
pub mod window_parts;
#[cfg(feature = "gui")]
extern crate self as fili_im;
#[cfg(feature = "gui")]
pub mod gui;
