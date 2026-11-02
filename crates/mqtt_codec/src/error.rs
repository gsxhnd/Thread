//! Error types for the MQTT codec library.
//!
//! This module provides a comprehensive set of error types for handling
//! various error conditions that may occur during MQTT packet encoding
//! and decoding operations.

use std::fmt;

/// The main error type for MQTT codec operations.
///
/// This enum covers all possible error conditions that can occur
/// during encoding, decoding, and validation of MQTT packets.
#[derive(Debug, thiserror::Error)]
pub enum MqttError {
    /// Protocol violation error.
    ///
    /// This error indicates that the packet does not conform to the MQTT
    /// protocol specification. The `message` field provides details about
    /// the specific violation, and `packet_type` indicates the packet type
    /// if available.
    #[error("Protocol violation: {message}")]
    ProtocolViolation {
        /// Human-readable description of the violation.
        message: String,
        /// The packet type where the violation occurred, if known.
        packet_type: Option<u8>,
    },

    /// Invalid client identifier error.
    ///
    /// This error occurs when a client ID does not meet the requirements
    /// specified in the MQTT protocol.
    #[error("Invalid client ID '{client_id}': {reason}")]
    InvalidClientId {
        /// The invalid client identifier.
        client_id: String,
        /// The reason why the client ID is invalid.
        reason: ClientIdErrorReason,
    },

    /// Invalid topic name error.
    ///
    /// This error occurs when a topic name does not meet the requirements
    /// for publish operations.
    #[error("Invalid topic name '{topic}': {reason}")]
    InvalidTopicName {
        /// The invalid topic name.
        topic: String,
        /// The reason why the topic name is invalid.
        reason: TopicErrorReason,
    },

    /// Invalid topic filter error.
    ///
    /// This error occurs when a topic filter does not meet the requirements
    /// for subscribe/unsubscribe operations.
    #[error("Invalid topic filter '{filter}': {reason}")]
    InvalidTopicFilter {
        /// The invalid topic filter.
        filter: String,
        /// The reason why the topic filter is invalid.
        reason: TopicFilterErrorReason,
    },

    /// Invalid return code error (MQTT v3.1.1).
    ///
    /// This error occurs when a return code value is not valid for
    /// the given context in MQTT v3.1.1.
    #[error("Invalid return code: 0x{code:02X}")]
    InvalidReturnCode {
        /// The invalid return code value.
        code: u8,
    },

    /// Invalid reason code error (MQTT v5.0).
    ///
    /// This error occurs when a reason code value is not valid for
    /// the given packet type in MQTT v5.0.
    #[error("Invalid reason code 0x{code:02X} for {context}")]
    InvalidReasonCode {
        /// The invalid reason code value.
        code: u8,
        /// The context (packet type) where the error occurred.
        context: String,
    },

    /// Invalid property error (MQTT v5.0).
    ///
    /// This error occurs when a property is not valid for the given
    /// packet type or has an invalid value.
    #[error("Invalid property 0x{property_id:02X}: {reason}")]
    InvalidProperty {
        /// The property identifier.
        property_id: u8,
        /// The reason why the property is invalid.
        reason: String,
    },

    /// Malformed packet error.
    ///
    /// This error occurs when the packet structure is invalid or
    /// cannot be parsed correctly.
    #[error("Malformed packet: {message}")]
    MalformedPacket {
        /// Human-readable description of the malformation.
        message: String,
    },

    /// Invalid remaining length error.
    ///
    /// This error occurs when the remaining length encoding is invalid
    /// or exceeds the maximum allowed value.
    #[error("Invalid remaining length: {length}")]
    InvalidRemainingLength {
        /// The invalid length value.
        length: usize,
    },

    /// Incomplete packet error.
    ///
    /// This error occurs when attempting to decode a packet that
    /// does not have enough bytes.
    #[error("Incomplete packet: expected {expected} bytes, got {actual}")]
    IncompletePacket {
        /// The expected number of bytes.
        expected: usize,
        /// The actual number of bytes available.
        actual: usize,
    },

