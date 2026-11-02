//! Validation functions for MQTT v5.0 packets.
//!
//! This module provides validation functions that ensure MQTT packets
//! conform to the MQTT v5.0 specification requirements.

use super::packet::{
    AuthPacket, ConnAckPacket, ConnectPacket, DisconnectPacket, Packet, PacketType, Properties,
    PubAckPacket, PubCompPacket, PubRecPacket, PubRelPacket, PublishPacket, QoS, ReasonCode,
    SubAckPacket, SubscribePacket, SubscriptionOption, UnsubAckPacket, UnsubscribePacket,
};
use crate::error::MqttError;
use crate::v4::{self, validate_packet_id};

// ============================================================================
// Constants
// ============================================================================

/// The protocol name for MQTT v5.0.
pub const PROTOCOL_NAME_MQTT: &str = "MQTT";

/// The protocol level for MQTT v5.0.
pub const PROTOCOL_LEVEL_V5: u8 = 5;

/// Maximum length for topic name (limited by 2-byte length field).
pub const MAX_TOPIC_LENGTH: usize = 65535;

// ============================================================================
// CONNECT Packet Validation
// ============================================================================

/// Validates a CONNECT packet according to MQTT v5.0 specification.
///
/// # Arguments
///
/// * `packet` - The CONNECT packet to validate.
///
/// # Returns
///
/// `Ok(())` if the packet is valid, or an `Err(MqttError)` describing the violation.
pub fn validate_connect_packet(packet: &ConnectPacket) -> Result<(), MqttError> {
    validate_protocol_name(&packet.protocol_name)?;
    validate_protocol_level(packet.protocol_level)?;
    validate_connect_flags(packet)?;
    validate_client_id(&packet.client_id, packet.clean_start)?;
    validate_will_message(packet)?;
    validate_credentials_flags(packet)?;
    validate_connect_properties(&packet.properties)?;
    if let Some(ref will_props) = packet.will_properties {
        validate_will_properties(will_props)?;
    }
    Ok(())
}

/// Validates the protocol name.
///
/// According to MQTT v5.0 specification, the protocol name must be "MQTT".
///
/// # Arguments
///
/// * `name` - The protocol name string to validate.
pub fn validate_protocol_name(name: &str) -> Result<(), MqttError> {
    if name != PROTOCOL_NAME_MQTT {
        return Err(MqttError::protocol_violation(
            format!(
                "Invalid protocol name: expected '{}', got '{}'",
                PROTOCOL_NAME_MQTT, name
            ),
            Some(1),
        ));
    }
    Ok(())
}

/// Validates the protocol level.
///
/// According to MQTT v5.0 specification, the protocol level must be 5.
///
/// # Arguments
///
/// * `level` - The protocol level byte to validate.
pub fn validate_protocol_level(level: u8) -> Result<(), MqttError> {
    if level != PROTOCOL_LEVEL_V5 {
        return Err(MqttError::protocol_violation(
            format!(
                "Invalid protocol level: expected {}, got {}",
                PROTOCOL_LEVEL_V5, level
            ),
            Some(1),
        ));
    }
    Ok(())
}

/// Validates the connect flags consistency.
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` whose flags are to be validated.
pub fn validate_connect_flags(packet: &ConnectPacket) -> Result<(), MqttError> {
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
/// # Arguments
///
/// * `client_id` - The client identifier string.
/// * `_clean_start` - The clean start flag (unused in v5 validation of ID).
pub fn validate_client_id(client_id: &str, _clean_start: bool) -> Result<(), MqttError> {
    // In MQTT v5.0, empty client ID is allowed even without clean_start
    // The server will assign a client ID
    if client_id.len() > 65535 {
        return Err(MqttError::invalid_client_id(
            client_id,
            crate::error::ClientIdErrorReason::TooLong,
        ));
    }
    Ok(())
}

/// Validates Will message consistency.
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` to validate.
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
    }
    Ok(())
}

