//! Builder pattern for MQTT v3.1.1 packets.
//!
//! This module provides builder patterns for constructing complex MQTT packets
//! with a fluent API, ensuring type safety and correct default values.

use bytes::Bytes;

use super::packet::{ConnectPacket, PublishPacket, QoS, SubscribePacket, UnsubscribePacket};

/// Builder for creating [`ConnectPacket`] instances.
///
/// This builder provides a fluent interface for setting all possible fields
/// of an MQTT v3.1.1 CONNECT packet.
///
/// # Example
///
/// ```
/// use mqtt_codec::v4::{ConnectPacket, ConnectPacketBuilder, QoS};
///
/// let packet = ConnectPacketBuilder::new("my-client-id")
///     .keep_alive(30)
///     .clean_session(true)
///     .username("user")
///     .password("pass")
///     .will("will/topic", "will message", QoS::AtLeastOnce, false)
///     .build();
/// ```
#[derive(Debug)]
pub struct ConnectPacketBuilder {
    client_id: String,
    clean_session: bool,
    keep_alive: u16,
    will_flag: bool,
    will_qos: QoS,
    will_retain: bool,
    will_topic: Option<String>,
    will_message: Option<Bytes>,
    username: Option<String>,
    password: Option<Bytes>,
}

impl ConnectPacketBuilder {
    /// Creates a new CONNECT packet builder with the given client ID.
    ///
    /// # Arguments
    ///
    /// * `client_id` - The unique client identifier for this connection.
    ///
    /// # Default values
    ///
    /// - `clean_session`: true
    /// - `keep_alive`: 60 seconds
    /// - `will_flag`: false
    /// - `username`: None
    /// - `password`: None
    pub fn new(client_id: impl Into<String>) -> Self {
        ConnectPacketBuilder {
            client_id: client_id.into(),
            clean_session: true,
            keep_alive: 60,
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
            will_topic: None,
            will_message: None,
            username: None,
            password: None,
        }
    }

    /// Sets the clean session flag.
    ///
    /// # Arguments
    ///
    /// * `clean_session` - If true, the broker clears any existing state for this client.
    pub fn clean_session(mut self, clean_session: bool) -> Self {
        self.clean_session = clean_session;
        self
    }

    /// Sets the keep alive timeout in seconds.
    ///
    /// # Arguments
    ///
    /// * `keep_alive` - The timeout in seconds. A value of 0 disables keep alive.
    pub fn keep_alive(mut self, keep_alive: u16) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    /// Configures the will message for this connection.
    ///
    /// The will message is published by the broker if the client disconnects
    /// unexpectedly without sending a DISCONNECT packet.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to publish the will message to.
    /// * `message` - The payload of the will message.
    /// * `qos` - The Quality of Service level for the will message.
    /// * `retain` - Whether the will message should be retained by the broker.
    pub fn will(
        mut self,
        topic: impl Into<String>,
        message: impl Into<Bytes>,
        qos: QoS,
        retain: bool,
    ) -> Self {
        self.will_flag = true;
        self.will_topic = Some(topic.into());
        self.will_message = Some(message.into());
        self.will_qos = qos;
        self.will_retain = retain;
        self
    }

    /// Sets the username for authentication.
    ///
    /// # Arguments
    ///
    /// * `username` - The username string.
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Sets the password for authentication.
    ///
    /// # Arguments
    ///
    /// * `password` - The password as binary data.
    ///
    /// # Note
    ///
    /// A password can only be set if a username is also set in the final packet.
    pub fn password(mut self, password: impl Into<Bytes>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Builds the [`ConnectPacket`].
    ///
    /// # Returns
    ///
    /// A fully constructed `ConnectPacket` with the configured values.
    pub fn build(self) -> ConnectPacket {
        let username_flag = self.username.is_some();
        let password_flag = self.password.is_some();

        ConnectPacket {
            protocol_name: "MQTT".to_string(),
            protocol_level: 4,
            clean_session: self.clean_session,
            will_flag: self.will_flag,
            will_qos: self.will_qos,
            will_retain: self.will_retain,
            password_flag,
            username_flag,
            keep_alive: self.keep_alive,
            client_id: self.client_id,
            will_topic: self.will_topic,
            will_message: self.will_message,
            username: self.username,
            password: self.password,
        }
    }
}

impl ConnectPacket {
    /// Returns a new [`ConnectPacketBuilder`] with the given client ID.
    pub fn builder(client_id: impl Into<String>) -> ConnectPacketBuilder {
        ConnectPacketBuilder::new(client_id)
    }
}

/// Builder for creating [`PublishPacket`] instances.
///
/// This builder simplifies the creation of MQTT v3.1.1 PUBLISH packets.
///
/// # Example
///
/// ```
/// use mqtt_codec::v4::{PublishPacket, PublishPacketBuilder, QoS};
/// use bytes::Bytes;
///
/// let packet = PublishPacketBuilder::new("topic/name", Bytes::from("payload"))
///     .qos(QoS::AtLeastOnce)
///     .packet_id(1)
///     .retain(true)
///     .build();
/// ```
#[derive(Debug)]
pub struct PublishPacketBuilder {
    topic_name: String,
    payload: Bytes,
    qos: QoS,
    packet_id: Option<u16>,
    duplicate: bool,
    retain: bool,
}

impl PublishPacketBuilder {
    /// Creates a new PUBLISH packet builder with the given topic and payload.
    ///
    /// # Arguments
    ///
    /// * `topic_name` - The topic name to publish to.
    /// * `payload` - The message payload.
    pub fn new(topic_name: impl Into<String>, payload: impl Into<Bytes>) -> Self {
        PublishPacketBuilder {
            topic_name: topic_name.into(),
            payload: payload.into(),
            qos: QoS::AtMostOnce,
            packet_id: None,
            duplicate: false,
            retain: false,
        }
    }

