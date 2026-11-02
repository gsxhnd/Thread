//! MQTT v3.1.1 packet types and structures.
//!
//! This module defines all the packet types used in MQTT v3.1.1 protocol,
//! including their structures and related implementations.

use bytes::Bytes;

use super::return_codes::{ConnectReturnCode, SubAckReturnCode};

/// MQTT control packet types.
///
/// Each type corresponds to a specific MQTT control packet as defined
/// in the MQTT v3.1.1 specification.
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
    /// * `Some(PacketType)` - If the value is a valid packet type.
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
            _ => None,
        }
    }
}

/// Quality of Service levels for MQTT messages.
///
/// QoS levels determine the guarantee of message delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum QoS {
    /// At most once delivery - fire and forget.
    ///
    /// The message is delivered at most once, or it may not be delivered at all.
    /// This is the fastest but least reliable delivery mode.
    AtMostOnce = 0,

    /// At least once delivery - acknowledgment required.
    ///
    /// The message is guaranteed to be delivered at least once.
    /// Duplicates may occur if the acknowledgment is lost.
    AtLeastOnce = 1,

    /// Exactly once delivery - two-phase commit.
    ///
    /// The message is guaranteed to be delivered exactly once.
    /// This is the most reliable but slowest delivery mode.
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

/// MQTT CONNECT packet.
///
/// Contains protocol information and client credentials for establishing
/// a connection to an MQTT broker.
#[derive(Debug, Clone)]
pub struct ConnectPacket {
    /// Protocol name (must be "MQTT" for MQTT v3.1.1).
    pub protocol_name: String,
    /// Protocol version level (must be 4 for MQTT v3.1.1).
    pub protocol_level: u8,
    /// Clean session flag.
    ///
    /// If set to true, the broker starts a new session and clears any
    /// existing session state. If set to false, the broker resumes an
    /// existing session or creates a new one.
    pub clean_session: bool,
    /// Will message flag.
    ///
    /// If set to true, a will message will be published if the client
    /// disconnects unexpectedly.
    pub will_flag: bool,
    /// Will message QoS level.
    pub will_qos: QoS,
    /// Will message retain flag.
    pub will_retain: bool,
    /// Password flag.
    ///
    /// Indicates whether a password is present in the payload.
    pub password_flag: bool,
    /// Username flag.
    ///
    /// Indicates whether a username is present in the payload.
    pub username_flag: bool,
    /// Keep alive timeout in seconds.
    ///
    /// The maximum time interval between messages sent from the client.
    /// A value of 0 disables keep alive.
    pub keep_alive: u16,
    /// Unique client identifier.
    ///
    /// Must be a non-empty UTF-8 string if clean_session is false.
    /// Can be empty if clean_session is true (broker will assign an ID).
    pub client_id: String,
    /// Topic for will message.
    ///
    /// Required if will_flag is true.
    pub will_topic: Option<String>,
    /// Will message payload.
    ///
    /// Required if will_flag is true.
    pub will_message: Option<Bytes>,
    /// Username for authentication.
    ///
    /// Required if username_flag is true.
    pub username: Option<String>,
    /// Password for authentication.
    ///
    /// Required if password_flag is true. Also requires username to be set.
    pub password: Option<Bytes>,
}

impl Default for ConnectPacket {
    fn default() -> Self {
        ConnectPacket {
            protocol_name: "MQTT".to_string(),
            protocol_level: 4,
            clean_session: true,
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
            password_flag: false,
            username_flag: false,
            keep_alive: 60,
            client_id: String::new(),
            will_topic: None,
            will_message: None,
            username: None,
            password: None,
        }
    }
}

/// MQTT CONNACK packet.
///
/// Server response to a CONNECT packet, indicating whether the
/// connection was accepted or rejected.
#[derive(Debug, Clone)]
pub struct ConnAckPacket {
    /// Session present flag.
    ///
    /// Indicates whether a previous session exists for this client.
    /// Only meaningful when clean_session was set to false in CONNECT.
    pub session_present: bool,
    /// Connection return code.
    ///
    /// Indicates the result of the connection attempt.
    pub return_code: ConnectReturnCode,
}