/// Validates credential flags consistency.
///
/// In MQTT v5.0, password can be sent without username (unlike v3.1.1).
///
/// # Arguments
///
/// * `packet` - The `ConnectPacket` to validate.
pub fn validate_credentials_flags(packet: &ConnectPacket) -> Result<(), MqttError> {
    // Note: In MQTT v5.0, password_flag can be set without username_flag.
    // This is different from v3.1.1 which required username when password is present.

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
// Properties Validation
// ============================================================================

/// Validates properties for CONNECT packet.
///
/// # Arguments
///
/// * `props` - The properties to validate.
pub fn validate_connect_properties(props: &Properties) -> Result<(), MqttError> {
    // Validate receive_maximum
    if let Some(receive_max) = props.receive_maximum
        && receive_max == 0
    {
        return Err(MqttError::protocol_violation(
            "Receive Maximum must be non-zero",
            Some(1),
        ));
    }

    // Validate maximum_packet_size
    if let Some(max_size) = props.maximum_packet_size
        && max_size == 0
    {
        return Err(MqttError::protocol_violation(
            "Maximum Packet Size must be non-zero",
            Some(1),
        ));
    }

    // Validate property scope: only allowed properties for CONNECT
    validate_property_scope(props, PacketType::Connect)?;

    Ok(())
}

/// Validates properties for Will message.
///
/// # Arguments
///
/// * `props` - The will properties to validate.
pub fn validate_will_properties(props: &Properties) -> Result<(), MqttError> {
    // Validate payload_format_indicator
    if let Some(pfi) = props.payload_format_indicator
        && pfi > 1
    {
        return Err(MqttError::protocol_violation(
            "Payload Format Indicator must be 0 or 1",
            Some(3),
        ));
    }

    // Will properties only allow specific properties
    if props.session_expiry_interval.is_some()
        || props.receive_maximum.is_some()
        || props.maximum_packet_size.is_some()
        || props.topic_alias_maximum.is_some()
        || props.topic_alias.is_some()
        || props.request_response_information.is_some()
        || props.request_problem_information.is_some()
        || props.assigned_client_identifier.is_some()
        || props.server_keep_alive.is_some()
        || props.maximum_qos.is_some()
        || props.retain_available.is_some()
        || props.wildcard_subscription_available.is_some()
        || props.subscription_identifiers_available.is_some()
        || props.shared_subscription_available.is_some()
        || props.authentication_method.is_some()
        || props.authentication_data.is_some()
        || props.server_reference.is_some()
        || props.reason_string.is_some()
        || !props.subscription_identifiers.is_empty()
    {
        return Err(MqttError::protocol_violation(
            "Will Properties contain properties not allowed in Will",
            Some(1),
        ));
    }

    Ok(())
}

/// Validates that properties are appropriate for the given packet type.
///
/// According to MQTT v5.0 spec, each property can only appear in specific packet types.
///
/// # Arguments
///
/// * `props` - The properties to validate.
/// * `packet_type` - The packet type the properties belong to.
pub fn validate_property_scope(
    props: &Properties,
    packet_type: PacketType,
) -> Result<(), MqttError> {
    let err = |prop_name: &str| -> MqttError {
        MqttError::protocol_violation(
            format!(
                "Property '{}' is not allowed in {:?} packet",
                prop_name, packet_type
            ),
            None,
        )
    };

    match packet_type {
        PacketType::Connect => {
            // CONNECT allows: session_expiry_interval, receive_maximum, maximum_packet_size,
            // topic_alias_maximum, request_response_information, request_problem_information,
            // user_properties, authentication_method, authentication_data
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.reason_string.is_some() {
                return Err(err("Reason String"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
        }
        PacketType::ConnAck => {
            // CONNACK allows: session_expiry_interval, receive_maximum, maximum_qos,
            // retain_available, maximum_packet_size, assigned_client_identifier,
            // topic_alias_maximum, reason_string, user_properties, wildcard_subscription_available,
            // subscription_identifiers_available, shared_subscription_available,
            // server_keep_alive, response_information, server_reference,
            // authentication_method, authentication_data
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
        }
        PacketType::Publish => {
            // PUBLISH allows: payload_format_indicator, message_expiry_interval,
            // topic_alias, response_topic, correlation_data, user_properties,
            // subscription_identifiers (only in messages from server to client), content_type
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.reason_string.is_some() {
                return Err(err("Reason String"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::PubAck | PacketType::PubRec | PacketType::PubRel | PacketType::PubComp => {
            // These allow: reason_string, user_properties
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::Subscribe => {
            // SUBSCRIBE allows: subscription_identifier, user_properties
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.reason_string.is_some() {
                return Err(err("Reason String"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::SubAck | PacketType::UnsubAck => {
            // These allow: reason_string, user_properties
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::Unsubscribe => {
            // UNSUBSCRIBE allows: user_properties
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.reason_string.is_some() {
                return Err(err("Reason String"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::Disconnect => {
            // DISCONNECT allows: session_expiry_interval, reason_string, user_properties,
            // server_reference
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.authentication_method.is_some() {
                return Err(err("Authentication Method"));
            }
            if props.authentication_data.is_some() {
                return Err(err("Authentication Data"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        PacketType::Auth => {
            // AUTH allows: authentication_method, authentication_data, reason_string, user_properties
            if props.session_expiry_interval.is_some() {
                return Err(err("Session Expiry Interval"));
            }
            if props.receive_maximum.is_some() {
                return Err(err("Receive Maximum"));
            }
            if props.maximum_packet_size.is_some() {
                return Err(err("Maximum Packet Size"));
            }
            if props.topic_alias_maximum.is_some() {
                return Err(err("Topic Alias Maximum"));
            }
            if props.topic_alias.is_some() {
                return Err(err("Topic Alias"));
            }
            if props.payload_format_indicator.is_some() {
                return Err(err("Payload Format Indicator"));
            }
            if props.message_expiry_interval.is_some() {
                return Err(err("Message Expiry Interval"));
            }
            if props.content_type.is_some() {
                return Err(err("Content Type"));
            }
            if props.response_topic.is_some() {
                return Err(err("Response Topic"));
            }
            if props.correlation_data.is_some() {
                return Err(err("Correlation Data"));
            }
            if !props.subscription_identifiers.is_empty() {
                return Err(err("Subscription Identifier"));
            }
            if props.assigned_client_identifier.is_some() {
                return Err(err("Assigned Client Identifier"));
            }
            if props.server_keep_alive.is_some() {
                return Err(err("Server Keep Alive"));
            }
            if props.maximum_qos.is_some() {
                return Err(err("Maximum QoS"));
            }
            if props.retain_available.is_some() {
                return Err(err("Retain Available"));
            }
            if props.wildcard_subscription_available.is_some() {
                return Err(err("Wildcard Subscription Available"));
            }
            if props.subscription_identifiers_available.is_some() {
                return Err(err("Subscription Identifiers Available"));
            }
            if props.shared_subscription_available.is_some() {
                return Err(err("Shared Subscription Available"));
            }
            if props.server_reference.is_some() {
                return Err(err("Server Reference"));
            }
            if props.will_delay_interval.is_some() {
                return Err(err("Will Delay Interval"));
            }
            if props.request_response_information.is_some() {
                return Err(err("Request Response Information"));
            }
            if props.request_problem_information.is_some() {
                return Err(err("Request Problem Information"));
            }
            if props.response_information.is_some() {
                return Err(err("Response Information"));
            }
        }
        _ => {
            // PingReq, PingResp don't have properties
        }
    }

    Ok(())
}

// ============================================================================
// Topic Validation
// ============================================================================

/// Validates a topic name for PUBLISH operations.
///
/// # Arguments
///
/// * `topic` - The topic name string to validate.
pub fn validate_topic_name(topic: &str) -> Result<(), MqttError> {
    // Reuse v4 validation
    v4::validate_topic_name(topic)
}

/// Validates a topic filter for SUBSCRIBE/UNSUBSCRIBE operations.
///
/// # Arguments
///
/// * `filter` - The topic filter string to validate.
pub fn validate_topic_filter(filter: &str) -> Result<(), MqttError> {
    // Check for shared subscription format: $share/{ShareName}/{filter}
    if let Some(shared) = parse_shared_subscription(filter) {
        // Validate the share name
        if shared.share_name.is_empty() {
            return Err(MqttError::InvalidTopicFilter {
                filter: filter.to_string(),
                reason: crate::error::TopicFilterErrorReason::Empty,
            });
        }
        // Share name must not contain '/', '+', or '#'
        if shared.share_name.contains('/')
            || shared.share_name.contains('+')
            || shared.share_name.contains('#')
        {
            return Err(MqttError::InvalidTopicFilter {
                filter: filter.to_string(),
                reason: crate::error::TopicFilterErrorReason::InvalidMultiLevelWildcard,
            });
        }
        // Validate the actual topic filter part
        v4::validate_topic_filter(&shared.topic_filter)
    } else {
        // Regular topic filter
        v4::validate_topic_filter(filter)
    }
}

// ============================================================================
// Shared Subscription
// ============================================================================

/// Parsed shared subscription information.
///
/// A shared subscription has the format: `$share/{ShareName}/{filter}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedSubscription {
    /// The share name (group identifier).
    pub share_name: String,
    /// The actual topic filter.
    pub topic_filter: String,
}

/// Parses a shared subscription topic filter.
///
/// Shared subscriptions have the format: `$share/{ShareName}/{filter}`
///
/// # Arguments
///
/// * `filter` - The topic filter string to parse.
///
/// # Returns
///
/// * `Some(SharedSubscription)` - If the filter is a valid shared subscription format.
/// * `None` - If the filter is not a shared subscription.
///
/// # Example
///
/// ```
/// use mqtt_codec::v5::parse_shared_subscription;
///
/// let shared = parse_shared_subscription("$share/consumer-group/sensor/+/data").unwrap();
/// assert_eq!(shared.share_name, "consumer-group");
/// assert_eq!(shared.topic_filter, "sensor/+/data");
///
/// // Not a shared subscription
/// assert!(parse_shared_subscription("sensor/data").is_none());
/// ```
pub fn parse_shared_subscription(filter: &str) -> Option<SharedSubscription> {
    const SHARE_PREFIX: &str = "$share/";

    if !filter.starts_with(SHARE_PREFIX) {
        return None;
    }

    let after_prefix = &filter[SHARE_PREFIX.len()..];

    // Find the next '/' which separates ShareName from the topic filter
    let slash_pos = after_prefix.find('/')?;

    if slash_pos == 0 {
        // Empty share name
        return None;
    }

    let share_name = &after_prefix[..slash_pos];
    let topic_filter = &after_prefix[slash_pos + 1..];

    if topic_filter.is_empty() {
        // Topic filter part cannot be empty
        return None;
    }

    Some(SharedSubscription {
        share_name: share_name.to_string(),
        topic_filter: topic_filter.to_string(),
    })
}

// ============================================================================
// Reason Code Validation
// ============================================================================

/// Validates that a reason code is valid for the given packet type.
///
/// # Arguments
///
/// * `code` - The reason code to validate.
/// * `packet_type` - The packet type the code is used in.
pub fn validate_reason_code_for_packet(
    code: ReasonCode,
    packet_type: PacketType,
) -> Result<(), MqttError> {
    let valid = match packet_type {
        PacketType::ConnAck => is_valid_connack_reason_code(code),
        PacketType::PubAck => is_valid_puback_reason_code(code),
        PacketType::PubRec => is_valid_pubrec_reason_code(code),
        PacketType::PubRel => is_valid_pubrel_reason_code(code),
        PacketType::PubComp => is_valid_pubcomp_reason_code(code),
        PacketType::SubAck => is_valid_suback_reason_code(code),
        PacketType::UnsubAck => is_valid_unsuback_reason_code(code),
        PacketType::Disconnect => is_valid_disconnect_reason_code(code),
        PacketType::Auth => is_valid_auth_reason_code(code),
        _ => true, // Other packets don't use reason codes in the same way
    };

    if !valid {
        return Err(MqttError::invalid_reason_code(
            code.as_u8(),
            format!("{:?}", packet_type),
        ));
    }

    Ok(())
}

/// Checks if a reason code is valid for CONNACK.
fn is_valid_connack_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::UnspecifiedError
            | ReasonCode::MalformedPacket
            | ReasonCode::ProtocolError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::UnsupportedProtocolVersion
            | ReasonCode::ClientIdentifierNotValid
            | ReasonCode::BadUserNameOrPassword
            | ReasonCode::NotAuthorized
            | ReasonCode::ServerUnavailable
            | ReasonCode::ServerBusy
            | ReasonCode::Banned
            | ReasonCode::BadAuthenticationMethod
            | ReasonCode::TopicNameInvalid
            | ReasonCode::ReceiveMaximumExceeded
            | ReasonCode::TopicAliasInvalid
            | ReasonCode::QuotaExceeded
            | ReasonCode::PayloadFormatInvalid
            | ReasonCode::RetainNotSupported
            | ReasonCode::QoSNotSupported
            | ReasonCode::UseAnotherServer
            | ReasonCode::ServerMoved
            | ReasonCode::ConnectionRateExceeded
    )
}

/// Checks if a reason code is valid for PUBACK.
fn is_valid_puback_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::NoMatchingSubscribers
            | ReasonCode::UnspecifiedError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::TopicNameInvalid
            | ReasonCode::PacketIdentifierNotFound
            | ReasonCode::QuotaExceeded
            | ReasonCode::PayloadFormatInvalid
    )
}

/// Checks if a reason code is valid for PUBREC.
fn is_valid_pubrec_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::NoMatchingSubscribers
            | ReasonCode::UnspecifiedError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::TopicNameInvalid
            | ReasonCode::PacketIdentifierNotFound
            | ReasonCode::QuotaExceeded
            | ReasonCode::PayloadFormatInvalid
    )
}

/// Checks if a reason code is valid for PUBREL.
fn is_valid_pubrel_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success | ReasonCode::PacketIdentifierNotFound
    )
}

/// Checks if a reason code is valid for PUBCOMP.
fn is_valid_pubcomp_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success | ReasonCode::PacketIdentifierNotFound
    )
}

/// Checks if a reason code is valid for SUBACK.
fn is_valid_suback_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::GrantedQoS1
            | ReasonCode::GrantedQoS2
            | ReasonCode::UnspecifiedError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::NotAuthorized
            | ReasonCode::TopicFilterInvalid
            | ReasonCode::TopicNameInvalid
            | ReasonCode::QuotaExceeded
            | ReasonCode::SharedSubscriptionNotSupported
            | ReasonCode::SubscriptionIdentifiersNotSupported
            | ReasonCode::WildcardSubscriptionsNotSupported
    )
}

/// Checks if a reason code is valid for UNSUBACK.
fn is_valid_unsuback_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::NoMatchingSubscribers
            | ReasonCode::UnspecifiedError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::NotAuthorized
            | ReasonCode::TopicFilterInvalid
            | ReasonCode::TopicNameInvalid
            | ReasonCode::QuotaExceeded
    )
}

/// Checks if a reason code is valid for DISCONNECT.
fn is_valid_disconnect_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success
            | ReasonCode::DisconnectWithWillMessage
            | ReasonCode::UnspecifiedError
            | ReasonCode::MalformedPacket
            | ReasonCode::ProtocolError
            | ReasonCode::ImplementationSpecificError
            | ReasonCode::NotAuthorized
            | ReasonCode::ServerBusy
            | ReasonCode::ServerShuttingDown
            | ReasonCode::BadAuthenticationMethod
            | ReasonCode::KeepAliveTimeout
            | ReasonCode::SessionTakenOver
            | ReasonCode::TopicFilterInvalid
            | ReasonCode::TopicNameInvalid
            | ReasonCode::ReceiveMaximumExceeded
            | ReasonCode::TopicAliasInvalid
            | ReasonCode::MessageRateTooHigh
            | ReasonCode::QuotaExceeded
            | ReasonCode::AdministrativeAction
            | ReasonCode::PayloadFormatInvalid
            | ReasonCode::RetainNotSupported
            | ReasonCode::QoSNotSupported
            | ReasonCode::UseAnotherServer
            | ReasonCode::ServerMoved
            | ReasonCode::SharedSubscriptionNotSupported
            | ReasonCode::MaximumConnectTime
            | ReasonCode::SubscriptionIdentifiersNotSupported
            | ReasonCode::WildcardSubscriptionsNotSupported
    )
}

/// Checks if a reason code is valid for AUTH.
fn is_valid_auth_reason_code(code: ReasonCode) -> bool {
    matches!(
        code,
        ReasonCode::Success | ReasonCode::ContinueAuthentication | ReasonCode::ReAuthenticate
    )
}

// ============================================================================
// Fixed Header Validation
// ============================================================================

/// Validates the fixed header flags for a given packet type.
///
/// # Arguments
///
/// * `packet_type` - The packet type identifier.
/// * `flags` - The flags byte from the fixed header.
pub fn validate_fixed_header_flags(packet_type: PacketType, flags: u8) -> Result<(), MqttError> {
    match packet_type {
        PacketType::Connect => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "CONNECT reserved bits must be 0",
                    Some(1),
                ));
            }
        }
        PacketType::ConnAck => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "CONNACK reserved bits must be 0",
                    Some(2),
                ));
            }
        }
        PacketType::Publish => {
            // QoS validation
            let qos = (flags >> 1) & 0x03;
            if qos == 3 {
                return Err(MqttError::protocol_violation(
                    "PUBLISH QoS value 3 is reserved",
                    Some(3),
                ));
            }
        }
        PacketType::PubAck => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBACK reserved bits must be 0",
                    Some(4),
                ));
            }
        }
        PacketType::PubRec => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBREC reserved bits must be 0",
                    Some(5),
                ));
            }
        }
        PacketType::PubRel => {
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "PUBREL reserved bits must be 0010",
                    Some(6),
                ));
            }
        }
        PacketType::PubComp => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PUBCOMP reserved bits must be 0",
                    Some(7),
                ));
            }
        }
        PacketType::Subscribe => {
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "SUBSCRIBE reserved bits must be 0010",
                    Some(8),
                ));
            }
        }
        PacketType::SubAck => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "SUBACK reserved bits must be 0",
                    Some(9),
                ));
            }
        }
        PacketType::Unsubscribe => {
            if flags != 0x02 {
                return Err(MqttError::protocol_violation(
                    "UNSUBSCRIBE reserved bits must be 0010",
                    Some(10),
                ));
            }
        }
        PacketType::UnsubAck => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "UNSUBACK reserved bits must be 0",
                    Some(11),
                ));
            }
        }
        PacketType::PingReq => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PINGREQ reserved bits must be 0",
                    Some(12),
                ));
            }
        }
        PacketType::PingResp => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "PINGRESP reserved bits must be 0",
                    Some(13),
                ));
            }
        }
        PacketType::Disconnect => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "DISCONNECT reserved bits must be 0",
                    Some(14),
                ));
            }
        }
        PacketType::Auth => {
            if flags != 0 {
                return Err(MqttError::protocol_violation(
                    "AUTH reserved bits must be 0",
                    Some(15),
                ));
            }
        }
    }
    Ok(())
}

