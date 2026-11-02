//! MQTT v5.0 property identifier constants.
//!
//! This module defines all property identifiers as specified in MQTT v5.0.
//! Property identifiers are used in the properties field of various packet types.

// ============================================================================
// Property Identifier Constants
// ============================================================================

/// Payload Format Indicator (0x01).
///
/// Indicates the format of the payload.
/// - 0: Unspecified byte stream (default)
/// - 1: UTF-8 encoded character data
///
/// Used in: PUBLISH, Will Properties
pub const PROPERTY_PAYLOAD_FORMAT_INDICATOR: u8 = 0x01;

/// Message Expiry Interval (0x02).
///
/// The lifetime of the message in seconds.
/// If absent, the message does not expire.
///
/// Used in: PUBLISH, Will Properties
pub const PROPERTY_MESSAGE_EXPIRY_INTERVAL: u8 = 0x02;

/// Content Type (0x03).
///
/// A UTF-8 string describing the content of the message.
///
/// Used in: PUBLISH, Will Properties
pub const PROPERTY_CONTENT_TYPE: u8 = 0x03;

/// Response Topic (0x08).
///
/// A UTF-8 string used as the topic name for a response message.
///
/// Used in: PUBLISH, Will Properties
pub const PROPERTY_RESPONSE_TOPIC: u8 = 0x08;

/// Correlation Data (0x09).
///
/// Binary data used by the sender of the request message to identify
/// which request the response message is for when it is received.
///
/// Used in: PUBLISH, Will Properties
pub const PROPERTY_CORRELATION_DATA: u8 = 0x09;

/// Subscription Identifier (0x0B).
///
/// A variable byte integer identifier for the subscription.
///
/// Used in: PUBLISH, SUBSCRIBE
pub const PROPERTY_SUBSCRIPTION_IDENTIFIER: u8 = 0x0B;

/// Session Expiry Interval (0x11).
///
/// The session expiry interval in seconds.
///
/// Used in: CONNECT, CONNACK, DISCONNECT
pub const PROPERTY_SESSION_EXPIRY_INTERVAL: u8 = 0x11;

/// Assigned Client Identifier (0x12).
///
/// A UTF-8 string assigned by the server as the client identifier.
///
/// Used in: CONNACK
pub const PROPERTY_ASSIGNED_CLIENT_IDENTIFIER: u8 = 0x12;

/// Server Keep Alive (0x13).
///
/// The keep alive time in seconds assigned by the server.
///
/// Used in: CONNACK
pub const PROPERTY_SERVER_KEEP_ALIVE: u8 = 0x13;

/// Authentication Method (0x15).
///
/// A UTF-8 string containing the name of the authentication method.
///
/// Used in: CONNECT, CONNACK, AUTH
pub const PROPERTY_AUTHENTICATION_METHOD: u8 = 0x15;

/// Authentication Data (0x16).
///
/// Binary data containing authentication data.
///
/// Used in: CONNECT, CONNACK, AUTH
pub const PROPERTY_AUTHENTICATION_DATA: u8 = 0x16;

/// Request Problem Information (0x17).
///
/// Indicates whether the client wants the server to return problem information.
/// - 0: Server should not return Problem Information
/// - 1: Server may return Problem Information (default)
///
/// Used in: CONNECT
pub const PROPERTY_REQUEST_PROBLEM_INFORMATION: u8 = 0x17;

/// Will Delay Interval (0x18).
///
/// The delay in seconds before the will message is published.
///
/// Used in: Will Properties
pub const PROPERTY_WILL_DELAY_INTERVAL: u8 = 0x18;

/// Request Response Information (0x19).
///
/// Indicates whether the client wants the server to return response information.
/// - 0: Server should not return Response Information (default)
/// - 1: Server may return Response Information
///
/// Used in: CONNECT
pub const PROPERTY_REQUEST_RESPONSE_INFORMATION: u8 = 0x19;

/// Response Information (0x1A).
///
/// A UTF-8 string containing response information.
///
/// Used in: CONNACK
pub const PROPERTY_RESPONSE_INFORMATION: u8 = 0x1A;

/// Server Reference (0x1C).
///
/// A UTF-8 string indicating an alternative server to use.
///
/// Used in: CONNACK, DISCONNECT
pub const PROPERTY_SERVER_REFERENCE: u8 = 0x1C;

