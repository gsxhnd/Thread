//! Builder pattern for MQTT v5.0 packets.
//!
//! This module provides builder patterns for constructing complex MQTT v5.0 packets
//! with a fluent API, ensuring type safety and correct default values.

use bytes::Bytes;

use super::packet::{
    ConnectPacket, Properties, PublishPacket, QoS, SubscribePacket, SubscriptionOption,
    UnsubscribePacket,
};

/// Builder for creating [`ConnectPacket`] instances for MQTT v5.0.
///
/// This builder provides a fluent interface for setting all possible fields
/// and properties of an MQTT v5.0 CONNECT packet.
///
/// # Example
///
/// ```
/// use mqtt_codec::v5::{ConnectPacketBuilder, QoS};
///
/// let packet = ConnectPacketBuilder::new("my-client-id")
///     .keep_alive(30)
///     .clean_start(true)
///     .session_expiry_interval(3600)
///     .username("user")
///     .password("pass")
///     .will("will/topic", "will message", QoS::AtLeastOnce, false)
///     .build();
/// ```
#[derive(Debug)]
pub struct ConnectPacketBuilder {
    client_id: String,
    clean_start: bool,
    keep_alive: u16,
    properties: Properties,
    will_flag: bool,
    will_qos: QoS,
    will_retain: bool,
    will_topic: Option<String>,
    will_message: Option<Bytes>,
    will_properties: Option<Properties>,
    username: Option<String>,
    password: Option<Bytes>,
}

impl ConnectPacketBuilder {
    /// Creates a new CONNECT packet builder with the given client ID.
    ///
    /// # Arguments
    ///
    /// * `client_id` - The unique client identifier.
    ///
    /// # Default values
    ///
    /// - `clean_start`: true
    /// - `keep_alive`: 60 seconds
    /// - `will_flag`: false
    /// - `username`: None
    /// - `password`: None
    pub fn new(client_id: impl Into<String>) -> Self {
        ConnectPacketBuilder {
            client_id: client_id.into(),
            clean_start: true,
            keep_alive: 60,
            properties: Properties::new(),
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
            will_topic: None,
            will_message: None,
            will_properties: None,
            username: None,
            password: None,
        }
    }

