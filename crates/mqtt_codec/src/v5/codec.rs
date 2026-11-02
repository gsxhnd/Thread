//! MQTT v5.0 codec combining encoder and decoder.
//!
//! This module provides a combined codec for MQTT v5.0 packets,
//! suitable for use with asynchronous I/O frameworks like `tokio-util`.

use crate::{Decoder, Encoder};
use bytes::BytesMut;

use super::decoder::MqttDecoder;
use super::encoder::MqttEncoder;
use crate::MqttError;
use crate::v5::packet::Packet;

// ============================================================================
// Codec
// ============================================================================

/// MQTT v5.0 codec that combines encoder and decoder.
///
/// This codec can be used with tokio's `Framed` for async I/O.
/// It provides high-level packet encoding and decoding for the MQTT v5.0 protocol.
///
/// # Example
///
/// ```rust,ignore
/// use mqtt_codec::v5::MqttCodec;
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
pub struct MqttCodec {
    /// Maximum packet size in bytes (0 means no limit).
    /// This is negotiated during the CONNECT/CONNACK exchange.
    max_packet_size: u32,
}

impl MqttCodec {
    /// Creates a new MQTT v5.0 codec with no packet size limit.
    ///
    /// # Returns
    ///
    /// A new `MqttCodec` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new MQTT v5.0 codec with a maximum packet size limit.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - The maximum packet size in bytes. 0 means no limit.
    ///
    /// # Returns
    ///
    /// A new `MqttCodec` instance with the specified limit.
    pub fn with_max_packet_size(max_packet_size: u32) -> Self {
        Self { max_packet_size }
    }

    /// Sets the maximum packet size.
    ///
    /// This should be called after receiving CONNACK to apply the server's
    /// maximum packet size, or after sending CONNACK to apply the client's limit.
    ///
    /// # Arguments
    ///
    /// * `max_packet_size` - The maximum packet size in bytes. 0 means no limit.
    pub fn set_max_packet_size(&mut self, max_packet_size: u32) {
        self.max_packet_size = max_packet_size;
    }

    /// Returns the current maximum packet size setting.
    pub fn max_packet_size(&self) -> u32 {
        self.max_packet_size
    }
}

impl Decoder for MqttCodec {
    /// The type of items returned by decoding.
    type Item = Packet;
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Attempts to decode an MQTT v5.0 packet from the provided buffer.
    ///
    /// If a maximum packet size is configured, packets exceeding that size
    /// will be rejected with a `FrameTooLarge` error.
    ///
    /// # Arguments
    ///
    /// * `src` - A mutable reference to the bytes buffer to decode from.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Packet))` - A complete MQTT v5.0 packet was successfully decoded.
    /// * `Ok(None)` - Not enough data is available to decode a complete packet.
    /// * `Err(MqttError)` - An unrecoverable error occurred during decoding.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if the incoming bytes violate the MQTT v5.0 protocol
    /// or if the packet exceeds the configured maximum packet size.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Check packet size before full decode if we have enough data
        if self.max_packet_size > 0 && src.len() >= 2 {
            // Peek at remaining length to check total size
            let (remaining_length, header_size) = peek_remaining_length(&src[1..]);
            if header_size > 0 {
                let total_length = 1 + header_size + remaining_length;
                if total_length > self.max_packet_size as usize {
                    return Err(MqttError::packet_too_large(
                        total_length,
                        self.max_packet_size as usize,
                    ));
                }
            }
        }

        let mut decoder = MqttDecoder;
        decoder.decode(src)
    }
}

impl Encoder<Packet> for MqttCodec {
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Encodes an MQTT v5.0 packet into the provided buffer.
    ///
    /// If a maximum packet size is configured, the encoded packet size
    /// will be checked against the limit.
    ///
    /// # Arguments
    ///
    /// * `item` - The `Packet` to encode.
    /// * `dst` - A mutable reference to the bytes buffer where encoded data is written.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The packet was successfully encoded.
    /// * `Err(MqttError)` - An error occurred during encoding or the packet exceeds size limit.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if the packet fails validation before encoding
    /// or if the encoded packet exceeds the configured maximum packet size.
    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let start_len = dst.len();
        let mut encoder = MqttEncoder;
        encoder.encode(item, dst)?;

        if self.max_packet_size > 0 {
            let encoded_size = dst.len() - start_len;
            if encoded_size > self.max_packet_size as usize {
                // Remove the encoded data since it exceeds the limit
                dst.truncate(start_len);
                return Err(MqttError::packet_too_large(
                    encoded_size,
                    self.max_packet_size as usize,
                ));
            }
        }

        Ok(())
    }
}

/// Peek at the remaining length without consuming the buffer.
/// Returns (remaining_length, header_size) or (0, 0) if incomplete.
fn peek_remaining_length(buf: &[u8]) -> (usize, usize) {
    let mut multiplier = 1;
    let mut remaining_length = 0;
    let mut idx = 0;

    loop {
        if idx >= buf.len() {
            return (0, 0); // Incomplete
        }

        let encoded_byte = buf[idx];
        remaining_length += (encoded_byte & 0x7F) as usize * multiplier;
        idx += 1;

        if multiplier > 128 * 128 * 128 {
            return (0, 0); // Invalid
        }

        if (encoded_byte & 0x80) == 0 {
            return (remaining_length, idx);
        }

        multiplier *= 128;
    }
}
