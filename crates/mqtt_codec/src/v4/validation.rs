//! Validation functions for MQTT v3.1.1 packets.
//!
//! This module provides validation functions that ensure MQTT packets
//! conform to the MQTT v3.1.1 specification requirements.

use super::packet::{ConnectPacket, QoS};
use crate::error::{ClientIdErrorReason, MqttError, TopicErrorReason, TopicFilterErrorReason};

// ============================================================================
// Constants
// ============================================================================

/// The protocol name for MQTT v3.1.1.
pub const PROTOCOL_NAME_MQTT: &str = "MQTT";

/// The protocol level for MQTT v3.1.1.
pub const PROTOCOL_LEVEL_V4: u8 = 4;

/// Maximum length for client ID (limited by 2-byte length field).
pub const MAX_CLIENT_ID_LENGTH: usize = 65535;

/// Maximum length for topic name (limited by 2-byte length field).
pub const MAX_TOPIC_LENGTH: usize = 65535;

// ============================================================================
// CONNECT Packet Validation
// ============================================================================

/// Validates a CONNECT packet according to MQTT v3.1.1 specification.
///
/// # Arguments
///
/// * `packet` - The CONNECT packet to validate.
///
/// # Returns
///
/// `Ok(())` if the packet is valid, or an `Err(MqttError)` describing the violation.
///
/// # Validation Rules
///
/// 1. Protocol Name must be "MQTT"
/// 2. Protocol Level must be 4
/// 3. Reserved bit (bit 0 of connect flags) must be 0
/// 4. If Will Flag is set, Will Topic and Will Message must be present
/// 5. If Will Flag is not set, Will QoS and Will Retain must be 0
/// 6. If Password Flag is set, Username Flag must also be set
/// 7. If Client ID is empty, Clean Session must be 1
pub fn validate_connect_packet(packet: &ConnectPacket) -> Result<(), MqttError> {
    validate_protocol_name(&packet.protocol_name)?;
    validate_protocol_level(packet.protocol_level)?;
    validate_connect_flags(packet)?;
    validate_client_id(&packet.client_id, packet.clean_session)?;
    validate_will_message(packet)?;
    validate_credentials_flags(packet)?;
    Ok(())
}

/// Validates the protocol name.
///
/// According to MQTT v3.1.1 specification, the protocol name must be "MQTT".
///
/// # Arguments
///
/// * `name` - The protocol name string to validate.
///
/// # Returns
///
/// `Ok(())` if the name is "MQTT", otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_protocol_name(name: &str) -> Result<(), MqttError> {
    if name != PROTOCOL_NAME_MQTT {
        return Err(MqttError::protocol_violation(
            format!(
                "Invalid protocol name: expected '{}', got '{}'",
                PROTOCOL_NAME_MQTT, name
            ),
            Some(1), // CONNECT packet type
        ));
    }
    Ok(())
}

/// Validates the protocol level.
///
/// According to MQTT v3.1.1 specification, the protocol level must be 4.
///
/// # Arguments
///
/// * `level` - The protocol level byte to validate.
///
/// # Returns
///
/// `Ok(())` if the level is 4, otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_protocol_level(level: u8) -> Result<(), MqttError> {
    if level != PROTOCOL_LEVEL_V4 {
        return Err(MqttError::protocol_violation(
            format!(
                "Invalid protocol level: expected {}, got {}",
                PROTOCOL_LEVEL_V4, level
            ),
            Some(1),
        ));
    }
    Ok(())
}