    /// Packet too large error.
    ///
    /// This error occurs when a packet exceeds the negotiated
    /// Maximum Packet Size (MQTT v5.0).
    #[error("Packet too large: {size} bytes exceeds maximum of {max_size} bytes")]
    PacketTooLarge {
        /// The actual packet size in bytes.
        size: usize,
        /// The maximum allowed packet size in bytes.
        max_size: usize,
    },

    /// UTF-8 encoding error.
    ///
    /// This error wraps a UTF-8 parsing error.
    #[error("UTF-8 encoding error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// String encoding error.
    ///
    /// This error occurs when converting bytes to a String fails.
    #[error("String encoding error: {0}")]
    StringUtf8Error(#[from] std::string::FromUtf8Error),

    /// I/O error.
    ///
    /// This error wraps a standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error with a message.
    ///
    /// This is a catch-all error type for miscellaneous errors.
    #[error("{0}")]
    Other(String),
}

/// Reasons why a client ID is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientIdErrorReason {
    /// The client ID exceeds the maximum allowed length (65535 bytes).
    TooLong,
    /// The client ID contains characters that are not allowed.
    InvalidCharacters,
    /// The client ID is empty but Clean Session is set to 0.
    EmptyWithNonCleanSession,
}

impl fmt::Display for ClientIdErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientIdErrorReason::TooLong => write!(f, "exceeds maximum length of 65535 bytes"),
            ClientIdErrorReason::InvalidCharacters => {
                write!(f, "contains invalid characters (must be UTF-8)")
            }
            ClientIdErrorReason::EmptyWithNonCleanSession => {
                write!(f, "empty client ID requires Clean Session = 1")
            }
        }
    }
}

/// Reasons why a topic name is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicErrorReason {
    /// The topic name is empty.
    Empty,
    /// The topic name contains a null character (U+0000).
    ContainsNull,
    /// The topic name contains wildcard characters (+ or #).
    ContainsWildcard,
    /// The topic name is not valid UTF-8.
    InvalidUtf8,
    /// The topic name exceeds the maximum length.
    TooLong,
}

impl fmt::Display for TopicErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopicErrorReason::Empty => write!(f, "topic name cannot be empty"),
            TopicErrorReason::ContainsNull => {
                write!(f, "topic name cannot contain null character (U+0000)")
            }
            TopicErrorReason::ContainsWildcard => {
                write!(f, "topic name cannot contain wildcards (+ or #)")
            }
            TopicErrorReason::InvalidUtf8 => write!(f, "topic name is not valid UTF-8"),
            TopicErrorReason::TooLong => write!(f, "topic name exceeds maximum length"),
        }
    }
}

/// Reasons why a topic filter is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicFilterErrorReason {
    /// The topic filter is empty.
    Empty,
    /// The topic filter contains a null character (U+0000).
    ContainsNull,
    /// The multi-level wildcard (#) is not at the end.
    InvalidMultiLevelWildcard,
    /// The single-level wildcard (+) is not used correctly.
    InvalidSingleLevelWildcard,
    /// The topic filter is not valid UTF-8.
    InvalidUtf8,
    /// The topic filter exceeds the maximum length.
    TooLong,
}

impl fmt::Display for TopicFilterErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopicFilterErrorReason::Empty => write!(f, "topic filter cannot be empty"),
            TopicFilterErrorReason::ContainsNull => {
                write!(f, "topic filter cannot contain null character (U+0000)")
            }
            TopicFilterErrorReason::InvalidMultiLevelWildcard => {
                write!(f, "multi-level wildcard (#) must be the last character")
            }
            TopicFilterErrorReason::InvalidSingleLevelWildcard => {
                write!(f, "single-level wildcard (+) must occupy an entire level")
            }
            TopicFilterErrorReason::InvalidUtf8 => write!(f, "topic filter is not valid UTF-8"),
            TopicFilterErrorReason::TooLong => write!(f, "topic filter exceeds maximum length"),
        }
    }
}

