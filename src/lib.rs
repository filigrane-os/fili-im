//! Conversations (fili-im): Filigrane's instant messaging. The daemon owns the store and the connectors; the window
//! and the CLI are its clients.
pub mod client;
pub mod daemon;
pub mod paths;
pub mod protocol;
pub mod store;
