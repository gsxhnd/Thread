//! MQTT v5.0 packet types and structures.
//!
//! This module defines all the packet types used in MQTT v5.0 protocol,
//! including their structures, properties, and reason codes.

use bytes::Bytes;

// ============================================================================
// Packet Type
// ============================================================================

/// MQTT v5.0 control packet types.
///
/// Each type corresponds to a specific MQTT control packet as defined
/// in the MQTT v5.0 specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    /// Connection request (client to server).
    Connect = 1,
    /// Connection acknowledgment (server to client).
    ConnAck = 2,
    /// Publish message.
    Publish = 3,
    /// Publish acknowledgment (QoS 1).
    PubAck = 4,
    /// Publish received (QoS 2).
    PubRec = 5,
    /// Publish release (QoS 2).
    PubRel = 6,
    /// Publish complete (QoS 2).
    PubComp = 7,
    /// Subscribe request.
    Subscribe = 8,
    /// Subscribe acknowledgment.
    SubAck = 9,
    /// Unsubscribe request.
    Unsubscribe = 10,
    /// Unsubscribe acknowledgment.
    UnsubAck = 11,
    /// PING request.
    PingReq = 12,
    /// PING response.
    PingResp = 13,
    /// Disconnect notification.
    Disconnect = 14,
    /// Authentication exchange.
    Auth = 15,
}

impl PacketType {
    /// Returns the packet type from a numeric value.
    ///
    /// # Arguments
    ///
    /// * `value` - The numeric value representing the packet type.
    ///
    /// # Returns
    ///
    /// * `Some(PacketType)` - If the value is a valid MQTT v5.0 packet type.
    /// * `None` - If the value is not a valid packet type.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(PacketType::Connect),
            2 => Some(PacketType::ConnAck),
            3 => Some(PacketType::Publish),
            4 => Some(PacketType::PubAck),
            5 => Some(PacketType::PubRec),
            6 => Some(PacketType::PubRel),
            7 => Some(PacketType::PubComp),
            8 => Some(PacketType::Subscribe),
            9 => Some(PacketType::SubAck),
            10 => Some(PacketType::Unsubscribe),
            11 => Some(PacketType::UnsubAck),
            12 => Some(PacketType::PingReq),
            13 => Some(PacketType::PingResp),
            14 => Some(PacketType::Disconnect),
            15 => Some(PacketType::Auth),
            _ => None,
        }
    }
}

// ============================================================================
// QoS
// ============================================================================

/// Quality of Service levels for MQTT messages.
///
/// QoS levels determine the guarantee of message delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum QoS {
    /// At most once delivery - fire and forget.
    AtMostOnce = 0,
    /// At least once delivery - acknowledgment required.
    AtLeastOnce = 1,
    /// Exactly once delivery - two-phase commit.
    #[default]
    ExactlyOnce = 2,
}

impl TryFrom<u8> for QoS {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(QoS::AtMostOnce),
            1 => Ok(QoS::AtLeastOnce),
            2 => Ok(QoS::ExactlyOnce),
            _ => Err("Invalid QoS value: must be 0, 1, or 2"),
        }
    }
}

impl From<QoS> for u8 {
    fn from(qos: QoS) -> Self {
        qos as u8
    }
}

// ============================================================================
// Reason Code
// ============================================================================

