//! MQTT v5.0 packet encoder.
//!
//! This module provides encoding functionality for MQTT v5.0 packets,
//! converting structured `Packet` variants into raw byte streams.

use crate::Encoder;
use crate::MqttError;
use crate::v5::packet::*;
use crate::v5::validation::validate_packet;
use bytes::{BufMut, BytesMut};

/// Constants for MQTT 5.0 protocol.
const CONTINUATION_BIT: u8 = 0x80;

/// MQTT v5.0 packet encoder.
///
/// Implements the [`Encoder`] trait to serialize MQTT v5.0 control packets
/// into a byte buffer.
#[derive(Debug, Default)]
pub struct MqttEncoder;

impl Encoder<Packet> for MqttEncoder {
    /// The type of unrecoverable frame errors.
    type Error = MqttError;

    /// Encodes an MQTT v5.0 packet into the provided buffer.
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
    /// Returns `MqttError` if the packet violates MQTT v5.0 protocol constraints.
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
            Packet::Disconnect(packet) => encode_disconnect_packet(packet, dst),
            Packet::Auth(packet) => encode_auth_packet(packet, dst),
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

/// Encode variable length integer.
fn encode_variable_length(mut length: usize, dst: &mut BytesMut) {
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

/// Encode properties into a buffer.
///
/// # Arguments
///
/// * `props` - The properties structure to encode.
/// * `dst` - The buffer to write the encoded properties to.
fn encode_properties(props: &Properties, dst: &mut BytesMut) {
    let mut props_buf = BytesMut::new();

    // Session expiry interval (0x11)
    if let Some(interval) = props.session_expiry_interval {
        props_buf.put_u8(0x11);
        props_buf.put_u32(interval);
    }

    // Receive maximum (0x21)
    if let Some(max) = props.receive_maximum {
        props_buf.put_u8(0x21);
        props_buf.put_u16(max);
    }

    // Maximum packet size (0x27)
    if let Some(size) = props.maximum_packet_size {
        props_buf.put_u8(0x27);
        props_buf.put_u32(size);
    }

    // Topic alias maximum (0x22)
    if let Some(max) = props.topic_alias_maximum {
        props_buf.put_u8(0x22);
        props_buf.put_u16(max);
    }

    // Request response information (0x19)
    if let Some(req) = props.request_response_information {
        props_buf.put_u8(0x19);
        props_buf.put_u8(if req { 1 } else { 0 });
    }

    // Response information (0x1A)
    if let Some(ref info) = props.response_information {
        props_buf.put_u8(0x1A);
        encode_utf8_string(info, &mut props_buf);
    }

    // Request problem information (0x17)
    if let Some(req) = props.request_problem_information {
        props_buf.put_u8(0x17);
        props_buf.put_u8(if req { 1 } else { 0 });
    }

    // Assigned client identifier (0x12)
    if let Some(ref id) = props.assigned_client_identifier {
        props_buf.put_u8(0x12);
        encode_utf8_string(id, &mut props_buf);
    }

    // Server keep alive (0x13)
    if let Some(keep_alive) = props.server_keep_alive {
        props_buf.put_u8(0x13);
        props_buf.put_u16(keep_alive);
    }

    // Maximum QoS (0x24)
    if let Some(qos) = props.maximum_qos {
        props_buf.put_u8(0x24);
        props_buf.put_u8(qos as u8);
    }

    // Retain available (0x25)
    if let Some(available) = props.retain_available {
        props_buf.put_u8(0x25);
        props_buf.put_u8(if available { 1 } else { 0 });
    }

    // Wildcard subscription available (0x28)
    if let Some(available) = props.wildcard_subscription_available {
        props_buf.put_u8(0x28);
        props_buf.put_u8(if available { 1 } else { 0 });
    }

    // Subscription identifiers available (0x29)
    if let Some(available) = props.subscription_identifiers_available {
        props_buf.put_u8(0x29);
        props_buf.put_u8(if available { 1 } else { 0 });
    }

    // Shared subscription available (0x2A)
    if let Some(available) = props.shared_subscription_available {
        props_buf.put_u8(0x2A);
        props_buf.put_u8(if available { 1 } else { 0 });
    }

    // Authentication method (0x15)
    if let Some(ref method) = props.authentication_method {
        props_buf.put_u8(0x15);
        encode_utf8_string(method, &mut props_buf);
    }

    // Authentication data (0x16)
    if let Some(ref data) = props.authentication_data {
        props_buf.put_u8(0x16);
        encode_binary_data(data, &mut props_buf);
    }

    // Will delay interval (0x18)
    if let Some(interval) = props.will_delay_interval {
        props_buf.put_u8(0x18);
        props_buf.put_u32(interval);
    }

    // Payload format indicator (0x01)
    if let Some(indicator) = props.payload_format_indicator {
        props_buf.put_u8(0x01);
        props_buf.put_u8(indicator);
    }

    // Message expiry interval (0x02)
    if let Some(interval) = props.message_expiry_interval {
        props_buf.put_u8(0x02);
        props_buf.put_u32(interval);
    }

    // Content type (0x03)
    if let Some(ref content_type) = props.content_type {
        props_buf.put_u8(0x03);
        encode_utf8_string(content_type, &mut props_buf);
    }

    // Response topic (0x08)
    if let Some(ref response_topic) = props.response_topic {
        props_buf.put_u8(0x08);
        encode_utf8_string(response_topic, &mut props_buf);
    }

    // Correlation data (0x09)
    if let Some(ref correlation_data) = props.correlation_data {
        props_buf.put_u8(0x09);
        encode_binary_data(correlation_data, &mut props_buf);
    }

    // Topic alias (0x23)
    if let Some(alias) = props.topic_alias {
        props_buf.put_u8(0x23);
        props_buf.put_u16(alias);
    }

    // Subscription identifiers (0x0B)
    for id in &props.subscription_identifiers {
        props_buf.put_u8(0x0B);
        encode_variable_length(*id as usize, &mut props_buf);
    }

    // Reason string (0x1F)
    if let Some(ref reason_string) = props.reason_string {
        props_buf.put_u8(0x1F);
        encode_utf8_string(reason_string, &mut props_buf);
    }

    // User properties (0x26)
    for (key, value) in &props.user_properties {
        props_buf.put_u8(0x26);
        encode_utf8_string(key, &mut props_buf);
        encode_utf8_string(value, &mut props_buf);
    }

    // Server reference (0x1C)
    if let Some(ref reference) = props.server_reference {
        props_buf.put_u8(0x1C);
        encode_utf8_string(reference, &mut props_buf);
    }

    // Write properties length and data
    encode_variable_length(props_buf.len(), dst);
    dst.put(props_buf);
}

fn encode_reason_tail(reason_code: ReasonCode, properties: &Properties, dst: &mut BytesMut) {
    if reason_code == ReasonCode::Success && properties.is_empty() {
        return;
    }

    dst.put_u8(reason_code.as_u8());
    encode_properties(properties, dst);
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
    if packet.clean_start {
        connect_flags |= 0x02;
    }
    // Bit 0 is reserved and must be 0
    payload.put_u8(connect_flags);

    // Keep alive
    payload.put_u16(packet.keep_alive);

    // Properties
    encode_properties(&packet.properties, &mut payload);

    // Client ID
    encode_utf8_string(&packet.client_id, &mut payload);

    // Will topic and message
    if packet.will_flag {
        if let Some(will_props) = &packet.will_properties {
            encode_properties(will_props, &mut payload);
        } else {
            encode_variable_length(0, &mut payload);
        }
        if let Some(will_topic) = &packet.will_topic {
            encode_utf8_string(will_topic, &mut payload);
        }
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
    dst.put_u8(0x10); // CONNECT packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a CONNACK packet.
fn encode_connack_packet(packet: ConnAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();

    let mut flags = 0u8;
    if packet.session_present {
        flags |= 0x01;
    }
    // Bits 7-1 are reserved and must be 0
    payload.put_u8(flags);
    payload.put_u8(packet.reason_code.as_u8());

    encode_properties(&packet.properties, &mut payload);

    // Write fixed header
    dst.put_u8(0x20); // CONNACK packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PUBLISH packet.
fn encode_publish_packet(packet: PublishPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();

    encode_utf8_string(&packet.topic_name, &mut payload);

    if let Some(packet_id) = packet.packet_id {
        payload.put_u16(packet_id);
    }

    encode_properties(&packet.properties, &mut payload);
    payload.put_slice(&packet.payload);

    let mut first_byte = 0x30; // PUBLISH packet type
    if packet.duplicate {
        first_byte |= 0x08;
    }
    first_byte |= (packet.qos as u8) << 1;
    if packet.retain {
        first_byte |= 0x01;
    }

    dst.put_u8(first_byte);
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PUBACK packet.
fn encode_puback_packet(packet: PubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0x40); // PUBACK packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PUBREC packet.
fn encode_pubrec_packet(packet: PubRecPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0x50); // PUBREC packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PUBREL packet.
fn encode_pubrel_packet(packet: PubRelPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0x62); // PUBREL packet type with flags (bit 1 must be 1)
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PUBCOMP packet.
fn encode_pubcomp_packet(packet: PubCompPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0x70); // PUBCOMP packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a SUBSCRIBE packet.
fn encode_subscribe_packet(packet: SubscribePacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);

    encode_properties(&packet.properties, &mut payload);

    for topic in &packet.topics {
        encode_utf8_string(&topic.topic_filter, &mut payload);
        let mut options = topic.qos as u8;
        if topic.no_local {
            options |= 0x04;
        }
        if topic.retain_as_published {
            options |= 0x08;
        }
        options |= (topic.retain_handling & 0x03) << 4;
        // Bits 7-6 are reserved and must be 0
        payload.put_u8(options);
    }

    dst.put_u8(0x82); // SUBSCRIBE packet type with flags (bit 1 must be 1)
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a SUBACK packet.
fn encode_suback_packet(packet: SubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);

    encode_properties(&packet.properties, &mut payload);

    for code in &packet.reason_codes {
        payload.put_u8(code.as_u8());
    }

    dst.put_u8(0x90); // SUBACK packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode an UNSUBSCRIBE packet.
fn encode_unsubscribe_packet(
    packet: UnsubscribePacket,
    dst: &mut BytesMut,
) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);

    encode_properties(&packet.properties, &mut payload);

    for topic in &packet.topics {
        encode_utf8_string(topic, &mut payload);
    }

    dst.put_u8(0xA2); // UNSUBSCRIBE packet type with flags (bit 1 must be 1)
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode an UNSUBACK packet.
fn encode_unsuback_packet(packet: UnsubAckPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    payload.put_u16(packet.packet_id);

    encode_properties(&packet.properties, &mut payload);

    for code in &packet.reason_codes {
        payload.put_u8(code.as_u8());
    }

    dst.put_u8(0xB0); // UNSUBACK packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode a PINGREQ packet.
fn encode_pingreq_packet(dst: &mut BytesMut) -> Result<(), MqttError> {
    dst.put_u8(0xC0); // PINGREQ packet type
    dst.put_u8(0); // Remaining length

    Ok(())
}

/// Encode a PINGRESP packet.
fn encode_pingresp_packet(dst: &mut BytesMut) -> Result<(), MqttError> {
    dst.put_u8(0xD0); // PINGRESP packet type
    dst.put_u8(0); // Remaining length

    Ok(())
}

/// Encode a DISCONNECT packet.
fn encode_disconnect_packet(packet: DisconnectPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0xE0); // DISCONNECT packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}

/// Encode an AUTH packet.
fn encode_auth_packet(packet: AuthPacket, dst: &mut BytesMut) -> Result<(), MqttError> {
    let mut payload = BytesMut::new();
    encode_reason_tail(packet.reason_code, &packet.properties, &mut payload);

    dst.put_u8(0xF0); // AUTH packet type
    encode_remaining_length(payload.len(), dst);
    dst.put(payload);

    Ok(())
}