impl MqttError {
    /// Creates a new protocol violation error.
    ///
    /// # Arguments
    ///
    /// * `message` - A description of the protocol violation.
    /// * `packet_type` - The packet type where the violation occurred, if known.
    ///
    /// # Returns
    ///
    /// A new `MqttError::ProtocolViolation` instance.
    pub fn protocol_violation(message: impl Into<String>, packet_type: Option<u8>) -> Self {
        MqttError::ProtocolViolation {
            message: message.into(),
            packet_type,
        }
    }

    /// Creates a new malformed packet error.
    ///
    /// # Arguments
    ///
    /// * `message` - A description of the malformation.
    ///
    /// # Returns
    ///
    /// A new `MqttError::MalformedPacket` instance.
    pub fn malformed(message: impl Into<String>) -> Self {
        MqttError::MalformedPacket {
            message: message.into(),
        }
    }

    /// Creates a new incomplete packet error.
    ///
    /// # Arguments
    ///
    /// * `expected` - The expected number of bytes.
    /// * `actual` - The actual number of bytes available.
    ///
    /// # Returns
    ///
    /// A new `MqttError::IncompletePacket` instance.
    pub fn incomplete(expected: usize, actual: usize) -> Self {
        MqttError::IncompletePacket { expected, actual }
    }

    /// Creates a new invalid client ID error.
    ///
    /// # Arguments
    ///
    /// * `client_id` - The invalid client identifier.
    /// * `reason` - The reason why the client ID is invalid.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidClientId` instance.
    pub fn invalid_client_id(client_id: impl Into<String>, reason: ClientIdErrorReason) -> Self {
        MqttError::InvalidClientId {
            client_id: client_id.into(),
            reason,
        }
    }

    /// Creates a new invalid topic name error.
    ///
    /// # Arguments
    ///
    /// * `topic` - The invalid topic name.
    /// * `reason` - The reason why the topic name is invalid.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidTopicName` instance.
    pub fn invalid_topic_name(topic: impl Into<String>, reason: TopicErrorReason) -> Self {
        MqttError::InvalidTopicName {
            topic: topic.into(),
            reason,
        }
    }

    /// Creates a new invalid topic filter error.
    ///
    /// # Arguments
    ///
    /// * `filter` - The invalid topic filter.
    /// * `reason` - The reason why the topic filter is invalid.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidTopicFilter` instance.
    pub fn invalid_topic_filter(filter: impl Into<String>, reason: TopicFilterErrorReason) -> Self {
        MqttError::InvalidTopicFilter {
            filter: filter.into(),
            reason,
        }
    }

    /// Creates a new invalid return code error (MQTT v3.1.1).
    ///
    /// # Arguments
    ///
    /// * `code` - The invalid return code value.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidReturnCode` instance.
    pub fn invalid_return_code(code: u8) -> Self {
        MqttError::InvalidReturnCode { code }
    }

    /// Creates a new invalid reason code error (MQTT v5.0).
    ///
    /// # Arguments
    ///
    /// * `code` - The invalid reason code value.
    /// * `context` - The packet type where the error occurred.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidReasonCode` instance.
    pub fn invalid_reason_code(code: u8, context: impl Into<String>) -> Self {
        MqttError::InvalidReasonCode {
            code,
            context: context.into(),
        }
    }

    /// Creates a new invalid property error (MQTT v5.0).
    ///
    /// # Arguments
    ///
    /// * `property_id` - The property identifier.
    /// * `reason` - The reason why the property is invalid.
    ///
    /// # Returns
    ///
    /// A new `MqttError::InvalidProperty` instance.
    pub fn invalid_property(property_id: u8, reason: impl Into<String>) -> Self {
        MqttError::InvalidProperty {
            property_id,
            reason: reason.into(),
        }
    }

    /// Creates a new packet too large error.
    ///
    /// # Arguments
    ///
    /// * `size` - The actual packet size in bytes.
    /// * `max_size` - The maximum allowed packet size in bytes.
    ///
    /// # Returns
    ///
    /// A new `MqttError::PacketTooLarge` instance.
    pub fn packet_too_large(size: usize, max_size: usize) -> Self {
        MqttError::PacketTooLarge { size, max_size }
    }
}