/// MQTT v5.0 reason codes.
///
/// Reason codes are used in various packet types to indicate the
/// result of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReasonCode {
    // Success codes
    /// Success (also means Granted QoS 0 for SUBACK).
    Success = 0x00,
    /// Granted QoS 1 (SUBACK).
    GrantedQoS1 = 0x01,
    /// Granted QoS 2 (SUBACK).
    GrantedQoS2 = 0x02,
    /// Disconnect with Will Message (DISCONNECT).
    DisconnectWithWillMessage = 0x04,
    /// No matching subscribers (PUBACK/PUBREC).
    NoMatchingSubscribers = 0x10,
    /// Continue authentication (AUTH).
    ContinueAuthentication = 0x18,
    /// Re-authenticate (AUTH).
    ReAuthenticate = 0x19,

    // Error codes
    /// Unspecified error.
    UnspecifiedError = 0x80,
    /// Malformed packet.
    MalformedPacket = 0x81,
    /// Protocol error.
    ProtocolError = 0x82,
    /// Implementation specific error.
    ImplementationSpecificError = 0x83,
    /// Unsupported protocol version.
    UnsupportedProtocolVersion = 0x84,
    /// Client identifier not valid.
    ClientIdentifierNotValid = 0x85,
    /// Bad user name or password.
    BadUserNameOrPassword = 0x86,
    /// Not authorized.
    NotAuthorized = 0x87,
    /// Server unavailable.
    ServerUnavailable = 0x88,
    /// Server busy.
    ServerBusy = 0x89,
    /// Banned.
    Banned = 0x8A,
    /// Server shutting down.
    ServerShuttingDown = 0x8B,
    /// Bad authentication method.
    BadAuthenticationMethod = 0x8C,
    /// Keep alive timeout.
    KeepAliveTimeout = 0x8D,
    /// Session taken over.
    SessionTakenOver = 0x8E,
    /// Topic filter invalid.
    TopicFilterInvalid = 0x8F,
    /// Topic name invalid.
    TopicNameInvalid = 0x90,
    /// Packet identifier in use.
    PacketIdentifierInUse = 0x91,
    /// Packet identifier not found.
    PacketIdentifierNotFound = 0x92,
    /// Receive maximum exceeded.
    ReceiveMaximumExceeded = 0x93,
    /// Topic alias invalid.
    TopicAliasInvalid = 0x94,
    /// Frame too large.
    FrameTooLarge = 0x95,
    /// Message rate too high.
    MessageRateTooHigh = 0x96,
    /// Quota exceeded.
    QuotaExceeded = 0x97,
    /// Administrative action.
    AdministrativeAction = 0x98,
    /// Payload format invalid.
    PayloadFormatInvalid = 0x99,
    /// Retain not supported.
    RetainNotSupported = 0x9A,
    /// QoS not supported.
    QoSNotSupported = 0x9B,
    /// Use another server.
    UseAnotherServer = 0x9C,
    /// Server moved.
    ServerMoved = 0x9D,
    /// Shared subscription not supported.
    SharedSubscriptionNotSupported = 0x9E,
    /// Connection rate exceeded.
    ConnectionRateExceeded = 0x9F,
    /// Maximum connect time.
    MaximumConnectTime = 0xA0,
    /// Subscription identifiers not supported.
    SubscriptionIdentifiersNotSupported = 0xA1,
    /// Wildcard subscriptions not supported.
    WildcardSubscriptionsNotSupported = 0xA2,
}

impl ReasonCode {
    /// Returns the byte value of this reason code.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns true if this reason code indicates success.
    pub fn is_success(self) -> bool {
        matches!(
            self,
            ReasonCode::Success
                | ReasonCode::GrantedQoS1
                | ReasonCode::GrantedQoS2
                | ReasonCode::DisconnectWithWillMessage
                | ReasonCode::NoMatchingSubscribers
                | ReasonCode::ContinueAuthentication
                | ReasonCode::ReAuthenticate
        )
    }

    /// Returns true if this reason code indicates an error.
    pub fn is_error(self) -> bool {
        (self as u8) >= 0x80 && !self.is_success()
    }
}

