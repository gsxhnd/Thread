//! MQTT Codec Library
//!
//! This library provides encoding and decoding functionality for MQTT protocol packets.
//! It supports both MQTT v3.1.1 (MQTT 4) and MQTT v5.0 protocols.
//!
//! # Features
//!
//! - MQTT v3.1.1 and v5.0 control packet encode/decode with spec validation
//! - Comprehensive validation logic
//! - Builder pattern for complex packet types
//! - Detailed error handling with `thiserror`
//!
//! # Example
//!
//! ```rust,ignore
//! use mqtt_codec::{v4, Encoder, Decoder};
//! use bytes::BytesMut;
//!
//! // Create a CONNECT packet
//! let connect = v4::ConnectPacket {
//!     protocol_name: "MQTT".to_string(),
//!     protocol_level: 4,
//!     clean_session: true,
//!     keep_alive: 60,
//!     client_id: "test-client".to_string(),
//!     ..Default::default()
//! };
//!
//! // Encode the packet
//! let mut encoder = v4::MqttCodec::new();
//! let mut buffer = BytesMut::new();
//! encoder.encode(v4::Packet::Connect(connect), &mut buffer).unwrap();
//! ```

use bytes::BytesMut;

pub mod error;
pub mod v4;
pub mod v5;

// Re-export commonly used types
pub use error::{ClientIdErrorReason, MqttError, TopicErrorReason, TopicFilterErrorReason};

/// Generic trait for decoding items from bytes.
pub trait Decoder {
    /// The type of items returned by decoding.
    type Item;
    /// The type of unrecoverable frame errors.
    type Error;

    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// This method is called by `Framed` whenever bytes are available for decoding.
    /// The default implementation does nothing.
    ///
    /// # Arguments
    ///
    /// * `src` - A mutable reference to the bytes buffer to decode from.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(frame))` - A frame was successfully decoded.
    /// * `Ok(None)` - Not enough data is available to decode a complete frame.
    /// * `Err(e)` - An unrecoverable error occurred during decoding.
    ///
    /// # Errors
    ///
    /// Returns an error of type `Self::Error` if the data in `src` violates the protocol
    /// or is otherwise malformed.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;
}

/// Generic trait for encoding items into bytes.
pub trait Encoder<T> {
    /// The type of unrecoverable frame errors.
    type Error;

    /// Encodes a frame into the buffer provided.
    ///
    /// This method is called by `Framed` whenever data needs to be sent.
    ///
    /// # Arguments
    ///
    /// * `item` - The item to encode into the buffer.
    /// * `dst` - A mutable reference to the bytes buffer where encoded data is written.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The item was successfully encoded into the buffer.
    /// * `Err(e)` - An unrecoverable error occurred during encoding.
    ///
    /// # Errors
    ///
    /// Returns an error of type `Self::Error` if the `item` cannot be encoded,
    /// for example, if it contains invalid values that violate protocol constraints.
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error>;
}
