//! MQTT v3.1.1 packet encoder.
//!
//! This module provides encoding functionality for MQTT v3.1.1 packets,
//! converting structured `Packet` variants into raw byte streams.

use crate::Encoder;
use crate::MqttError;
use crate::v4::packet::*;
use crate::v4::validation::validate_packet;
use bytes::{BufMut, BytesMut};

/// Constants for MQTT protocol.
const CONTINUATION_BIT: u8 = 0x80;

/// MQTT v3.1.1 packet encoder.
///
/// Implements the [`Encoder`] trait to serialize MQTT v3.1.1 control packets
/// into a byte buffer.
#[derive(Debug, Default)]
pub struct MqttEncoder;

impl Encoder<Packet> for MqttEncoder {
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Encodes an MQTT v3.1.1 packet into the provided buffer.
    ///
    /// This method validates the packet and then delegates to specific encoding
    /// functions based on the packet type.
    ///
    /// # Arguments
    ///
    /// * `item` - The `Packet` to encode.
    /// * `dst` - A mutable reference to the bytes buffer where encoded data is written.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The packet was successfully encoded.
    /// * `Err(MqttError)` - An error occurred during encoding or validation.
    ///
    /// # Errors
    ///
    /// Returns `MqttError` if the packet violates MQTT v3.1.1 protocol constraints.
    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        validate_packet(&item)?;
        match item {
            Packet::Connect(packet) => encode_connect_packet(packet, dst),
            Packet::ConnAck(packet) => encode_connack_packet(packet, dst),
            Packet::Publish(packet) => encode_publish_packet(packet, dst),
            Packet::PubAck(packet) => encode_puback_packet(packet, dst),
            Packet::PubRec(packet) => encode_pubrec_packet(packet, dst),
            Packet::PubRel(packet) => encode_pubrel_packet(packet, dst),
            Packet::PubComp(packet) => encode_pubcomp_packet(packet, dst),
            Packet::Subscribe(packet) => encode_subscribe_packet(packet, dst),
            Packet::SubAck(packet) => encode_suback_packet(packet, dst),
            Packet::Unsubscribe(packet) => encode_unsubscribe_packet(packet, dst),
            Packet::UnsubAck(packet) => encode_unsuback_packet(packet, dst),
            Packet::PingReq(_) => encode_pingreq_packet(dst),
            Packet::PingResp(_) => encode_pingresp_packet(dst),
            Packet::Disconnect(_) => encode_disconnect_packet(dst),
        }
    }
}

// ============================================================================
// Helper functions for encoding
// ============================================================================

/// Encode a UTF-8 string with 2-byte length prefix.
#[inline]
fn encode_utf8_string(s: &str, dst: &mut BytesMut) {
    dst.put_u16(s.len() as u16);
    dst.put_slice(s.as_bytes());
}

/// Encode binary data with 2-byte length prefix.
#[inline]
fn encode_binary_data(data: &[u8], dst: &mut BytesMut) {
    dst.put_u16(data.len() as u16);
    dst.put_slice(data);
}

/// Encode MQTT variable length integer.
fn encode_remaining_length(mut length: usize, dst: &mut BytesMut) {
    loop {
        let mut encoded_byte = (length % 128) as u8;
        length /= 128;
        if length > 0 {
            encoded_byte |= CONTINUATION_BIT;
        }
        dst.put_u8(encoded_byte);
        if length == 0 {
            break;
        }
    }
}

// ============================================================================
// Packet encoding functions
// ============================================================================

/// Encode a CONNECT packet.
fn encode_connect_packet(packet: ConnectPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();

    // Protocol name
    encode_utf8_string(&packet.protocol_name, &mut payload);

    // Protocol level
    payload.put_u8(packet.protocol_level);

    // Connect flags
    let mut connect_flags = 0u8;
    if packet.username_flag {
        connect_flags |= 0x80;
    }
    if packet.password_flag {
        connect_flags |= 0x40;
    }
    if packet.will_retain {
        connect_flags |= 0x20;
    }
    connect_flags |= (packet.will_qos as u8) << 3;
    if packet.will_flag {
        connect_flags |= 0x04;
    }
    if packet.clean_session {
        connect_flags |= 0x02;
    }
    // Bit 0 is reserved and must be 0
    payload.put_u8(connect_flags);

    // Keep alive
    payload.put_u16(packet.keep_alive);

    // Client ID
    encode_utf8_string(&packet.client_id, &mut payload);

    // Will Topic and Will Message
    if let Some(will_topic) = &packet.will_topic {
        encode_utf8_string(will_topic, &mut payload);
        if let Some(will_message) = &packet.will_message {
            encode_binary_data(will_message, &mut payload);
        }
    }

    // Username
    if let Some(username) = &packet.username {
        encode_utf8_string(username, &mut payload);
    }

    // Password
    if let Some(password) = &packet.password {
        encode_binary_data(password, &mut payload);
    }

    // Write fixed header
    dst.put_u8((PacketType::Connect as u8) << 4);
    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode a CONNACK packet.
fn encode_connack_packet(packet: ConnAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::with_capacity(2);

    // Connect acknowledge flags
    let mut connect_flags = 0u8;
    if packet.session_present {
        connect_flags |= 0x01;
    }
    // Bits 7-1 are reserved and must be 0
    payload.put_u8(connect_flags);

    // Return code
    payload.put_u8(packet.return_code.as_u8());

    // Write fixed header
    dst.put_u8((PacketType::ConnAck as u8) << 4);
    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode a PUBLISH packet.
fn encode_publish_packet(packet: PublishPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();

    // Topic name
    encode_utf8_string(&packet.topic_name, &mut payload);

    // Packet ID (only for QoS > 0)
    if let Some(packet_id) = packet.packet_id {
        payload.put_u16(packet_id);
    }

    // Payload
    payload.put_slice(&packet.payload);

    // Write fixed header
    let mut first_byte = (PacketType::Publish as u8) << 4;
    if packet.duplicate {
        first_byte |= 0x08;
    }
    first_byte |= (packet.qos as u8) << 1;
    if packet.retain {
        first_byte |= 0x01;
    }
    dst.put_u8(first_byte);

    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode a PUBACK packet.
fn encode_puback_packet(packet: PubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::PubAck as u8) << 4);
    dst.put_u8(2); // Remaining length

    // Write packet ID
    dst.put_u16(packet.packet_id);

    Ok(())
}

/// Encode a PUBREC packet.
fn encode_pubrec_packet(packet: PubRecPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::PubRec as u8) << 4);
    dst.put_u8(2); // Remaining length

    // Write packet ID
    dst.put_u16(packet.packet_id);

    Ok(())
}