/// Reason String (0x1F).
///
/// A UTF-8 string representing the reason for the operation.
///
/// Used in: All packets except PUBLISH
pub const PROPERTY_REASON_STRING: u8 = 0x1F;

/// Receive Maximum (0x21).
///
/// The maximum number of QoS 1 and QoS 2 publications that the client
/// or server is willing to process concurrently.
///
/// Used in: CONNECT, CONNACK
pub const PROPERTY_RECEIVE_MAXIMUM: u8 = 0x21;

/// Topic Alias Maximum (0x22).
///
/// The maximum value for a topic alias that the client or server will accept.
///
/// Used in: CONNECT, CONNACK
pub const PROPERTY_TOPIC_ALIAS_MAXIMUM: u8 = 0x22;

/// Topic Alias (0x23).
///
/// A variable byte integer used to identify the topic name.
///
/// Used in: PUBLISH
pub const PROPERTY_TOPIC_ALIAS: u8 = 0x23;

/// Maximum QoS (0x24).
///
/// The maximum QoS level that the server supports.
///
/// Used in: CONNACK
pub const PROPERTY_MAXIMUM_QOS: u8 = 0x24;

/// Retain Available (0x25).
///
/// Indicates whether the server supports retained messages.
/// - 0: Retained messages not supported
/// - 1: Retained messages supported (default)
///
/// Used in: CONNACK
pub const PROPERTY_RETAIN_AVAILABLE: u8 = 0x25;

/// User Property (0x26).
///
/// A key-value pair for application-defined properties.
///
/// Used in: All packets
pub const PROPERTY_USER_PROPERTY: u8 = 0x26;

/// Maximum Packet Size (0x27).
///
/// The maximum packet size that the client or server is willing to accept.
///
/// Used in: CONNECT, CONNACK
pub const PROPERTY_MAXIMUM_PACKET_SIZE: u8 = 0x27;

/// Wildcard Subscription Available (0x28).
///
/// Indicates whether the server supports wildcard subscriptions.
/// - 0: Wildcard subscriptions not supported
/// - 1: Wildcard subscriptions supported (default)
///
/// Used in: CONNACK
pub const PROPERTY_WILDCARD_SUBSCRIPTION_AVAILABLE: u8 = 0x28;

/// Subscription Identifier Available (0x29).
///
/// Indicates whether the server supports subscription identifiers.
/// - 0: Subscription identifiers not supported
/// - 1: Subscription identifiers supported (default)
///
/// Used in: CONNACK
pub const PROPERTY_SUBSCRIPTION_IDENTIFIER_AVAILABLE: u8 = 0x29;

/// Shared Subscription Available (0x2A).
///
/// Indicates whether the server supports shared subscriptions.
/// - 0: Shared subscriptions not supported
/// - 1: Shared subscriptions supported (default)
///
/// Used in: CONNACK
pub const PROPERTY_SHARED_SUBSCRIPTION_AVAILABLE: u8 = 0x2A;

// ============================================================================
// Property Data Types
// ============================================================================

/// Property data types for parsing and encoding.
///
/// Defines the wire format for various MQTT v5.0 property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    /// Single byte (0 or 1).
    Byte,
    /// Two-byte unsigned integer.
    TwoByteInteger,
    /// Four-byte unsigned integer.
    FourByteInteger,
    /// Variable byte integer (1-4 bytes).
    VariableByteInteger,
    /// UTF-8 encoded string.
    Utf8String,
    /// UTF-8 string pair (key-value).
    Utf8StringPair,
    /// Binary data.
    BinaryData,
}

impl PropertyType {
    /// Skips the wire representation of a property value in `buf`.
    pub fn skip_value(self, buf: &mut &[u8]) -> Result<(), crate::MqttError> {
        use crate::MqttError;
        use bytes::Buf;

        match self {
            PropertyType::Byte => {
                if buf.is_empty() {
                    return Err(MqttError::incomplete(1, 0));
                }
                buf.advance(1);
            }
            PropertyType::TwoByteInteger => {
                if buf.len() < 2 {
                    return Err(MqttError::incomplete(2, buf.len()));
                }
                buf.advance(2);
            }
            PropertyType::FourByteInteger => {
                if buf.len() < 4 {
                    return Err(MqttError::incomplete(4, buf.len()));
                }
                buf.advance(4);
            }
            PropertyType::VariableByteInteger => {
                skip_variable_byte_integer(buf)?;
            }
            PropertyType::Utf8String => {
                skip_utf8_string(buf)?;
            }
            PropertyType::Utf8StringPair => {
                skip_utf8_string(buf)?;
                skip_utf8_string(buf)?;
            }
            PropertyType::BinaryData => {
                if buf.len() < 2 {
                    return Err(MqttError::incomplete(2, buf.len()));
                }
                let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
                if buf.len() < 2 + len {
                    return Err(MqttError::incomplete(2 + len, buf.len()));
                }
                buf.advance(2 + len);
            }
        }
        Ok(())
    }

