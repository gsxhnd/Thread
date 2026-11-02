//! MQTT v5.0 packet decoder.
//!
//! This module provides decoding functionality for MQTT v5.0 packets,
//! handling the conversion from raw bytes into structured `Packet` variants.

use crate::Decoder;
use crate::MqttError;
use crate::v5::packet::*;
use crate::v5::properties_codec::parse_required_properties;
use crate::v5::validation::{validate_fixed_header_flags, validate_packet};
use bytes::{Buf, Bytes, BytesMut};

/// Constants for MQTT 5.0 protocol.
const MIN_PACKET_SIZE: usize = 2;
const MAX_REMAINING_LENGTH_MULTIPLIER: usize = 128 * 128 * 128;
const CONTINUATION_BIT: u8 = 0x80;
const LENGTH_MASK: u8 = 0x7F;

/// MQTT v5.0 packet decoder.
///
/// Implements the [`Decoder`] trait to parse MQTT v5.0 control packets
/// from a byte stream.
#[derive(Debug, Default)]
pub struct MqttDecoder;

impl Decoder for MqttDecoder {
    /// The type of items returned by decoding.
    type Item = Packet;
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Attempts to decode an MQTT v5.0 packet from the provided buffer.
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

        // Validate packet type (1-15 for MQTT 5.0)
        if packet_type == 0 || packet_type > 15 {
            return Err(MqttError::malformed(format!(
                "Invalid packet type: {}",
                packet_type
            )));
        }

        let packet_type_enum = PacketType::from_u8(packet_type)
            .ok_or_else(|| MqttError::malformed(format!("Invalid packet type: {}", packet_type)))?;
        validate_fixed_header_flags(packet_type_enum, flags)?;

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
            1 => parse_connect_packet(first_byte, packet_buf)?,
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
            14 => parse_disconnect_packet(packet_buf)?,
            15 => parse_auth_packet(packet_buf)?,
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

fn parse_reason_tail_properties(
    _reason_code: ReasonCode,
    buf: &mut &[u8],
    _packet_name: &str,
) -> Result<Properties, MqttError> {
    if buf.is_empty() {
        return Ok(Properties::new());
    }
    parse_required_properties(buf)
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

// ============================================================================
// Packet parsing functions
// ============================================================================

/// Parse a CONNECT packet from the given buffer.
fn parse_connect_packet(_first_byte: u8, mut buf: &[u8]) -> Result<Packet, MqttError> {
    let protocol_name = parse_utf8_string(&mut buf)?;

    if buf.len() < 4 {
        return Err(MqttError::incomplete(4, buf.len()));
    }

    let protocol_level = buf.get_u8();
    let connect_flags = buf.get_u8();

    if (connect_flags & 0x01) != 0 {
        return Err(MqttError::protocol_violation(
            "CONNECT reserved bit must be 0",
            Some(1),
        ));
    }

    let clean_start = (connect_flags & 0x02) != 0;
    let will_flag = (connect_flags & 0x04) != 0;
    let will_qos = QoS::try_from((connect_flags & 0x18) >> 3)
        .map_err(|e| MqttError::protocol_violation(format!("Invalid Will QoS: {}", e), Some(1)))?;
    let will_retain = (connect_flags & 0x20) != 0;
    let password_flag = (connect_flags & 0x40) != 0;
    let username_flag = (connect_flags & 0x80) != 0;

    let keep_alive = buf.get_u16();
    let properties = parse_required_properties(&mut buf)?;

    let client_id = parse_utf8_string(&mut buf)?;

    let (will_topic, will_message, will_properties) = if will_flag {
        let will_props = parse_required_properties(&mut buf)?;
        let topic = parse_utf8_string(&mut buf)?;
        let len = buf.get_u16() as usize;
        if buf.len() < len {
            return Err(MqttError::incomplete(len, buf.len()));
        }
        let message = Bytes::copy_from_slice(&buf[..len]);
        buf = &buf[len..];
        (Some(topic), Some(message), Some(will_props))
    } else {
        (None, None, None)
    };

    let username = if username_flag {
        Some(parse_utf8_string(&mut buf)?)
    } else {
        None
    };

    let password = if password_flag {
        let len = buf.get_u16() as usize;
        if buf.len() < len {
            return Err(MqttError::incomplete(len, buf.len()));
        }
        let password = Bytes::copy_from_slice(&buf[..len]);
        buf = &buf[len..];
        Some(password)
    } else {
        None
    };
    ensure_no_remaining(buf, "CONNECT", 1)?;

    Ok(Packet::Connect(ConnectPacket {
        protocol_name,
        protocol_level,
        clean_start,
        will_flag,
        will_qos,
        will_retain,
        password_flag,
        username_flag,
        keep_alive,
        properties,
        client_id,
        will_topic,
        will_message,
        will_properties,
        username,
        password,
    }))
}

/// Parse a CONNACK packet from the given buffer.
fn parse_connack_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let flags = buf.get_u8();
    if (flags & 0xFE) != 0 {
        return Err(MqttError::protocol_violation(
            "CONNACK reserved bits must be 0",
            Some(2),
        ));
    }
    let session_present = (flags & 0x01) != 0;
    let reason_code = ReasonCode::try_from(buf.get_u8())
        .map_err(|code| MqttError::invalid_reason_code(code, "CONNACK"))?;
    let properties = parse_required_properties(&mut buf)?;
    ensure_no_remaining(buf, "CONNACK", 2)?;

    Ok(Packet::ConnAck(ConnAckPacket {
        session_present,
        reason_code,
        properties,
    }))
}

/// Parse a PUBLISH packet from the given buffer.
fn parse_publish_packet(first_byte: u8, mut buf: &[u8]) -> Result<Packet, MqttError> {
    let duplicate = (first_byte & 0x08) != 0;
    let qos = QoS::try_from((first_byte & 0x06) >> 1).map_err(|e| {
        MqttError::protocol_violation(format!("Invalid PUBLISH QoS: {}", e), Some(3))
    })?;
    let retain = (first_byte & 0x01) != 0;

    let topic_name = parse_utf8_string(&mut buf)?;

    let packet_id = if matches!(qos, QoS::AtLeastOnce | QoS::ExactlyOnce) {
        if buf.len() < 2 {
            return Err(MqttError::incomplete(2, buf.len()));
        }
        Some(buf.get_u16())
    } else {
        None
    };

    let properties = parse_required_properties(&mut buf)?;
    let payload = Bytes::copy_from_slice(buf);

    Ok(Packet::Publish(PublishPacket {
        topic_name,
        packet_id,
        payload,
        qos,
        duplicate,
        retain,
        properties,
    }))
}

/// Parse a PUBACK packet from the given buffer.
fn parse_puback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "PUBACK"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "PUBACK")?
    };

    Ok(Packet::PubAck(PubAckPacket {
        packet_id,
        reason_code,
        properties,
    }))
}