impl ConnAckPacket {
    /// Creates a new CONNACK packet with the given return code.
    ///
    /// # Arguments
    ///
    /// * `return_code` - The connection return code.
    /// * `session_present` - Whether a session was already present.
    ///
    /// # Returns
    ///
    /// A new `ConnAckPacket` instance.
    pub fn new(return_code: ConnectReturnCode, session_present: bool) -> Self {
        ConnAckPacket {
            session_present,
            return_code,
        }
    }

    /// Creates a successful CONNACK packet.
    ///
    /// # Arguments
    ///
    /// * `session_present` - Whether a session was already present.
    ///
    /// # Returns
    ///
    /// A new `ConnAckPacket` instance with `ConnectReturnCode::Accepted`.
    pub fn accepted(session_present: bool) -> Self {
        Self::new(ConnectReturnCode::Accepted, session_present)
    }
}

/// MQTT PUBLISH packet.
///
/// Conveys message data from a publisher to subscribers.
#[derive(Debug, Clone)]
pub struct PublishPacket {
    /// Topic name the message is published to.
    ///
    /// Must not contain wildcard characters (+ or #).
    pub topic_name: String,
    /// Packet ID (required for QoS > 0).
    ///
    /// Must be non-zero for QoS 1 and QoS 2 messages.
    pub packet_id: Option<u16>,
    /// Message payload.
    pub payload: Bytes,
    /// Quality of Service level.
    pub qos: QoS,
    /// Duplicate delivery flag.
    ///
    /// Set to true when re-delivering a message. Only valid for
    /// QoS 1 and QoS 2 messages.
    pub duplicate: bool,
    /// Message retention flag.
    ///
    /// If set to true, the broker stores the message for future
    /// subscribers to this topic.
    pub retain: bool,
}

impl PublishPacket {
    /// Creates a new PUBLISH packet with QoS 0.
    ///
    /// # Arguments
    ///
    /// * `topic_name` - The topic name to publish to.
    /// * `payload` - The message payload.
    ///
    /// # Returns
    ///
    /// A new `PublishPacket` instance with QoS 0 and no packet ID.
    pub fn new(topic_name: impl Into<String>, payload: impl Into<Bytes>) -> Self {
        PublishPacket {
            topic_name: topic_name.into(),
            packet_id: None,
            payload: payload.into(),
            qos: QoS::AtMostOnce,
            duplicate: false,
            retain: false,
        }
    }
}

/// MQTT PUBACK packet.
///
/// Acknowledgment for a PUBLISH packet with QoS 1.
#[derive(Debug, Clone)]
pub struct PubAckPacket {
    /// Packet identifier from the PUBLISH packet being acknowledged.
    pub packet_id: u16,
}

/// MQTT PUBREC packet.
///
/// Publication received (QoS 2 delivery, part 1).
#[derive(Debug, Clone)]
pub struct PubRecPacket {
    /// Packet identifier from the PUBLISH packet.
    pub packet_id: u16,
}

/// MQTT PUBREL packet.
///
/// Publication release (QoS 2 delivery, part 2).
#[derive(Debug, Clone)]
pub struct PubRelPacket {
    /// Packet identifier from the PUBLISH packet.
    pub packet_id: u16,
}

/// MQTT PUBCOMP packet.
///
/// Publication complete (QoS 2 delivery, part 3).
#[derive(Debug, Clone)]
pub struct PubCompPacket {
    /// Packet identifier from the PUBLISH packet.
    pub packet_id: u16,
}

/// MQTT SUBSCRIBE packet.
///
/// Request to subscribe to one or more topic filters.
#[derive(Debug, Clone)]
pub struct SubscribePacket {
    /// Packet identifier for this subscription request.
    pub packet_id: u16,
    /// List of topic filters with their requested QoS levels.
    ///
    /// Must contain at least one topic filter.
    pub topics: Vec<(String, QoS)>,
}