    /// Sets the Quality of Service level.
    ///
    /// # Arguments
    ///
    /// * `qos` - The requested QoS level. Default is `QoS::AtMostOnce` (QoS 0).
    pub fn qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    /// Sets the packet identifier.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier. Required for QoS 1 and QoS 2.
    ///
    /// # Note
    ///
    /// The packet identifier must be non-zero for QoS > 0.
    pub fn packet_id(mut self, packet_id: u16) -> Self {
        self.packet_id = Some(packet_id);
        self
    }

    /// Sets the duplicate flag.
    ///
    /// # Arguments
    ///
    /// * `duplicate` - If true, indicates this is a re-delivery of a message.
    pub fn duplicate(mut self, duplicate: bool) -> Self {
        self.duplicate = duplicate;
        self
    }

    /// Sets the retain flag.
    ///
    /// # Arguments
    ///
    /// * `retain` - If true, the broker should store the message for future subscribers.
    pub fn retain(mut self, retain: bool) -> Self {
        self.retain = retain;
        self
    }

    /// Builds the [`PublishPacket`].
    ///
    /// # Returns
    ///
    /// A fully constructed `PublishPacket`.
    pub fn build(self) -> PublishPacket {
        PublishPacket {
            topic_name: self.topic_name,
            packet_id: self.packet_id,
            payload: self.payload,
            qos: self.qos,
            duplicate: self.duplicate,
            retain: self.retain,
        }
    }
}

impl PublishPacket {
    /// Returns a new [`PublishPacketBuilder`] for creating PUBLISH packets.
    pub fn builder(
        topic_name: impl Into<String>,
        payload: impl Into<Bytes>,
    ) -> PublishPacketBuilder {
        PublishPacketBuilder::new(topic_name, payload)
    }
}

/// Builder for creating [`SubscribePacket`] instances.
///
/// # Example
///
/// ```
/// use mqtt_codec::v4::{SubscribePacket, SubscribePacketBuilder, QoS};
///
/// let packet = SubscribePacketBuilder::new(1)
///     .topic("topic/filter1", QoS::AtLeastOnce)
///     .topic("topic/filter2", QoS::ExactlyOnce)
///     .build();
/// ```
#[derive(Debug)]
pub struct SubscribePacketBuilder {
    packet_id: u16,
    topics: Vec<(String, QoS)>,
}

impl SubscribePacketBuilder {
    /// Creates a new SUBSCRIBE packet builder with the given packet ID.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier for this request.
    pub fn new(packet_id: u16) -> Self {
        SubscribePacketBuilder {
            packet_id,
            topics: Vec::new(),
        }
    }

    /// Adds a topic filter with the requested QoS level to the subscription list.
    ///
    /// # Arguments
    ///
    /// * `topic_filter` - The topic filter string.
    /// * `qos` - The requested QoS level for this filter.
    pub fn topic(mut self, topic_filter: impl Into<String>, qos: QoS) -> Self {
        self.topics.push((topic_filter.into(), qos));
        self
    }

    /// Builds the [`SubscribePacket`].
    ///
    /// # Returns
    ///
    /// * `Some(SubscribePacket)` - If at least one topic filter was added.
    /// * `None` - If no topics were added.
    pub fn build(self) -> Option<SubscribePacket> {
        if self.topics.is_empty() {
            return None;
        }
        Some(SubscribePacket {
            packet_id: self.packet_id,
            topics: self.topics,
        })
    }
}

impl SubscribePacket {
    /// Returns a new [`SubscribePacketBuilder`] for creating SUBSCRIBE packets.
    pub fn builder(packet_id: u16) -> SubscribePacketBuilder {
        SubscribePacketBuilder::new(packet_id)
    }
}

/// Builder for creating [`UnsubscribePacket`] instances.
///
/// # Example
///
/// ```
/// use mqtt_codec::v4::UnsubscribePacketBuilder;
///
/// let packet = UnsubscribePacketBuilder::new(1)
///     .topic("topic/filter1")
///     .topic("topic/filter2")
///     .build();
/// ```
#[derive(Debug)]
pub struct UnsubscribePacketBuilder {
    packet_id: u16,
    topics: Vec<String>,
}

impl UnsubscribePacketBuilder {
    /// Creates a new UNSUBSCRIBE packet builder with the given packet ID.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet identifier for this request.
    pub fn new(packet_id: u16) -> Self {
        UnsubscribePacketBuilder {
            packet_id,
            topics: Vec::new(),
        }
    }

    /// Adds a topic filter to the unsubscription list.
    ///
    /// # Arguments
    ///
    /// * `topic_filter` - The topic filter string to remove.
    pub fn topic(mut self, topic_filter: impl Into<String>) -> Self {
        self.topics.push(topic_filter.into());
        self
    }

    /// Builds the [`UnsubscribePacket`].
    ///
    /// # Returns
    ///
    /// * `Some(UnsubscribePacket)` - If at least one topic filter was added.
    /// * `None` - If no topics were added.
    pub fn build(self) -> Option<UnsubscribePacket> {
        if self.topics.is_empty() {
            return None;
        }
        Some(UnsubscribePacket {
            packet_id: self.packet_id,
            topics: self.topics,
        })
    }
}

impl UnsubscribePacket {
    /// Returns a new [`UnsubscribePacketBuilder`] for creating UNSUBSCRIBE packets.
    pub fn builder(packet_id: u16) -> UnsubscribePacketBuilder {
        UnsubscribePacketBuilder::new(packet_id)
    }
}