/// Encode a PUBREL packet.
fn encode_pubrel_packet(packet: PubRelPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header (bit 1 must be set to 1)
    dst.put_u8(((PacketType::PubRel as u8) << 4) | 0x02);
    dst.put_u8(2); // Remaining length

    // Write packet ID
    dst.put_u16(packet.packet_id);

    Ok(())
}

/// Encode a PUBCOMP packet.
fn encode_pubcomp_packet(packet: PubCompPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::PubComp as u8) << 4);
    dst.put_u8(2); // Remaining length

    // Write packet ID
    dst.put_u16(packet.packet_id);

    Ok(())
}

/// Encode a SUBSCRIBE packet.
fn encode_subscribe_packet(packet: SubscribePacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Validate topics
    if packet.topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBSCRIBE must contain at least one topic filter",
            Some(8),
        ));
    }

    let mut payload = BytesMut::new();

    // Packet ID
    payload.put_u16(packet.packet_id);

    // Topic filters
    for (topic_filter, qos) in &packet.topics {
        encode_utf8_string(topic_filter, &mut payload);
        payload.put_u8(*qos as u8);
    }

    // Write fixed header (bit 1 must be set to 1)
    dst.put_u8(((PacketType::Subscribe as u8) << 4) | 0x02);
    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode a SUBACK packet.
fn encode_suback_packet(packet: SubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Validate return codes
    if packet.return_codes.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBACK must contain at least one return code",
            Some(9),
        ));
    }

    let mut payload = BytesMut::new();

    // Packet ID
    payload.put_u16(packet.packet_id);

    // Return codes
    for code in &packet.return_codes {
        payload.put_u8(code.as_u8());
    }

    // Write fixed header
    dst.put_u8((PacketType::SubAck as u8) << 4);
    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode an UNSUBSCRIBE packet.
fn encode_unsubscribe_packet(
    packet: UnsubscribePacket,
    dst: &mut BytesMut,
) -> Result<(), MqttError> {
    // Validate topics
    if packet.topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "UNSUBSCRIBE must contain at least one topic filter",
            Some(10),
        ));
    }

    let mut payload = BytesMut::new();

    // Packet ID
    payload.put_u16(packet.packet_id);

    // Topic filters
    for topic_filter in &packet.topics {
        encode_utf8_string(topic_filter, &mut payload);
    }

    // Write fixed header (bit 1 must be set to 1)
    dst.put_u8(((PacketType::Unsubscribe as u8) << 4) | 0x02);
    encode_remaining_length(payload.len(), dst);

    // Write payload
    dst.put(payload);

    Ok(())
}

/// Encode an UNSUBACK packet.
fn encode_unsuback_packet(packet: UnsubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::UnsubAck as u8) << 4);
    dst.put_u8(2); // Remaining length

    // Write packet ID
    dst.put_u16(packet.packet_id);

    Ok(())
}

/// Encode a PINGREQ packet.
fn encode_pingreq_packet(dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::PingReq as u8) << 4);
    dst.put_u8(0); // Remaining length

    Ok(())
}

/// Encode a PINGRESP packet.
fn encode_pingresp_packet(dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::PingResp as u8) << 4);
    dst.put_u8(0); // Remaining length

    Ok(())
}

/// Encode a DISCONNECT packet.
fn encode_disconnect_packet(dst: &mut BytesMut) -> Result<(), MqttError> {
    // Write fixed header
    dst.put_u8((PacketType::Disconnect as u8) << 4);
    dst.put_u8(0); // Remaining length

    Ok(())
}