/// Validates the connect flags consistency.
///
/// This checks:
/// - Reserved bit (bit 0) must be 0
/// - Will flag consistency with Will Topic/Message
/// - Password flag requires Username flag
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` whose flags are to be validated.
///
/// # Returns
///
/// `Ok(())` if flags are consistent, otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_connect_flags(packet: &ConnectPacket) -> Result<(), MqttError> {
    // Note: The reserved bit check is done during decoding since we don't
    // store the raw flags byte. This function validates the logical consistency.

    // If Will Flag is set, Will QoS must be valid (0, 1, or 2)
    // This is already enforced by the QoS enum type

    // If Will Flag is not set, Will QoS and Will Retain should be 0
    if !packet.will_flag && packet.will_qos != QoS::AtMostOnce {
        return Err(MqttError::protocol_violation(
            "Will QoS must be 0 when Will Flag is not set",
            Some(1),
        ));
    }

    Ok(())
}

/// Validates the client identifier.
///
/// # Rules
///
/// 1. Length must not exceed 65535 bytes (limited by 2-byte length field)
/// 2. If empty, Clean Session must be 1
/// 3. Must be valid UTF-8 (already enforced by String type)
///
/// # Arguments
///
/// * `client_id` - The client identifier string.
/// * `clean_session` - The clean session flag from the CONNECT packet.
///
/// # Returns
///
/// `Ok(())` if valid, otherwise returns `MqttError::InvalidClientId`.
pub fn validate_client_id(client_id: &str, clean_session: bool) -> Result<(), MqttError> {
    // Check length
    if client_id.len() > MAX_CLIENT_ID_LENGTH {
        return Err(MqttError::invalid_client_id(
            client_id,
            ClientIdErrorReason::TooLong,
        ));
    }

    // Check empty client ID with Clean Session = 0
    if client_id.is_empty() && !clean_session {
        return Err(MqttError::invalid_client_id(
            client_id,
            ClientIdErrorReason::EmptyWithNonCleanSession,
        ));
    }

    Ok(())
}

/// Validates Will message consistency.
///
/// If Will Flag is set, Will Topic and Will Message must be present.
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` to validate.
///
/// # Returns
///
/// `Ok(())` if consistent, otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_will_message(packet: &ConnectPacket) -> Result<(), MqttError> {
    if packet.will_flag {
        if packet.will_topic.is_none() {
            return Err(MqttError::protocol_violation(
                "Will Topic is required when Will Flag is set",
                Some(1),
            ));
        }
        if packet.will_message.is_none() {
            return Err(MqttError::protocol_violation(
                "Will Message is required when Will Flag is set",
                Some(1),
            ));
        }
        // Validate Will Topic as a topic name (no wildcards)
        if let Some(ref topic) = packet.will_topic {
            validate_topic_name(topic)?;
        }
    } else {
        // Will Topic and Will Message should not be present when Will Flag is not set
        if packet.will_topic.is_some() {
            return Err(MqttError::protocol_violation(
                "Will Topic should not be present when Will Flag is not set",
                Some(1),
            ));
        }
        if packet.will_message.is_some() {
            return Err(MqttError::protocol_violation(
                "Will Message should not be present when Will Flag is not set",
                Some(1),
            ));
        }
    }
    Ok(())
}

