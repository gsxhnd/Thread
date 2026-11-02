//! MQTT v3.1.1 packet decoder.
//!
//! This module provides decoding functionality for MQTT v3.1.1 packets,
//! handling the conversion from raw bytes into structured `Packet` variants.

use crate::Decoder;
use crate::MqttError;
use crate::v4::packet::*;
use crate::v4::return_codes::{ConnectReturnCode, SubAckReturnCode};
use crate::v4::validation::{
    validate_connack_packet, validate_connect_packet, validate_fixed_header_flags, validate_packet,
    validate_packet_id, validate_topic_filter, validate_topic_name,
};
use bytes::{Buf, Bytes, BytesMut};

/// Constants for MQTT protocol.
const MIN_PACKET_SIZE: usize = 2;
const MAX_REMAINING_LENGTH_MULTIPLIER: usize = 128 * 128 * 128;
const CONTINUATION_BIT: u8 = 0x80;
const LENGTH_MASK: u8 = 0x7F;

/// MQTT v3.1.1 packet decoder.
///
/// Implements the [`Decoder`] trait to parse MQTT v3.1.1 control packets
/// from a byte stream.
#[derive(Debug, Default)]
pub struct MqttDecoder;

impl Decoder for MqttDecoder {
    /// The type of items returned by decoding.
    type Item = Packet;
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Attempts to decode an MQTT v3.1.1 packet from the provided buffer.
    ///
    /// This method parses the fixed header, calculates the remaining length,
    /// and then delegates to specific parsing functions based on the packet type.
    ///
    /// # Arguments
    ///
    /// * `src` - A mutable reference to the bytes buffer to decode from.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Packet))` - A complete packet was successfully decoded.
    /// * `Ok(None)` - Not enough data is available to decode a complete packet.
    /// * `Err(MqttError)` - An error occurred during decoding.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if:
    /// - The packet type is invalid.
    /// - The fixed header flags are invalid for the packet type.
    /// - The remaining length encoding is malformed.
    /// - The packet payload violates protocol constraints.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < MIN_PACKET_SIZE {
            return Ok(None);
        }

        // Parse fixed header
        let first_byte = src[0];
        let packet_type = first_byte >> 4;
        let flags = first_byte & 0x0F;

        // Validate packet type (1-14, but not 0 or 15)
        if packet_type == 0 || packet_type > 14 {
            return Err(MqttError::malformed(format!(
                "Invalid packet type: {}",
                packet_type
            )));
        }

        // Validate fixed header flags
        validate_fixed_header_flags(packet_type, flags)?;

        // Parse remaining length
        let (remaining_length, header_size) = parse_remaining_length(&src[1..])?;
        if header_size == 0 {
            return Ok(None);
        }

        let total_length = 1 + header_size + remaining_length;

        if src.len() < total_length {
            return Ok(None);
        }

        // Extract the packet data
        let packet_data = src.split_to(total_length);
        let packet_buf = &packet_data[1 + header_size..];

        // Parse packet based on type
        let packet = match packet_type {
            1 => parse_connect_packet(packet_buf)?,
            2 => parse_connack_packet(packet_buf)?,
            3 => parse_publish_packet(first_byte, packet_buf)?,
            4 => parse_puback_packet(packet_buf)?,
            5 => parse_pubrec_packet(packet_buf)?,
            6 => parse_pubrel_packet(packet_buf)?,
            7 => parse_pubcomp_packet(packet_buf)?,
            8 => parse_subscribe_packet(packet_buf)?,
            9 => parse_suback_packet(packet_buf)?,
            10 => parse_unsubscribe_packet(packet_buf)?,
            11 => parse_unsuback_packet(packet_buf)?,
            12 => {
                ensure_empty_body(packet_buf, "PINGREQ", 12)?;
                Packet::PingReq(PingReqPacket)
            }
            13 => {
                ensure_empty_body(packet_buf, "PINGRESP", 13)?;
                Packet::PingResp(PingRespPacket)
            }
            14 => {
                ensure_empty_body(packet_buf, "DISCONNECT", 14)?;
                Packet::Disconnect(DisconnectPacket)
            }
            _ => unreachable!(),
        };

        validate_packet(&packet)?;
        Ok(Some(packet))
    }
}