impl TryFrom<u8> for ReasonCode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(ReasonCode::Success),
            0x01 => Ok(ReasonCode::GrantedQoS1),
            0x02 => Ok(ReasonCode::GrantedQoS2),
            0x04 => Ok(ReasonCode::DisconnectWithWillMessage),
            0x10 => Ok(ReasonCode::NoMatchingSubscribers),
            0x18 => Ok(ReasonCode::ContinueAuthentication),
            0x19 => Ok(ReasonCode::ReAuthenticate),
            0x80 => Ok(ReasonCode::UnspecifiedError),
            0x81 => Ok(ReasonCode::MalformedPacket),
            0x82 => Ok(ReasonCode::ProtocolError),
            0x83 => Ok(ReasonCode::ImplementationSpecificError),
            0x84 => Ok(ReasonCode::UnsupportedProtocolVersion),
            0x85 => Ok(ReasonCode::ClientIdentifierNotValid),
            0x86 => Ok(ReasonCode::BadUserNameOrPassword),
            0x87 => Ok(ReasonCode::NotAuthorized),
            0x88 => Ok(ReasonCode::ServerUnavailable),
            0x89 => Ok(ReasonCode::ServerBusy),
            0x8A => Ok(ReasonCode::Banned),
            0x8B => Ok(ReasonCode::ServerShuttingDown),
            0x8C => Ok(ReasonCode::BadAuthenticationMethod),
            0x8D => Ok(ReasonCode::KeepAliveTimeout),
            0x8E => Ok(ReasonCode::SessionTakenOver),
            0x8F => Ok(ReasonCode::TopicFilterInvalid),
            0x90 => Ok(ReasonCode::TopicNameInvalid),
            0x91 => Ok(ReasonCode::PacketIdentifierInUse),
            0x92 => Ok(ReasonCode::PacketIdentifierNotFound),
            0x93 => Ok(ReasonCode::ReceiveMaximumExceeded),
            0x94 => Ok(ReasonCode::TopicAliasInvalid),
            0x95 => Ok(ReasonCode::FrameTooLarge),
            0x96 => Ok(ReasonCode::MessageRateTooHigh),
            0x97 => Ok(ReasonCode::QuotaExceeded),
            0x98 => Ok(ReasonCode::AdministrativeAction),
            0x99 => Ok(ReasonCode::PayloadFormatInvalid),
            0x9A => Ok(ReasonCode::RetainNotSupported),
            0x9B => Ok(ReasonCode::QoSNotSupported),
            0x9C => Ok(ReasonCode::UseAnotherServer),
            0x9D => Ok(ReasonCode::ServerMoved),
            0x9E => Ok(ReasonCode::SharedSubscriptionNotSupported),
            0x9F => Ok(ReasonCode::ConnectionRateExceeded),
            0xA0 => Ok(ReasonCode::MaximumConnectTime),
            0xA1 => Ok(ReasonCode::SubscriptionIdentifiersNotSupported),
            0xA2 => Ok(ReasonCode::WildcardSubscriptionsNotSupported),
            other => Err(other),
        }
    }
}

impl From<ReasonCode> for u8 {
    fn from(code: ReasonCode) -> Self {
        code.as_u8()
    }
}

// ============================================================================
// Properties
// ============================================================================

/// MQTT v5.0 Properties.
///
/// Properties are key-value pairs that can be included in various
/// MQTT v5.0 packet types to provide additional metadata.
#[derive(Debug, Clone, Default)]
pub struct Properties {
    // ----------------------------------------------------------------
    // Session and Connection Properties
    // ----------------------------------------------------------------
    /// Session expiry interval in seconds (0x11).
    ///
    /// Used in: CONNECT, CONNACK, DISCONNECT.
    pub session_expiry_interval: Option<u32>,

    /// Receive maximum (0x21).
    ///
    /// The maximum number of QoS 1 and QoS 2 publications that can be
    /// processed concurrently.
    ///
    /// Used in: CONNECT, CONNACK.
    pub receive_maximum: Option<u16>,

    /// Maximum packet size (0x27).
    ///
    /// The maximum packet size the client or server is willing to accept.
    ///
    /// Used in: CONNECT, CONNACK.
    pub maximum_packet_size: Option<u32>,

    /// Topic alias maximum (0x22).
    ///
    /// The maximum value for a topic alias.
    ///
    /// Used in: CONNECT, CONNACK.
    pub topic_alias_maximum: Option<u16>,

    /// Request response information (0x19).
    ///
    /// Whether the client requests response information.
    ///
    /// Used in: CONNECT.
    pub request_response_information: Option<bool>,