    /// Sets the clean start flag.
    ///
    /// # Arguments
    ///
    /// * `clean_start` - If true, the broker clears any existing state for this client.
    pub fn clean_start(mut self, clean_start: bool) -> Self {
        self.clean_start = clean_start;
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

    /// Sets the session expiry interval property.
    ///
    /// # Arguments
    ///
    /// * `interval` - The time in seconds after which the session expires.
    pub fn session_expiry_interval(mut self, interval: u32) -> Self {
        self.properties.session_expiry_interval = Some(interval);
        self
    }

    /// Sets the receive maximum property.
    ///
    /// # Arguments
    ///
    /// * `maximum` - The maximum number of QoS 1 and QoS 2 publications to process concurrently.
    pub fn receive_maximum(mut self, maximum: u16) -> Self {
        self.properties.receive_maximum = Some(maximum);
        self
    }

    /// Sets the maximum packet size property.
    ///
    /// # Arguments
    ///
    /// * `size` - The maximum packet size in bytes the client is willing to accept.
    pub fn maximum_packet_size(mut self, size: u32) -> Self {
        self.properties.maximum_packet_size = Some(size);
        self
    }

    /// Sets the topic alias maximum property.
    ///
    /// # Arguments
    ///
    /// * `maximum` - The maximum value for a topic alias.
    pub fn topic_alias_maximum(mut self, maximum: u16) -> Self {
        self.properties.topic_alias_maximum = Some(maximum);
        self
    }

    /// Sets the authentication method property.
    ///
    /// # Arguments
    ///
    /// * `method` - The name of the authentication method.
    pub fn authentication_method(mut self, method: impl Into<String>) -> Self {
        self.properties.authentication_method = Some(method.into());
        self
    }

    /// Sets the authentication data property.
    ///
    /// # Arguments
    ///
    /// * `data` - The binary authentication data.
    pub fn authentication_data(mut self, data: impl Into<Bytes>) -> Self {
        self.properties.authentication_data = Some(data.into());
        self
    }

    /// Configures the will message for this connection.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to publish the will message to.
    /// * `message` - The payload of the will message.
    /// * `qos` - The Quality of Service level for the will message.
    /// * `retain` - Whether the will message should be retained.
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

    /// Sets the will delay interval property.
    ///
    /// # Arguments
    ///
    /// * `interval` - The delay in seconds before the will message is published.
    pub fn will_delay_interval(mut self, interval: u32) -> Self {
        self.will_properties
            .get_or_insert_with(Properties::new)
            .will_delay_interval = Some(interval);
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
    pub fn password(mut self, password: impl Into<Bytes>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Adds a user property to the connection.
    ///
    /// # Arguments
    ///
    /// * `key` - The key string.
    /// * `value` - The value string.
    pub fn user_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .user_properties
            .push((key.into(), value.into()));
        self
    }

    /// Builds the [`ConnectPacket`].
    ///
    /// # Returns
    ///
    /// A fully constructed `ConnectPacket`.
    pub fn build(self) -> ConnectPacket {
        let username_flag = self.username.is_some();
        let password_flag = self.password.is_some();

        ConnectPacket {
            protocol_name: "MQTT".to_string(),
            protocol_level: 5,
            clean_start: self.clean_start,
            will_flag: self.will_flag,
            will_qos: self.will_qos,
            will_retain: self.will_retain,
            password_flag,
            username_flag,
            keep_alive: self.keep_alive,
            properties: self.properties,
            client_id: self.client_id,
            will_topic: self.will_topic,
            will_message: self.will_message,
            will_properties: self.will_properties,
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

/// Builder for creating [`PublishPacket`] instances for MQTT v5.0.
///
/// # Example
///
/// ```
/// use mqtt_codec::v5::{PublishPacketBuilder, QoS};
/// use bytes::Bytes;
///
/// let packet = PublishPacketBuilder::new("topic/name", Bytes::from("payload"))
///     .qos(QoS::AtLeastOnce)
///     .packet_id(1)
///     .retain(true)
///     .message_expiry_interval(3600)
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
    properties: Properties,
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
            properties: Properties::new(),
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

    /// Sets the message expiry interval property.
    ///
    /// # Arguments
    ///
    /// * `interval` - The lifetime of the message in seconds.
    pub fn message_expiry_interval(mut self, interval: u32) -> Self {
        self.properties.message_expiry_interval = Some(interval);
        self
    }

    /// Sets the topic alias property.
    ///
    /// # Arguments
    ///
    /// * `alias` - An integer identifying the topic name.
    pub fn topic_alias(mut self, alias: u16) -> Self {
        self.properties.topic_alias = Some(alias);
        self
    }

    /// Sets the payload format indicator property.
    ///
    /// # Arguments
    ///
    /// * `indicator` - 0: Unspecified byte stream, 1: UTF-8 encoded.
    pub fn payload_format_indicator(mut self, indicator: u8) -> Self {
        self.properties.payload_format_indicator = Some(indicator);
        self
    }

    /// Sets the content type property.
    ///
    /// # Arguments
    ///
    /// * `content_type` - A string describing the content type.
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.properties.content_type = Some(content_type.into());
        self
    }

    /// Sets the response topic property.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic for response messages.
    pub fn response_topic(mut self, topic: impl Into<String>) -> Self {
        self.properties.response_topic = Some(topic.into());
        self
    }

    /// Sets the correlation data property.
    ///
    /// # Arguments
    ///
    /// * `data` - Binary data for correlating request and response.
    pub fn correlation_data(mut self, data: impl Into<Bytes>) -> Self {
        self.properties.correlation_data = Some(data.into());
        self
    }

    /// Adds a user property to the message.
    ///
    /// # Arguments
    ///
    /// * `key` - The key string.
    /// * `value` - The value string.
    pub fn user_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .user_properties
            .push((key.into(), value.into()));
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
            properties: self.properties,
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

/// Builder for creating [`SubscribePacket`] instances for MQTT v5.0.
///
/// # Example
///
/// ```
/// use mqtt_codec::v5::{SubscribePacketBuilder, QoS};
///
/// let packet = SubscribePacketBuilder::new(1)
///     .topic("topic/filter1", QoS::AtLeastOnce)
///     .topic_with_options("topic/filter2", QoS::ExactlyOnce, true, false, 0)
///     .build()
///     .unwrap();
/// ```
#[derive(Debug)]
pub struct SubscribePacketBuilder {
    packet_id: u16,
    properties: Properties,
    topics: Vec<SubscriptionOption>,
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
            properties: Properties::new(),
            topics: Vec::new(),
        }
    }

    /// Adds a topic filter with default subscription options.
    ///
    /// Default options: `no_local=false`, `retain_as_published=false`, `retain_handling=0`.
    ///
    /// # Arguments
    ///
    /// * `topic_filter` - The topic filter string.
    /// * `qos` - The requested QoS level.
    pub fn topic(mut self, topic_filter: impl Into<String>, qos: QoS) -> Self {
        self.topics.push(SubscriptionOption {
            topic_filter: topic_filter.into(),
            qos,
            no_local: false,
            retain_as_published: false,
            retain_handling: 0,
        });
        self
    }

    /// Adds a topic filter with custom subscription options.
    ///
    /// # Arguments
    ///
    /// * `topic_filter` - The topic filter string.
    /// * `qos` - The requested QoS level.
    /// * `no_local` - If true, don't receive publications from this client.
    /// * `retain_as_published` - If true, preserve the retain flag.
    /// * `retain_handling` - How to handle retained messages.
    pub fn topic_with_options(
        mut self,
        topic_filter: impl Into<String>,
        qos: QoS,
        no_local: bool,
        retain_as_published: bool,
        retain_handling: u8,
    ) -> Self {
        self.topics.push(SubscriptionOption {
            topic_filter: topic_filter.into(),
            qos,
            no_local,
            retain_as_published,
            retain_handling,
        });
        self
    }

    /// Adds a subscription identifier property.
    ///
    /// # Arguments
    ///
    /// * `id` - The subscription identifier value.
    pub fn subscription_identifier(mut self, id: u32) -> Self {
        self.properties.subscription_identifiers.push(id);
        self
    }

    /// Adds a user property to the subscription request.
    ///
    /// # Arguments
    ///
    /// * `key` - The key string.
    /// * `value` - The value string.
    pub fn user_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .user_properties
            .push((key.into(), value.into()));
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
            properties: self.properties,
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

/// Builder for creating [`UnsubscribePacket`] instances for MQTT v5.0.
///
/// # Example
///
/// ```
/// use mqtt_codec::v5::UnsubscribePacketBuilder;
///
/// let packet = UnsubscribePacketBuilder::new(1)
///     .topic("topic/filter1")
///     .topic("topic/filter2")
///     .build()
///     .unwrap();
/// ```
#[derive(Debug)]
pub struct UnsubscribePacketBuilder {
    packet_id: u16,
    properties: Properties,
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
            properties: Properties::new(),
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

    /// Adds a user property to the unsubscription request.
    ///
    /// # Arguments
    ///
    /// * `key` - The key string.
    /// * `value` - The value string.
    pub fn user_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties
            .user_properties
            .push((key.into(), value.into()));
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
            properties: self.properties,
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