impl SubscribePacket {
    /// Creates a new SUBSCRIBE packet with a single topic filter.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier.
    /// * `topic_filter` - The topic filter to subscribe to.
    /// * `qos` - The requested QoS level.
    ///
    /// # Returns
    ///
    /// A new `SubscribePacket` instance.
    pub fn new(packet_id: u16, topic_filter: impl Into<String>, qos: QoS) -> Self {
        SubscribePacket {
            packet_id,
            topics: vec![(topic_filter.into(), qos)],
        }
    }
}

/// MQTT SUBACK packet.
///
/// Acknowledgment for a SUBSCRIBE packet, containing the granted
/// QoS levels for each subscribed topic filter.
#[derive(Debug, Clone)]
pub struct SubAckPacket {
    /// Packet identifier from the SUBSCRIBE packet.
    pub packet_id: u16,
    /// Return codes for each topic filter in the SUBSCRIBE packet.
    ///
    /// The number of return codes must match the number of topic filters
    /// in the corresponding SUBSCRIBE packet.
    pub return_codes: Vec<SubAckReturnCode>,
}

impl SubAckPacket {
    /// Creates a new SUBACK packet.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier.
    /// * `return_codes` - The list of granted QoS levels or error codes.
    ///
    /// # Returns
    ///
    /// A new `SubAckPacket` instance.
    pub fn new(packet_id: u16, return_codes: Vec<SubAckReturnCode>) -> Self {
        SubAckPacket {
            packet_id,
            return_codes,
        }
    }
}

/// MQTT UNSUBSCRIBE packet.
///
/// Request to unsubscribe from one or more topic filters.
#[derive(Debug, Clone)]
pub struct UnsubscribePacket {
    /// Packet identifier for this unsubscription request.
    pub packet_id: u16,
    /// List of topic filters to unsubscribe from.
    ///
    /// Must contain at least one topic filter.
    pub topics: Vec<String>,
}

impl UnsubscribePacket {
    /// Creates a new UNSUBSCRIBE packet with a single topic filter.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier.
    /// * `topic_filter` - The topic filter to unsubscribe from.
    ///
    /// # Returns
    ///
    /// A new `UnsubscribePacket` instance.
    pub fn new(packet_id: u16, topic_filter: impl Into<String>) -> Self {
        UnsubscribePacket {
            packet_id,
            topics: vec![topic_filter.into()],
        }
    }
}

/// MQTT UNSUBACK packet.
///
/// Acknowledgment for an UNSUBSCRIBE packet.
#[derive(Debug, Clone)]
pub struct UnsubAckPacket {
    /// Packet identifier from the UNSUBSCRIBE packet.
    pub packet_id: u16,
}

/// MQTT PINGREQ packet.
///
/// Heartbeat request sent from client to server.
#[derive(Debug, Clone, Default)]
pub struct PingReqPacket;

/// MQTT PINGRESP packet.
///
/// Heartbeat response sent from server to client.
#[derive(Debug, Clone, Default)]
pub struct PingRespPacket;

/// MQTT DISCONNECT packet.
///
/// Notification that the client is disconnecting gracefully.
#[derive(Debug, Clone, Default)]
pub struct DisconnectPacket;

/// MQTT Packet enum containing all possible packet types.
#[derive(Debug, Clone)]
pub enum Packet {
    /// Connection request.
    Connect(ConnectPacket),
    /// Connection acknowledgment.
    ConnAck(ConnAckPacket),
    /// Publish message.
    Publish(PublishPacket),
    /// Publish acknowledgment (QoS 1).
    PubAck(PubAckPacket),
    /// Publish received (QoS 2).
    PubRec(PubRecPacket),
    /// Publish release (QoS 2).
    PubRel(PubRelPacket),
    /// Publish complete (QoS 2).
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
        }
    }
}