    /// Response information (0x1A).
    ///
    /// A string returned by the server for request-response.
    ///
    /// Used in: CONNACK.
    pub response_information: Option<String>,

    /// Request problem information (0x17).
    ///
    /// Whether the client wants the server to return problem information.
    ///
    /// Used in: CONNECT.
    pub request_problem_information: Option<bool>,

    // ----------------------------------------------------------------
    // Server Properties
    // ----------------------------------------------------------------
    /// Assigned client identifier (0x12).
    ///
    /// A client identifier assigned by the server.
    ///
    /// Used in: CONNACK.
    pub assigned_client_identifier: Option<String>,

    /// Server keep alive (0x13).
    ///
    /// Keep alive value assigned by the server.
    ///
    /// Used in: CONNACK.
    pub server_keep_alive: Option<u16>,

    /// Maximum QoS (0x24).
    ///
    /// The maximum QoS level supported by the server.
    ///
    /// Used in: CONNACK.
    pub maximum_qos: Option<QoS>,

    /// Retain available (0x25).
    ///
    /// Whether the server supports retained messages.
    ///
    /// Used in: CONNACK.
    pub retain_available: Option<bool>,

    /// Wildcard subscription available (0x28).
    ///
    /// Used in: CONNACK.
    pub wildcard_subscription_available: Option<bool>,

    /// Subscription identifiers available (0x29).
    ///
    /// Used in: CONNACK.
    pub subscription_identifiers_available: Option<bool>,

    /// Shared subscription available (0x2A).
    ///
    /// Used in: CONNACK.
    pub shared_subscription_available: Option<bool>,

    // ----------------------------------------------------------------
    // Authentication Properties
    // ----------------------------------------------------------------
    /// Authentication method (0x15).
    ///
    /// The name of the authentication method.
    ///
    /// Used in: CONNECT, CONNACK, AUTH.
    pub authentication_method: Option<String>,

    /// Authentication data (0x16).
    ///
    /// Binary data for authentication.
    ///
    /// Used in: CONNECT, CONNACK, AUTH.
    pub authentication_data: Option<Bytes>,

    // ----------------------------------------------------------------
    // Will Properties
    // ----------------------------------------------------------------
    /// Will delay interval (0x18).
    ///
    /// The delay before publishing the will message.
    ///
    /// Used in: Will Properties.
    pub will_delay_interval: Option<u32>,

    // ----------------------------------------------------------------
    // Publish Properties
    // ----------------------------------------------------------------
    /// Payload format indicator (0x01).
    ///
    /// 0: Unspecified byte stream, 1: UTF-8 encoded.
    ///
    /// Used in: PUBLISH, Will Properties.
    pub payload_format_indicator: Option<u8>,

    /// Message expiry interval (0x02).
    ///
    /// The lifetime of the message in seconds.
    ///
    /// Used in: PUBLISH, Will Properties.
    pub message_expiry_interval: Option<u32>,

    /// Content type (0x03).
    ///
    /// A string describing the content type.
    ///
    /// Used in: PUBLISH, Will Properties.
    pub content_type: Option<String>,

    /// Response topic (0x08).
    ///
    /// The topic for response messages.
    ///
    /// Used in: PUBLISH, Will Properties.
    pub response_topic: Option<String>,

    /// Correlation data (0x09).
    ///
    /// Binary data for correlating request and response.
    ///
    /// Used in: PUBLISH, Will Properties.
    pub correlation_data: Option<Bytes>,

    /// Topic alias (0x23).
    ///
    /// An integer identifying the topic.
    ///
    /// Used in: PUBLISH.
    pub topic_alias: Option<u16>,

    /// Subscription identifiers (0x0B).
    ///
    /// Identifiers for subscriptions.
    ///
    /// Used in: PUBLISH, SUBSCRIBE.
    pub subscription_identifiers: Vec<u32>,