/// Parse remaining length from MQTT variable length encoding.
///
/// # Arguments
///
/// * `buf` - The slice containing the variable length encoded integer.
///
/// # Returns
///
/// * `Ok((length, header_size_in_bytes))` - The decoded length and the number of bytes consumed.
/// * `Err(MqttError)` - If the encoding is invalid or exceeds protocol limits.
fn parse_remaining_length(buf: &[u8]) -> Result<(usize, usize), MqttError> {
    let mut multiplier = 1;
    let mut remaining_length = 0;
    let mut idx = 0;

    loop {
        if idx >= buf.len() {
            return Ok((0, 0)); // Incomplete, need more data
        }

        let encoded_byte = buf[idx];
        remaining_length += (encoded_byte & LENGTH_MASK) as usize * multiplier;
        idx += 1;

        if multiplier > MAX_REMAINING_LENGTH_MULTIPLIER {
            return Err(MqttError::InvalidRemainingLength {
                length: remaining_length,
            });
        }

        if (encoded_byte & CONTINUATION_BIT) == 0 {
            if idx != variable_length_byte_count(remaining_length) {
                return Err(MqttError::InvalidRemainingLength {
                    length: remaining_length,
                });
            }
            return Ok((remaining_length, idx));
        }

        multiplier *= 128;
    }
}

fn variable_length_byte_count(value: usize) -> usize {
    match value {
        0..=127 => 1,
        128..=16_383 => 2,
        16_384..=2_097_151 => 3,
        _ => 4,
    }
}

// ============================================================================
// Helper functions for decoding
// ============================================================================

fn ensure_empty_body(buf: &[u8], packet_name: &str, packet_type: u8) -> Result<(), MqttError> {
    if !buf.is_empty() {
        return Err(MqttError::protocol_violation(
            format!("{packet_name} remaining length must be 0"),
            Some(packet_type),
        ));
    }
    Ok(())
}

fn ensure_no_remaining(buf: &[u8], packet_name: &str, packet_type: u8) -> Result<(), MqttError> {
    if !buf.is_empty() {
        return Err(MqttError::protocol_violation(
            format!("{packet_name} contains {} trailing byte(s)", buf.len()),
            Some(packet_type),
        ));
    }
    Ok(())
}

/// Parse a UTF-8 string from the buffer.
///
/// The string must be prefixed with a 2-byte length field.
///
/// # Arguments
///
/// * `buf` - A mutable reference to the slice to parse from.
///
/// # Returns
///
/// * `Ok(String)` - The decoded string.
/// * `Err(MqttError)` - If the buffer is too short or the string is not valid UTF-8.
fn parse_utf8_string(buf: &mut &[u8]) -> Result<String, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }
    let len = buf.get_u16() as usize;
    if buf.len() < len {
        return Err(MqttError::incomplete(len, buf.len()));
    }
    let s = std::str::from_utf8(&buf[..len])?.to_string();
    *buf = &buf[len..];
    Ok(s)
}

/// Parse binary data from the buffer.
///
/// The data must be prefixed with a 2-byte length field.
///
/// # Arguments
///
/// * `buf` - A mutable reference to the slice to parse from.
///
/// # Returns
///
/// * `Ok(Bytes)` - The decoded binary data.
/// * `Err(MqttError)` - If the buffer is too short.
fn parse_binary_data(buf: &mut &[u8]) -> Result<Bytes, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }
    let len = buf.get_u16() as usize;
    if buf.len() < len {
        return Err(MqttError::incomplete(len, buf.len()));
    }
    let data = Bytes::copy_from_slice(&buf[..len]);
    *buf = &buf[len..];
    Ok(data)
}

// ============================================================================
// Packet parsing functions
// ============================================================================