// ============================================================================
// Packet-level validation
// ============================================================================

/// Validates QoS level and packet identifier consistency.
pub fn validate_qos_packet_id(qos: QoS, packet_id: Option<u16>) -> Result<(), MqttError> {
    match qos {
        QoS::AtMostOnce => {
            if packet_id.is_some() {
                return Err(MqttError::protocol_violation(
                    "Packet identifier must not be present for QoS 0",
                    None,
                ));
            }
        }
        QoS::AtLeastOnce | QoS::ExactlyOnce => match packet_id {
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

/// Validates common property value constraints.
pub fn validate_property_values(props: &Properties) -> Result<(), MqttError> {
    if let Some(pfi) = props.payload_format_indicator
        && pfi > 1
    {
        return Err(MqttError::protocol_violation(
            "Payload Format Indicator must be 0 or 1",
            None,
        ));
    }
    if let Some(alias) = props.topic_alias
        && alias == 0
    {
        return Err(MqttError::protocol_violation(
            "Topic Alias must not be 0",
            None,
        ));
    }
    if props.subscription_identifiers.contains(&0) {
        return Err(MqttError::protocol_violation(
            "Subscription Identifier must not be 0",
            None,
        ));
    }
    if let Some(ref topic) = props.response_topic {
        v4::validate_topic_name(topic)?;
    }
    Ok(())
}

/// Validates a PUBLISH topic name together with an optional Topic Alias.
pub fn validate_publish_topic(topic: &str, topic_alias: Option<u16>) -> Result<(), MqttError> {
    validate_property_values(&Properties {
        topic_alias,
        ..Properties::new()
    })?;

    let has_alias = topic_alias.is_some_and(|a| a != 0);
    if topic.is_empty() {
        if !has_alias {
            return Err(MqttError::protocol_violation(
                "Topic name is empty and no Topic Alias is set",
                Some(3),
            ));
        }
    } else {
        validate_topic_name(topic)?;
    }
    Ok(())
}

/// Validates a PUBLISH packet.
pub fn validate_publish_packet(packet: &PublishPacket) -> Result<(), MqttError> {
    validate_publish_topic(&packet.topic_name, packet.properties.topic_alias)?;
    validate_qos_packet_id(packet.qos, packet.packet_id)?;
    if packet.qos == QoS::AtMostOnce && packet.duplicate {
        return Err(MqttError::protocol_violation(
            "DUP flag must not be set for QoS 0 messages",
            Some(3),
        ));
    }
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::Publish)?;
    Ok(())
}

/// Validates a CONNACK packet.
pub fn validate_connack_packet(packet: &ConnAckPacket) -> Result<(), MqttError> {
    if packet.session_present && packet.reason_code != ReasonCode::Success {
        return Err(MqttError::protocol_violation(
            "Session Present must be 0 when Reason Code is not Success",
            Some(2),
        ));
    }
    validate_reason_code_for_packet(packet.reason_code, PacketType::ConnAck)?;
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::ConnAck)?;
    Ok(())
}

/// Validates a DISCONNECT packet.
pub fn validate_disconnect_packet(packet: &DisconnectPacket) -> Result<(), MqttError> {
    validate_reason_code_for_packet(packet.reason_code, PacketType::Disconnect)?;
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::Disconnect)?;
    Ok(())
}