    // ----------------------------------------------------------------
    // General Properties
    // ----------------------------------------------------------------
    /// Reason string (0x1F).
    ///
    /// A human-readable string describing the reason.
    ///
    /// Used in: All packets except PUBLISH.
    pub reason_string: Option<String>,

    /// User properties (0x26).
    ///
    /// Key-value pairs for application-defined properties.
    /// Stored as a Vec to allow duplicate keys per MQTT v5.0 spec.
    ///
    /// Used in: All packets.
    pub user_properties: Vec<(String, String)>,

    // ----------------------------------------------------------------
    // Server Reference Properties
    // ----------------------------------------------------------------
    /// Server reference (0x1C).
    ///
    /// An alternative server to use.
    ///
    /// Used in: CONNACK, DISCONNECT.
    pub server_reference: Option<String>,
}

impl Properties {
    /// Creates a new empty `Properties` struct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if there are no properties set.
    pub fn is_empty(&self) -> bool {
        self.session_expiry_interval.is_none()
            && self.receive_maximum.is_none()
            && self.maximum_packet_size.is_none()
            && self.topic_alias_maximum.is_none()
            && self.request_response_information.is_none()
            && self.response_information.is_none()
            && self.request_problem_information.is_none()
            && self.assigned_client_identifier.is_none()
            && self.server_keep_alive.is_none()
            && self.maximum_qos.is_none()
            && self.retain_available.is_none()
            && self.wildcard_subscription_available.is_none()
            && self.subscription_identifiers_available.is_none()
            && self.shared_subscription_available.is_none()
            && self.authentication_method.is_none()
            && self.authentication_data.is_none()
            && self.will_delay_interval.is_none()
            && self.payload_format_indicator.is_none()
            && self.message_expiry_interval.is_none()
            && self.content_type.is_none()
            && self.response_topic.is_none()
            && self.correlation_data.is_none()
            && self.topic_alias.is_none()
            && self.subscription_identifiers.is_empty()
            && self.reason_string.is_none()
            && self.user_properties.is_empty()
            && self.server_reference.is_none()
    }
}

// ============================================================================
// Packets
// ============================================================================

/// MQTT v5.0 CONNECT packet.
///
/// Sent by a client to request a connection to a server.
#[derive(Debug, Clone)]
pub struct ConnectPacket {
    /// Protocol name (must be "MQTT").
    pub protocol_name: String,
    /// Protocol version (must be 5).
    pub protocol_level: u8,
    /// Clean start flag.
    pub clean_start: bool,
    /// Will message flag.
    pub will_flag: bool,
    /// Will message QoS level.
    pub will_qos: QoS,
    /// Will message retain flag.
    pub will_retain: bool,
    /// Password flag.
    pub password_flag: bool,
    /// Username flag.
    pub username_flag: bool,
    /// Keep alive timeout in seconds.
    pub keep_alive: u16,
    /// Connection properties.
    pub properties: Properties,
    /// Client identifier.
    pub client_id: String,
    /// Will topic.
    pub will_topic: Option<String>,
    /// Will message payload.
    pub will_message: Option<Bytes>,
    /// Will properties.
    pub will_properties: Option<Properties>,
    /// Username for authentication.
    pub username: Option<String>,
    /// Password for authentication.
    pub password: Option<Bytes>,
}

impl Default for ConnectPacket {
    fn default() -> Self {
        ConnectPacket {
            protocol_name: "MQTT".to_string(),
            protocol_level: 5,
            clean_start: true,
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
            password_flag: false,
            username_flag: false,
            keep_alive: 60,
            properties: Properties::new(),
            client_id: String::new(),
            will_topic: None,
            will_message: None,
            will_properties: None,
            username: None,
            password: None,
        }
    }
}

/// MQTT v5.0 CONNACK packet.
///
/// Sent by the server in response to a CONNECT packet.
#[derive(Debug, Clone)]
pub struct ConnAckPacket {
    /// Session present flag.
    pub session_present: bool,
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Connection properties.
    pub properties: Properties,
}

