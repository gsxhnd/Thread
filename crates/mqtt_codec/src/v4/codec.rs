//! MQTT v3.1.1 codec combining encoder and decoder.
//!
//! This module provides a combined codec for MQTT v3.1.1 packets,
//! suitable for use with asynchronous I/O frameworks like `tokio-util`.

use crate::{Decoder, Encoder};
use bytes::BytesMut;

use super::decoder::MqttDecoder;
use super::encoder::MqttEncoder;
use crate::MqttError;
use crate::v4::packet::Packet;

// ============================================================================
// Codec
// ============================================================================

/// MQTT v3.1.1 codec that combines encoder and decoder.
///
/// This codec can be used with tokio's `Framed` for async I/O.
/// It provides high-level packet encoding and decoding for the MQTT v3.1.1 protocol.
///
/// # Example
///
/// ```rust,ignore
/// use mqtt_codec::v4::MqttCodec;
/// use mqtt_codec::{Encoder, Decoder};
/// use bytes::BytesMut;
///
/// let mut codec = MqttCodec::new();
/// let mut buffer = BytesMut::new();
///
/// // Encode a packet
/// // codec.encode(packet, &mut buffer).unwrap();
///
/// // Decode a packet
/// // let packet = codec.decode(&mut buffer).unwrap();
/// ```
#[derive(Debug, Default)]
pub struct MqttCodec;

impl MqttCodec {
    /// Creates a new MQTT v3.1.1 codec.
    ///
    /// # Returns
    ///
    /// A new `MqttCodec` instance.
    pub fn new() -> Self {
        MqttCodec
    }
}

impl Decoder for MqttCodec {
    /// The type of items returned by decoding.
    type Item = Packet;
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// # Arguments
    ///
    /// * `src` - A mutable reference to the bytes buffer to decode from.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Packet))` - A complete MQTT v3.1.1 packet was successfully decoded.
    /// * `Ok(None)` - Not enough data is available to decode a complete packet.
    /// * `Err(MqttError)` - An unrecoverable error occurred during decoding.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if the incoming bytes violate the MQTT v3.1.1 protocol.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut decoder = MqttDecoder;
        decoder.decode(src)
    }
}

impl Encoder<Packet> for MqttCodec {
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Encodes an MQTT v3.1.1 packet into the provided buffer.
    ///
    /// # Arguments
    ///
    /// * `item` - The `Packet` to encode.
    /// * `dst` - A mutable reference to the bytes buffer where encoded data is written.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The packet was successfully encoded.
    /// * `Err(MqttError)` - An error occurred during encoding.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if the packet fails validation before encoding.
    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut encoder = MqttEncoder;
        encoder.encode(item, dst)
    }
}