/// Validates an AUTH packet.
pub fn validate_auth_packet(packet: &AuthPacket) -> Result<(), MqttError> {
    validate_reason_code_for_packet(packet.reason_code, PacketType::Auth)?;
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::Auth)?;
    // MQTT-5.0 [MQTT-4.12.0-5]: Authentication Method is required on AUTH packets.
    if packet.properties.authentication_method.is_none() {
        return Err(MqttError::protocol_violation(
            "AUTH packet must include Authentication Method",
            Some(15),
        ));
    }
    Ok(())
}

/// Validates a subscription option in SUBSCRIBE.
pub fn validate_subscription_option(option: &SubscriptionOption) -> Result<(), MqttError> {
    validate_topic_filter(&option.topic_filter)?;
    if option.retain_handling > 2 {
        return Err(MqttError::protocol_violation(
            "Retain handling must be 0, 1, or 2",
            Some(8),
        ));
    }
    Ok(())
}

/// Validates a SUBSCRIBE packet.
pub fn validate_subscribe_packet(packet: &SubscribePacket) -> Result<(), MqttError> {
    validate_packet_id(packet.packet_id)?;
    if packet.topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBSCRIBE must contain at least one topic filter",
            Some(8),
        ));
    }
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::Subscribe)?;
    for topic in &packet.topics {
        validate_subscription_option(topic)?;
    }
    Ok(())
}