/// Parse a CONNECT packet from the given buffer.
///
/// # Arguments
///
/// * `buf` - The buffer containing the CONNECT packet payload.
///
/// # Returns
///
/// * `Ok(Packet::Connect)` - The successfully parsed CONNECT packet.
/// * `Err(MqttError)` - If the packet data is malformed or violates protocol rules.
fn parse_connect_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    // Protocol name
    let protocol_name = parse_utf8_string(&mut buf)?;

    if buf.len() < 1 + 1 + 2 {
        return Err(MqttError::incomplete(4, buf.len()));
    }

    // Protocol level
    let protocol_level = buf.get_u8();

    // Connect flags
    let connect_flags = buf.get_u8();

    // Validate reserved bit (bit 0 must be 0)
    if (connect_flags & 0x01) != 0 {
        return Err(MqttError::protocol_violation(
            "CONNECT reserved bit must be 0",
            Some(1),
        ));
    }

    let username_flag = (connect_flags & 0x80) != 0;
    let password_flag = (connect_flags & 0x40) != 0;
    let will_retain = (connect_flags & 0x20) != 0;
    let will_qos = QoS::try_from((connect_flags & 0x18) >> 3)
        .map_err(|e| MqttError::protocol_violation(format!("Invalid Will QoS: {}", e), Some(1)))?;
    let will_flag = (connect_flags & 0x04) != 0;
    let clean_session = (connect_flags & 0x02) != 0;

    // Keep alive
    let keep_alive = buf.get_u16();

    // Payload - Client ID
    let client_id = parse_utf8_string(&mut buf)?;

    // Will Topic and Will Message
    let will_topic = if will_flag {
        let topic = parse_utf8_string(&mut buf)?;
        // Validate will topic as a topic name (no wildcards)
        validate_topic_name(&topic)?;
        Some(topic)
    } else {
        None
    };

    let will_message = if will_flag {
        Some(parse_binary_data(&mut buf)?)
    } else {
        None
    };

    // Username
    let username = if username_flag {
        Some(parse_utf8_string(&mut buf)?)
    } else {
        None
    };

    // Password
    let password = if password_flag {
        Some(parse_binary_data(&mut buf)?)
    } else {
        None
    };
    ensure_no_remaining(buf, "CONNECT", 1)?;

    let packet = ConnectPacket {
        protocol_name,
        protocol_level,
        clean_session,
        will_flag,
        will_qos,
        will_retain,
        password_flag,
        username_flag,
        keep_alive,
        client_id,
        will_topic,
        will_message,
        username,
        password,
    };

    // Validate the packet
    validate_connect_packet(&packet)?;

    Ok(Packet::Connect(packet))
}

/// Parse a CONNACK packet from the given buffer.
///
/// # Arguments
///
/// * `buf` - The buffer containing the CONNACK packet payload.
///
/// # Returns
///
/// * `Ok(Packet::ConnAck)` - The successfully parsed CONNACK packet.
/// * `Err(MqttError)` - If the packet data is malformed.
fn parse_connack_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let connect_flags = buf.get_u8();

    // Validate reserved bits (bits 7-1 must be 0)
    if (connect_flags & 0xFE) != 0 {
        return Err(MqttError::protocol_violation(
            "CONNACK reserved bits must be 0",
            Some(2),
        ));
    }

    let session_present = (connect_flags & 0x01) != 0;

    let return_code_raw = buf.get_u8();

    // Validate and convert return code
    let return_code =
        ConnectReturnCode::try_from(return_code_raw).map_err(MqttError::invalid_return_code)?;

    let packet = ConnAckPacket {
        session_present,
        return_code,
    };
    validate_connack_packet(&packet)?;
    ensure_no_remaining(buf, "CONNACK", 2)?;

    Ok(Packet::ConnAck(packet))
}

/// Parse a PUBLISH packet from the given buffer.
///
/// # Arguments
///
/// * `first_byte` - The first byte of the packet containing flags.
/// * `buf` - The buffer containing the PUBLISH packet payload.
///
/// # Returns
///
/// * `Ok(Packet::Publish)` - The successfully parsed PUBLISH packet.
/// * `Err(MqttError)` - If the packet data is malformed.
fn parse_publish_packet(first_byte: u8, mut buf: &[u8]) -> Result<Packet, MqttError> {
    let duplicate = (first_byte & 0x08) != 0;
    let qos = QoS::try_from((first_byte & 0x06) >> 1).map_err(|e| {
        MqttError::protocol_violation(format!("Invalid PUBLISH QoS: {}", e), Some(3))
    })?;
    let retain = (first_byte & 0x01) != 0;

    // Topic name
    let topic_name = parse_utf8_string(&mut buf)?;

    // Validate topic name (no wildcards allowed in publish)
    validate_topic_name(&topic_name)?;

    // Packet ID (only for QoS > 0)
    let packet_id = if matches!(qos, QoS::AtLeastOnce | QoS::ExactlyOnce) {
        if buf.len() < 2 {
            return Err(MqttError::incomplete(2, buf.len()));
        }
        let id = buf.get_u16();
        validate_packet_id(id)?;
        Some(id)
    } else {
        // QoS 0 should not have packet ID
        // Also validate that duplicate flag is not set for QoS 0
        if duplicate {
            return Err(MqttError::protocol_violation(
                "DUP flag should not be set for QoS 0 messages",
                Some(3),
            ));
        }
        None
    };

    // Payload - remaining bytes
    let payload = Bytes::copy_from_slice(buf);

    Ok(Packet::Publish(PublishPacket {
        topic_name,
        packet_id,
        payload,
        qos,
        duplicate,
        retain,
    }))
}