    /// Returns the data type for a given property identifier.
    ///
    /// # Arguments
    ///
    /// * `id` - The property identifier byte.
    ///
    /// # Returns
    ///
    /// * `Some(PropertyType)` - The corresponding data type.
    /// * `None` - If the identifier is not recognized.
    pub fn from_id(id: u8) -> Option<PropertyType> {
        match id {
            PROPERTY_PAYLOAD_FORMAT_INDICATOR => Some(PropertyType::Byte),
            PROPERTY_MESSAGE_EXPIRY_INTERVAL => Some(PropertyType::FourByteInteger),
            PROPERTY_CONTENT_TYPE => Some(PropertyType::Utf8String),
            PROPERTY_RESPONSE_TOPIC => Some(PropertyType::Utf8String),
            PROPERTY_CORRELATION_DATA => Some(PropertyType::BinaryData),
            PROPERTY_SUBSCRIPTION_IDENTIFIER => Some(PropertyType::VariableByteInteger),
            PROPERTY_SESSION_EXPIRY_INTERVAL => Some(PropertyType::FourByteInteger),
            PROPERTY_ASSIGNED_CLIENT_IDENTIFIER => Some(PropertyType::Utf8String),
            PROPERTY_SERVER_KEEP_ALIVE => Some(PropertyType::TwoByteInteger),
            PROPERTY_AUTHENTICATION_METHOD => Some(PropertyType::Utf8String),
            PROPERTY_AUTHENTICATION_DATA => Some(PropertyType::BinaryData),
            PROPERTY_REQUEST_PROBLEM_INFORMATION => Some(PropertyType::Byte),
            PROPERTY_WILL_DELAY_INTERVAL => Some(PropertyType::FourByteInteger),
            PROPERTY_REQUEST_RESPONSE_INFORMATION => Some(PropertyType::Byte),
            PROPERTY_RESPONSE_INFORMATION => Some(PropertyType::Utf8String),
            PROPERTY_SERVER_REFERENCE => Some(PropertyType::Utf8String),
            PROPERTY_REASON_STRING => Some(PropertyType::Utf8String),
            PROPERTY_RECEIVE_MAXIMUM => Some(PropertyType::TwoByteInteger),
            PROPERTY_TOPIC_ALIAS_MAXIMUM => Some(PropertyType::TwoByteInteger),
            PROPERTY_TOPIC_ALIAS => Some(PropertyType::TwoByteInteger),
            PROPERTY_MAXIMUM_QOS => Some(PropertyType::Byte),
            PROPERTY_RETAIN_AVAILABLE => Some(PropertyType::Byte),
            PROPERTY_USER_PROPERTY => Some(PropertyType::Utf8StringPair),
            PROPERTY_MAXIMUM_PACKET_SIZE => Some(PropertyType::FourByteInteger),
            PROPERTY_WILDCARD_SUBSCRIPTION_AVAILABLE => Some(PropertyType::Byte),
            PROPERTY_SUBSCRIPTION_IDENTIFIER_AVAILABLE => Some(PropertyType::Byte),
            PROPERTY_SHARED_SUBSCRIPTION_AVAILABLE => Some(PropertyType::Byte),
            _ => None,
        }
    }
}

fn skip_variable_byte_integer(buf: &mut &[u8]) -> Result<(), crate::MqttError> {
    use crate::MqttError;
    use bytes::Buf;

    loop {
        if buf.is_empty() {
            return Err(MqttError::incomplete(1, 0));
        }
        let encoded_byte = buf[0];
        buf.advance(1);
        if (encoded_byte & 0x80) == 0 {
            return Ok(());
        }
    }
}

fn skip_utf8_string(buf: &mut &[u8]) -> Result<(), crate::MqttError> {
    use crate::MqttError;
    use bytes::Buf;

    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }
    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    if buf.len() < 2 + len {
        return Err(MqttError::incomplete(2 + len, buf.len()));
    }
    buf.advance(2 + len);
    Ok(())
}