/// Validates a SUBACK packet.
pub fn validate_suback_packet(packet: &SubAckPacket) -> Result<(), MqttError> {
    validate_packet_id(packet.packet_id)?;
    if packet.reason_codes.is_empty() {
        return Err(MqttError::protocol_violation(
            "SUBACK must contain at least one reason code",
            Some(9),
        ));
    }
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::SubAck)?;
    for code in &packet.reason_codes {
        validate_reason_code_for_packet(*code, PacketType::SubAck)?;
    }
    Ok(())
}

/// Validates an UNSUBSCRIBE packet.
pub fn validate_unsubscribe_packet(packet: &UnsubscribePacket) -> Result<(), MqttError> {
    validate_packet_id(packet.packet_id)?;
    if packet.topics.is_empty() {
        return Err(MqttError::protocol_violation(
            "UNSUBSCRIBE must contain at least one topic filter",
            Some(10),
        ));
    }
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::Unsubscribe)?;
    for filter in &packet.topics {
        validate_topic_filter(filter)?;
    }
    Ok(())
}

/// Validates an UNSUBACK packet.
pub fn validate_unsuback_packet(packet: &UnsubAckPacket) -> Result<(), MqttError> {
    validate_packet_id(packet.packet_id)?;
    if packet.reason_codes.is_empty() {
        return Err(MqttError::protocol_violation(
            "UNSUBACK must contain at least one reason code",
            Some(11),
        ));
    }
    validate_property_values(&packet.properties)?;
    validate_property_scope(&packet.properties, PacketType::UnsubAck)?;
    for code in &packet.reason_codes {
        validate_reason_code_for_packet(*code, PacketType::UnsubAck)?;
    }
    Ok(())
}