/// Validates credential flags consistency.
///
/// If Password Flag is set, Username Flag must also be set.
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` to validate.
///
/// # Returns
///
/// `Ok(())` if consistent, otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_credentials_flags(packet: &ConnectPacket) -> Result<(), MqttError> {
    // If Password Flag is set, Username Flag must be set
    if packet.password_flag && !packet.username_flag {
        return Err(MqttError::protocol_violation(
            "Username Flag must be set when Password Flag is set",
            Some(1),
        ));
    }

    // Check consistency with actual values
    if packet.username_flag && packet.username.is_none() {
        return Err(MqttError::protocol_violation(
            "Username is required when Username Flag is set",
            Some(1),
        ));
    }
    if !packet.username_flag && packet.username.is_some() {
        return Err(MqttError::protocol_violation(
            "Username Flag must be set when Username is present",
            Some(1),
        ));
    }
    if packet.password_flag && packet.password.is_none() {
        return Err(MqttError::protocol_violation(
            "Password is required when Password Flag is set",
            Some(1),
        ));
    }
    if !packet.password_flag && packet.password.is_some() {
        return Err(MqttError::protocol_violation(
            "Password Flag must be set when Password is present",
            Some(1),
        ));
    }

    Ok(())
}

// ============================================================================
// Topic Name Validation
// ============================================================================

/// Validates a topic name for PUBLISH operations.
///
/// # Rules (MQTT v3.1.1 Specification)
///
/// 1. Must not be empty
/// 2. Must not contain null character (U+0000)
/// 3. Must not contain wildcard characters (+ or #)
/// 4. Must be valid UTF-8 (already enforced by String type)
///
/// # Arguments
///
/// * `topic` - The topic name string to validate.
///
/// # Returns
///
/// `Ok(())` if valid, otherwise returns `MqttError::InvalidTopicName`.
pub fn validate_topic_name(topic: &str) -> Result<(), MqttError> {
    // Check empty
    if topic.is_empty() {
        return Err(MqttError::invalid_topic_name(
            topic,
            TopicErrorReason::Empty,
        ));
    }

    // Check length
    if topic.len() > MAX_TOPIC_LENGTH {
        return Err(MqttError::invalid_topic_name(
            topic,
            TopicErrorReason::TooLong,
        ));
    }

    // Check for null character
    if topic.contains('\0') {
        return Err(MqttError::invalid_topic_name(
            topic,
            TopicErrorReason::ContainsNull,
        ));
    }

    // Check for wildcards (not allowed in topic names for PUBLISH)
    if topic.contains('+') || topic.contains('#') {
        return Err(MqttError::invalid_topic_name(
            topic,
            TopicErrorReason::ContainsWildcard,
        ));
    }

    Ok(())
}

// ============================================================================
// Topic Filter Validation
// ============================================================================

/// Validates a topic filter for SUBSCRIBE/UNSUBSCRIBE operations.
///
/// # Rules (MQTT v3.1.1 Specification)
///
/// 1. Must not be empty
/// 2. Must not contain null character (U+0000)
/// 3. Multi-level wildcard (#) must be the last character and preceded by /
/// 4. Single-level wildcard (+) must occupy an entire level
///
/// # Arguments
///
/// * `filter` - The topic filter string to validate.
///
/// # Returns
///
/// `Ok(())` if valid, otherwise returns `MqttError::InvalidTopicFilter`.
pub fn validate_topic_filter(filter: &str) -> Result<(), MqttError> {
    // Check empty
    if filter.is_empty() {
        return Err(MqttError::invalid_topic_filter(
            filter,
            TopicFilterErrorReason::Empty,
        ));
    }

    // Check length
    if filter.len() > MAX_TOPIC_LENGTH {
        return Err(MqttError::invalid_topic_filter(
            filter,
            TopicFilterErrorReason::TooLong,
        ));
    }

    // Check for null character
    if filter.contains('\0') {
        return Err(MqttError::invalid_topic_filter(
            filter,
            TopicFilterErrorReason::ContainsNull,
        ));
    }

    // Validate wildcard usage
    validate_wildcards(filter)?;

    Ok(())
}

/// Validates wildcard usage in topic filters.
///
/// # Rules
///
/// - Multi-level wildcard (#):
///   - Must be the last character
///   - Must be preceded by a topic level separator (/) or be the only character
///
/// - Single-level wildcard (+):
///   - Must occupy an entire topic level (surrounded by / or at start/end)
///
/// # Arguments
///
/// * `filter` - The topic filter string to validate.
///
/// # Returns
///
/// `Ok(())` if wildcards are used correctly, otherwise returns `MqttError::InvalidTopicFilter`.
fn validate_wildcards(filter: &str) -> Result<(), MqttError> {
    let bytes = filter.as_bytes();
    let len = bytes.len();

    for (i, &byte) in bytes.iter().enumerate() {
        match byte {
            b'#' => {
                // Multi-level wildcard must be the last character
                if i != len - 1 {
                    return Err(MqttError::invalid_topic_filter(
                        filter,
                        TopicFilterErrorReason::InvalidMultiLevelWildcard,
                    ));
                }
                // If not the only character, must be preceded by /
                if i > 0 && bytes[i - 1] != b'/' {
                    return Err(MqttError::invalid_topic_filter(
                        filter,
                        TopicFilterErrorReason::InvalidMultiLevelWildcard,
                    ));
                }
            }
            b'+' => {
                // Single-level wildcard must occupy an entire level
                // It must be at start, end, or surrounded by /
                let at_start = i == 0;
                let at_end = i == len - 1;
                let preceded_by_slash = i > 0 && bytes[i - 1] == b'/';
                let followed_by_slash = i < len - 1 && bytes[i + 1] == b'/';

                // Valid cases:
                // 1. At start and followed by / (e.g., "+/topic")
                // 2. At end and preceded by / (e.g., "topic/+")
                // 3. Surrounded by / (e.g., "topic/+/subtopic")
                // 4. Only character (at_start && at_end) (e.g., "+")
                let valid_plus_wildcard =
                    (preceded_by_slash || at_start) && (followed_by_slash || at_end);
                if !valid_plus_wildcard {
                    return Err(MqttError::invalid_topic_filter(
                        filter,
                        TopicFilterErrorReason::InvalidSingleLevelWildcard,
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

// ============================================================================
// Fixed Header Validation
// ============================================================================

/// Validates the fixed header flags for a given packet type.
///
/// # Arguments
///
/// * `packet_type` - The packet type (1-14)
/// * `flags` - The flags byte (lower 4 bits of the first byte)
///
/// # Returns
///
/// `Ok(())` if the flags are valid, or an `Err(MqttError)` describing the violation.
pub fn validate_fixed_header_flags(packet_type: u8, flags: u8) -> Result<(), MqttError> {
    match packet_type {
        1 => {
            // CONNECT: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "CONNECT reserved bits must be 0",
                    Some(1),
                ));
            }
        }
        2 => {
            // CONNACK: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "CONNACK reserved bits must be 0",
                    Some(2),
                ));
            }
        }
        3 => {
            // PUBLISH: Flags are used (DUP, QoS, RETAIN), no validation needed here
            // QoS validation: if QoS bits are 3 (invalid), should reject
            let qos = (flags >> 1) & 0x03;
            if qos == 3 {
                return Err(MqttError::protocol_violation(
                    "PUBLISH QoS value 3 is reserved",
                    Some(3),
                ));
            }
        }
        4 => {
            // PUBACK: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBACK reserved bits must be 0",
                    Some(4),
                ));
            }
        }
        5 => {
            // PUBREC: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBREC reserved bits must be 0",
                    Some(5),
                ));
            }
        }
        6 => {
            // PUBREL: Reserved bits must be 0010 (0x02)
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "PUBREL reserved bits must be 0010",
                    Some(6),
                ));
            }
        }
        7 => {
            // PUBCOMP: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBCOMP reserved bits must be 0",
                    Some(7),
                ));
            }
        }
        8 => {
            // SUBSCRIBE: Reserved bits must be 0010 (0x02)
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "SUBSCRIBE reserved bits must be 0010",
                    Some(8),
                ));
            }
        }
        9 => {
            // SUBACK: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "SUBACK reserved bits must be 0",
                    Some(9),
                ));
            }
        }
        10 => {
            // UNSUBSCRIBE: Reserved bits must be 0010 (0x02)
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "UNSUBSCRIBE reserved bits must be 0010",
                    Some(10),
                ));
            }
        }
        11 => {
            // UNSUBACK: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "UNSUBACK reserved bits must be 0",
                    Some(11),
                ));
            }
        }
        12 => {
            // PINGREQ: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PINGREQ reserved bits must be 0",
                    Some(12),
                ));
            }
        }
        13 => {
            // PINGRESP: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PINGRESP reserved bits must be 0",
                    Some(13),
                ));
            }
        }
        14 => {
            // DISCONNECT: Reserved bits must be 0
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "DISCONNECT reserved bits must be 0",
                    Some(14),
                ));
            }
        }
        _ => {
            return Err(MqttError::protocol_violation(
                format!("Invalid packet type: {}", packet_type),
                None,
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Packet ID Validation
// ============================================================================

/// Validates that a packet ID is non-zero.
///
/// According to MQTT v3.1.1, packet identifiers must be non-zero
/// for packets that require them (PUBACK, PUBREC, PUBREL, PUBCOMP,
/// SUBSCRIBE, SUBACK, UNSUBSCRIBE, UNSUBACK).
///
/// # Arguments
///
/// * `packet_id` - The packet identifier to validate.
///
/// # Returns
///
/// `Ok(())` if non-zero, otherwise returns `MqttError::ProtocolViolation`.
pub fn validate_packet_id(packet_id: u16) -> Result<(), MqttError> {
    if packet_id == 0 {
        return Err(MqttError::protocol_violation(
            "Packet identifier must be non-zero",
            None,
        ));
    }
    Ok(())
}

// ============================================================================
// QoS / Packet ID
// ============================================================================

/// Validates QoS level and packet identifier consistency.
pub fn validate_qos_packet_id(
    qos: super::packet::QoS,
    packet_id: Option<u16>,
) -> Result<(), MqttError> {
    match qos {
        super::packet::QoS::AtMostOnce => {
            if packet_id.is_some() {
                return Err(MqttError::protocol_violation(
                    "Packet identifier must not be present for QoS 0",
                    None,
                ));
            }
        }
        super::packet::QoS::AtLeastOnce | super::packet::QoS::ExactlyOnce => match packet_id {
            Some(id) => validate_packet_id(id)?,
            None => {
                return Err(MqttError::protocol_violation(
                    "Packet identifier is required for QoS 1 and QoS 2",
                    None,
                ));
            }
        },
    }
    Ok(())
}

/// Validates a PUBLISH packet.
pub fn validate_publish_packet(packet: &super::packet::PublishPacket) -> Result<(), MqttError> {
    validate_topic_name(&packet.topic_name)?;
    validate_qos_packet_id(packet.qos, packet.packet_id)?;
    if packet.qos == super::packet::QoS::AtMostOnce && packet.duplicate {
        return Err(MqttError::protocol_violation(
            "DUP flag must not be set for QoS 0 messages",
            Some(3),
        ));
    }
    Ok(())
}

/// Validates a CONNACK packet.
pub fn validate_connack_packet(packet: &super::packet::ConnAckPacket) -> Result<(), MqttError> {
    if packet.session_present && !packet.return_code.is_success() {
        return Err(MqttError::protocol_violation(
            "Session Present must be 0 when return code is not Accepted",
            Some(2),
        ));
    }
    Ok(())
}

/// Validates any MQTT v3.1.1 packet before encode or after decode.
pub fn validate_packet(packet: &super::packet::Packet) -> Result<(), MqttError> {
    use super::packet::Packet;

    match packet {
        Packet::Connect(p) => validate_connect_packet(p),
        Packet::ConnAck(p) => validate_connack_packet(p),
        Packet::Publish(p) => validate_publish_packet(p),
        Packet::PubAck(p) => validate_packet_id(p.packet_id),
        Packet::PubRec(p) => validate_packet_id(p.packet_id),
        Packet::PubRel(p) => validate_packet_id(p.packet_id),
        Packet::PubComp(p) => validate_packet_id(p.packet_id),
        Packet::Subscribe(p) => {
            validate_packet_id(p.packet_id)?;
            if p.topics.is_empty() {
                return Err(MqttError::protocol_violation(
                    "SUBSCRIBE must contain at least one topic filter",
                    Some(8),
                ));
            }
            for (filter, _qos) in &p.topics {
                validate_topic_filter(filter)?;
            }
            Ok(())
        }
        Packet::SubAck(p) => {
            validate_packet_id(p.packet_id)?;
            if p.return_codes.is_empty() {
                return Err(MqttError::protocol_violation(
                    "SUBACK must contain at least one return code",
                    Some(9),
                ));
            }
            Ok(())
        }
        Packet::Unsubscribe(p) => {
            validate_packet_id(p.packet_id)?;
            if p.topics.is_empty() {
                return Err(MqttError::protocol_violation(
                    "UNSUBSCRIBE must contain at least one topic filter",
                    Some(10),
                ));
            }
            for filter in &p.topics {
                validate_topic_filter(filter)?;
            }
            Ok(())
        }
        Packet::UnsubAck(p) => validate_packet_id(p.packet_id),
        Packet::PingReq(_) | Packet::PingResp(_) | Packet::Disconnect(_) => Ok(()),
    }
}