impl ConnAckPacket {
    /// Creates a successful CONNACK packet.
    ///
    /// # Arguments
    ///
    /// * `session_present` - Whether a session was already present on the server.
    ///
    /// # Returns
    ///
    /// A new `ConnAckPacket` with `ReasonCode::Success`.
    pub fn accepted(session_present: bool) -> Self {
        ConnAckPacket {
            session_present,
            reason_code: ReasonCode::Success,
            properties: Properties::new(),
        }
    }
}

/// MQTT v5.0 PUBLISH packet.
///
/// Used to transport an application message from a client to a server, or vice versa.
#[derive(Debug, Clone)]
pub struct PublishPacket {
    /// Topic name.
    pub topic_name: String,
    /// Packet identifier (required for QoS > 0).
    pub packet_id: Option<u16>,
    /// Message payload.
    pub payload: Bytes,
    /// Quality of Service level.
    pub qos: QoS,
    /// Duplicate delivery flag.
    pub duplicate: bool,
    /// Message retention flag.
    pub retain: bool,
    /// Message properties.
    pub properties: Properties,
}

impl PublishPacket {
    /// Creates a new PUBLISH packet with QoS 0.
    ///
    /// # Arguments
    ///
    /// * `topic_name` - The topic to publish to.
    /// * `payload` - The message payload.
    ///
    /// # Returns
    ///
    /// A new `PublishPacket` with default settings and QoS 0.
    pub fn new(topic_name: impl Into<String>, payload: impl Into<Bytes>) -> Self {
        PublishPacket {
            topic_name: topic_name.into(),
            packet_id: None,
            payload: payload.into(),
            qos: QoS::AtMostOnce,
            duplicate: false,
            retain: false,
            properties: Properties::new(),
        }
    }
}

/// MQTT v5.0 PUBACK packet.
///
/// Sent in response to a PUBLISH packet with QoS 1.
#[derive(Debug, Clone)]
pub struct PubAckPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

/// MQTT v5.0 PUBREC packet.
///
/// Sent in response to a PUBLISH packet with QoS 2.
#[derive(Debug, Clone)]
pub struct PubRecPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

/// MQTT v5.0 PUBREL packet.
///
/// Sent in response to a PUBREC packet.
#[derive(Debug, Clone)]
pub struct PubRelPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

/// MQTT v5.0 PUBCOMP packet.
///
/// Sent in response to a PUBREL packet.
#[derive(Debug, Clone)]
pub struct PubCompPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

/// MQTT v5.0 SUBSCRIBE packet.
///
/// Sent by a client to create one or more subscriptions.
#[derive(Debug, Clone)]
pub struct SubscribePacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Subscription properties.
    pub properties: Properties,
    /// Subscription options.
    pub topics: Vec<SubscriptionOption>,
}

/// MQTT v5.0 subscription option.
///
/// Defines the behavior of a single subscription.
#[derive(Debug, Clone)]
pub struct SubscriptionOption {
    /// Topic filter.
    pub topic_filter: String,
    /// Requested QoS level.
    pub qos: QoS,
    /// No local flag (don't receive own publications).
    pub no_local: bool,
    /// Retain as published flag.
    pub retain_as_published: bool,
    /// Retain handling (0, 1, or 2).
    pub retain_handling: u8,
}

/// MQTT v5.0 SUBACK packet.
///
/// Sent by the server to acknowledge a SUBSCRIBE packet.
#[derive(Debug, Clone)]
pub struct SubAckPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Properties.
    pub properties: Properties,
    /// Reason codes for each subscription in the SUBSCRIBE packet.
    pub reason_codes: Vec<ReasonCode>,
}

/// MQTT v5.0 UNSUBSCRIBE packet.
///
/// Sent by a client to cancel existing subscriptions.
#[derive(Debug, Clone)]
pub struct UnsubscribePacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Properties.
    pub properties: Properties,
    /// Topic filters to unsubscribe from.
    pub topics: Vec<String>,
}