fn validate_puback_like(
    packet_id: u16,
    reason_code: ReasonCode,
    properties: &Properties,
    packet_type: PacketType,
) -> Result<(), MqttError> {
    validate_packet_id(packet_id)?;
    validate_reason_code_for_packet(reason_code, packet_type)?;
    validate_property_values(properties)?;
    validate_property_scope(properties, packet_type)?;
    Ok(())
}

/// Validates a PUBACK packet.
pub fn validate_puback_packet(packet: &PubAckPacket) -> Result<(), MqttError> {
    validate_puback_like(
        packet.packet_id,
        packet.reason_code,
        &packet.properties,
        PacketType::PubAck,
    )
}

/// Validates a PUBREC packet.
pub fn validate_pubrec_packet(packet: &PubRecPacket) -> Result<(), MqttError> {
    validate_puback_like(
        packet.packet_id,
        packet.reason_code,
        &packet.properties,
        PacketType::PubRec,
    )
}

/// Validates a PUBREL packet.
pub fn validate_pubrel_packet(packet: &PubRelPacket) -> Result<(), MqttError> {
    validate_puback_like(
        packet.packet_id,
        packet.reason_code,
        &packet.properties,
        PacketType::PubRel,
    )
}

/// Validates a PUBCOMP packet.
pub fn validate_pubcomp_packet(packet: &PubCompPacket) -> Result<(), MqttError> {
    validate_puback_like(
        packet.packet_id,
        packet.reason_code,
        &packet.properties,
        PacketType::PubComp,
    )
}

/// Validates any MQTT v5.0 packet before encode or after decode.
pub fn validate_packet(packet: &Packet) -> Result<(), MqttError> {
    match packet {
        Packet::Connect(p) => validate_connect_packet(p),
        Packet::ConnAck(p) => validate_connack_packet(p),
        Packet::Publish(p) => validate_publish_packet(p),
        Packet::PubAck(p) => validate_puback_packet(p),
        Packet::PubRec(p) => validate_pubrec_packet(p),
        Packet::PubRel(p) => validate_pubrel_packet(p),
        Packet::PubComp(p) => validate_pubcomp_packet(p),
        Packet::Subscribe(p) => validate_subscribe_packet(p),
        Packet::SubAck(p) => validate_suback_packet(p),
        Packet::Unsubscribe(p) => validate_unsubscribe_packet(p),
        Packet::UnsubAck(p) => validate_unsuback_packet(p),
        Packet::PingReq(_) | Packet::PingResp(_) => Ok(()),
        Packet::Disconnect(p) => validate_disconnect_packet(p),
        Packet::Auth(p) => validate_auth_packet(p),
    }
}
