//! MQTT v5.0 (Level 5) protocol implementation.
//!
//! This module provides encoding, decoding, and validation for all
//! MQTT v5.0 control packet types, including support for properties
//! and reason codes.
//!
//! # Main components
//!
//! - [`MqttCodec`]: Combined encoder and decoder for async I/O.
//! - [`Packet`]: Enum containing all MQTT v5.0 packet types.
//! - [`Properties`]: MQTT v5.0 property collection.
//! - [`ConnectPacketBuilder`]: Fluent API for building v5.0 CONNECT packets.

mod builder;
mod codec;
mod decoder;
mod encoder;
mod packet;
mod properties_codec;
mod property_id;
mod validation;

pub use builder::*;
pub use codec::*;
pub use decoder::*;
pub use encoder::*;
pub use packet::*;
pub use property_id::*;
pub use validation::*;