/// MQTT v5.0 UNSUBACK packet.
///
/// Sent by the server to acknowledge an UNSUBSCRIBE packet.
#[derive(Debug, Clone)]
pub struct UnsubAckPacket {
    /// Packet identifier.
    pub packet_id: u16,
    /// Properties.
    pub properties: Properties,
    /// Reason codes for each unsubscription in the UNSUBSCRIBE packet.
    pub reason_codes: Vec<ReasonCode>,
}

/// MQTT v5.0 PINGREQ packet.
///
/// Sent by a client to indicate it is still alive.
#[derive(Debug, Clone, Default)]
pub struct PingReqPacket;

/// MQTT v5.0 PINGRESP packet.
///
/// Sent by the server to acknowledge a PINGREQ packet.
#[derive(Debug, Clone, Default)]
pub struct PingRespPacket;

/// MQTT v5.0 DISCONNECT packet.
///
/// Sent by either the client or the server to indicate the connection is closing.
#[derive(Debug, Clone)]
pub struct DisconnectPacket {
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

impl Default for DisconnectPacket {
    fn default() -> Self {
        DisconnectPacket {
            reason_code: ReasonCode::Success,
            properties: Properties::new(),
        }
    }
}

/// MQTT v5.0 AUTH packet.
///
/// Used for extended authentication exchange between client and server.
#[derive(Debug, Clone)]
pub struct AuthPacket {
    /// Reason code.
    pub reason_code: ReasonCode,
    /// Properties.
    pub properties: Properties,
}

impl Default for AuthPacket {
    fn default() -> Self {
        AuthPacket {
            reason_code: ReasonCode::Success,
            properties: Properties::new(),
        }
    }
}

/// MQTT v5.0 Packet enum containing all possible packet types.
#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "CONNECT carries full Properties; boxing would break the public packet API"
)]
pub enum Packet {
    /// Connection request.
    Connect(ConnectPacket),
    /// Connection acknowledgment.
    ConnAck(ConnAckPacket),
    /// Publish message.
    Publish(PublishPacket),
    /// Publish acknowledgment.
    PubAck(PubAckPacket),
    /// Publish received.
    PubRec(PubRecPacket),
    /// Publish release.
    PubRel(PubRelPacket),
    /// Publish complete.
    PubComp(PubCompPacket),
    /// Subscribe request.
    Subscribe(SubscribePacket),
    /// Subscribe acknowledgment.
    SubAck(SubAckPacket),
    /// Unsubscribe request.
    Unsubscribe(UnsubscribePacket),
    /// Unsubscribe acknowledgment.
    UnsubAck(UnsubAckPacket),
    /// PING request.
    PingReq(PingReqPacket),
    /// PING response.
    PingResp(PingRespPacket),
    /// Disconnect notification.
    Disconnect(DisconnectPacket),
    /// Authentication exchange.
    Auth(AuthPacket),
}

impl Packet {
    /// Returns the packet type for this packet.
    ///
    /// # Returns
    ///
    /// The `PacketType` corresponding to the current enum variant.
    pub fn packet_type(&self) -> PacketType {
        match self {
            Packet::Connect(_) => PacketType::Connect,
            Packet::ConnAck(_) => PacketType::ConnAck,
            Packet::Publish(_) => PacketType::Publish,
            Packet::PubAck(_) => PacketType::PubAck,
            Packet::PubRec(_) => PacketType::PubRec,
            Packet::PubRel(_) => PacketType::PubRel,
            Packet::PubComp(_) => PacketType::PubComp,
            Packet::Subscribe(_) => PacketType::Subscribe,
            Packet::SubAck(_) => PacketType::SubAck,
            Packet::Unsubscribe(_) => PacketType::Unsubscribe,
            Packet::UnsubAck(_) => PacketType::UnsubAck,
            Packet::PingReq(_) => PacketType::PingReq,
            Packet::PingResp(_) => PacketType::PingResp,
            Packet::Disconnect(_) => PacketType::Disconnect,
            Packet::Auth(_) => PacketType::Auth,
        }
    }
}