/// Parse a PUBREC packet from the given buffer.
fn parse_pubrec_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "PUBREC"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "PUBREC")?
    };

    Ok(Packet::PubRec(PubRecPacket {
        packet_id,
        reason_code,
        properties,
    }))
}

/// Parse a PUBREL packet from the given buffer.
fn parse_pubrel_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "PUBREL"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "PUBREL")?
    };

    Ok(Packet::PubRel(PubRelPacket {
        packet_id,
        reason_code,
        properties,
    }))
}

/// Parse a PUBCOMP packet from the given buffer.
fn parse_pubcomp_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "PUBCOMP"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "PUBCOMP")?
    };

    Ok(Packet::PubComp(PubCompPacket {
        packet_id,
        reason_code,
        properties,
    }))
}

/// Parse a SUBSCRIBE packet from the given buffer.
fn parse_subscribe_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let properties = parse_required_properties(&mut buf)?;

    let mut topics = Vec::new();
    while !buf.is_empty() {
        let topic_filter = parse_utf8_string(&mut buf)?;
        if buf.is_empty() {
            return Err(MqttError::incomplete(1, 0));
        }
        let options_byte = buf.get_u8();
        if (options_byte & 0xC0) != 0 {
            return Err(MqttError::protocol_violation(
                "SUBSCRIBE subscription options reserved bits must be 0",
                Some(8),
            ));
        }
        let qos = QoS::try_from(options_byte & 0x03).map_err(|e| {
            MqttError::protocol_violation(format!("Invalid QoS in SUBSCRIBE: {}", e), Some(8))
        })?;
        let no_local = (options_byte & 0x04) != 0;
        let retain_as_published = (options_byte & 0x08) != 0;
        let retain_handling = (options_byte & 0x30) >> 4;

        topics.push(SubscriptionOption {
            topic_filter,
            qos,
            no_local,
            retain_as_published,
            retain_handling,
        });
    }

    Ok(Packet::Subscribe(SubscribePacket {
        packet_id,
        properties,
        topics,
    }))
}

/// Parse a SUBACK packet from the given buffer.
fn parse_suback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let properties = parse_required_properties(&mut buf)?;

    let mut reason_codes = Vec::new();
    while !buf.is_empty() {
        reason_codes.push(
            ReasonCode::try_from(buf.get_u8())
                .map_err(|code| MqttError::invalid_reason_code(code, "SUBACK"))?,
        );
    }

    Ok(Packet::SubAck(SubAckPacket {
        packet_id,
        properties,
        reason_codes,
    }))
}

/// Parse an UNSUBSCRIBE packet from the given buffer.
fn parse_unsubscribe_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let properties = parse_required_properties(&mut buf)?;

    let mut topics = Vec::new();
    while !buf.is_empty() {
        topics.push(parse_utf8_string(&mut buf)?);
    }

    Ok(Packet::Unsubscribe(UnsubscribePacket {
        packet_id,
        properties,
        topics,
    }))
}

/// Parse an UNSUBACK packet from the given buffer.
fn parse_unsuback_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }

    let packet_id = buf.get_u16();
    let properties = parse_required_properties(&mut buf)?;

    let mut reason_codes = Vec::new();
    while !buf.is_empty() {
        reason_codes.push(
            ReasonCode::try_from(buf.get_u8())
                .map_err(|code| MqttError::invalid_reason_code(code, "UNSUBACK"))?,
        );
    }

    Ok(Packet::UnsubAck(UnsubAckPacket {
        packet_id,
        properties,
        reason_codes,
    }))
}

/// Parse a DISCONNECT packet from the given buffer.
fn parse_disconnect_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "DISCONNECT"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "DISCONNECT")?
    };

    Ok(Packet::Disconnect(DisconnectPacket {
        reason_code,
        properties,
    }))
}

/// Parse an AUTH packet from the given buffer.
fn parse_auth_packet(mut buf: &[u8]) -> Result<Packet, MqttError> {
    let reason_code = if !buf.is_empty() {
        ReasonCode::try_from(buf.get_u8())
            .map_err(|code| MqttError::invalid_reason_code(code, "AUTH"))?
    } else {
        ReasonCode::Success
    };
    let properties = if buf.is_empty() {
        Properties::new()
    } else {
        parse_reason_tail_properties(reason_code, &mut buf, "AUTH")?
    };

    Ok(Packet::Auth(AuthPacket {
        reason_code,
        properties,
    }))
}