/// Parse a PUBACK packet from the given buffer.
fn parse_puback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;
    ensure_no_remaining(buf, "PUBACK", 4)?;

    Ok(Packet::PubAck(PubAckPacket { packet_id }))
}

/// Parse a PUBREC packet from the given buffer.
fn parse_pubrec_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;
    ensure_no_remaining(buf, "PUBREC", 5)?;

    Ok(Packet::PubRec(PubRecPacket { packet_id }))
}

/// Parse a PUBREL packet from the given buffer.
fn parse_pubrel_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;
    ensure_no_remaining(buf, "PUBREL", 6)?;

    Ok(Packet::PubRel(PubRelPacket { packet_id }))
}

/// Parse a PUBCOMP packet from the given buffer.
fn parse_pubcomp_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;
    ensure_no_remaining(buf, "PUBCOMP", 7)?;

    Ok(Packet::PubComp(PubCompPacket { packet_id }))
}

/// Parse a SUBSCRIBE packet from the given buffer.
///
/// # Arguments
///
/// * `buf` - The buffer containing the SUBSCRIBE packet payload.
///
/// # Returns
///
/// * `Ok(Packet::Subscribe)` - The successfully parsed SUBSCRIBE packet.
/// * `Err(MqttError)` - If the packet data is malformed.
fn parse_subscribe_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;

    let mut topics = Vec::new();

    while !buf.is_empty() {
        let topic_filter = parse_utf8_string(&mut buf)?;

        // Validate topic filter
        validate_topic_filter(&topic_filter)?;

        if buf.is_empty() {
            return Err(MqttError::incomplete(1, 0));
        }

        let requested_qos = buf.get_u8();
        let qos = QoS::try_from(requested_qos).map_err(|e| {
            MqttError::protocol_violation(format!("Invalid QoS in SUBSCRIBE: {}", e), Some(8))
        })?;

        topics.push((topic_filter, qos));
    }

    // SUBSCRIBE must have at least one topic
    if topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBSCRIBE must contain at least one topic filter",
            Some(8),
        ));
    }

    Ok(Packet::Subscribe(SubscribePacket { packet_id, topics }))
}

/// Parse a SUBACK packet from the given buffer.
///
/// # Arguments
///
/// * `buf` - The buffer containing the SUBACK packet payload.
///
/// # Returns
///
/// * `Ok(Packet::SubAck)` - The successfully parsed SUBACK packet.
/// * `Err(MqttError)` - If the packet data is malformed.
fn parse_suback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;

    let mut return_codes = Vec::new();

    while !buf.is_empty() {
        let code = buf.get_u8();

        // Validate return code
        let return_code =
            SubAckReturnCode::try_from(code).map_err(|_| MqttError::invalid_return_code(code))?;

        return_codes.push(return_code);
    }

    // SUBACK must have at least one return code
    if return_codes.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBACK must contain at least one return code",
            Some(9),
        ));
    }

    Ok(Packet::SubAck(SubAckPacket {
        packet_id,
        return_codes,
    }))
}

/// Parse an UNSUBSCRIBE packet from the given buffer.
///
/// # Arguments
///
/// * `buf` - The buffer containing the UNSUBSCRIBE packet payload.
///
/// # Returns
///
/// * `Ok(Packet::Unsubscribe)` - The successfully parsed UNSUBSCRIBE packet.
/// * `Err(MqttError)` - If the packet data is malformed.
fn parse_unsubscribe_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;

    let mut topics = Vec::new();

    while !buf.is_empty() {
        let topic_filter = parse_utf8_string(&mut buf)?;

        // Validate topic filter
        validate_topic_filter(&topic_filter)?;

        topics.push(topic_filter);
    }

    // UNSUBSCRIBE must have at least one topic
    if topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "UNSUBSCRIBE must contain at least one topic filter",
            Some(10),
        ));
    }

    Ok(Packet::Unsubscribe(UnsubscribePacket { packet_id, topics }))
}

/// Parse an UNSUBACK packet from the given buffer.
fn parse_unsuback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    validate_packet_id(packet_id)?;
    ensure_no_remaining(buf, "UNSUBACK", 11)?;

    Ok(Packet::UnsubAck(UnsubAckPacket { packet_id }))
}
