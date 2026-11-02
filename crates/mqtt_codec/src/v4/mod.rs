//! MQTT v3.1.1 (MQTT 4) protocol implementation.
//!
//! This module provides encoding, decoding, and validation for all
//! MQTT v3.1.1 control packet types as defined in the MQTT v3.1.1 specification.
//!
//! # Main components
//!
//! - [`MqttCodec`]: Combined encoder and decoder for async I/O.
//! - [`Packet`]: Enum containing all MQTT v3.1.1 packet types.
//! - [`ConnectPacketBuilder`]: Fluent API for building CONNECT packets.
//! - [`PublishPacketBuilder`]: Fluent API for building PUBLISH packets.

mod builder;
mod codec;
mod decoder;
mod encoder;
mod packet;
mod return_codes;
mod validation;

pub use builder::*;
pub use codec::*;
pub use decoder::*;
pub use encoder::*;
pub use packet::*;
pub use return_codes::*;
pub use validation::*;
