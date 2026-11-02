use bytes::Bytes;
use mqtt_codec::v5::{
    ConnectPacketBuilder, PublishPacketBuilder, QoS, SubscribePacketBuilder,
    UnsubscribePacketBuilder,
};

#[test]
fn test_connect_builder_basic() {
    let packet = ConnectPacketBuilder::new("test-client")
        .keep_alive(30)
        .clean_start(true)
        .build();

    assert_eq!(packet.client_id, "test-client");
    assert_eq!(packet.keep_alive, 30);
    assert!(packet.clean_start);
    assert!(!packet.will_flag);
}

#[test]
fn test_connect_builder_with_properties() {
    let packet = ConnectPacketBuilder::new("test-client")
        .session_expiry_interval(3600)
        .receive_maximum(100)
        .maximum_packet_size(65535)
        .build();

    assert_eq!(packet.properties.session_expiry_interval, Some(3600));
    assert_eq!(packet.properties.receive_maximum, Some(100));
    assert_eq!(packet.properties.maximum_packet_size, Some(65535));
}

#[test]
fn test_connect_builder_with_auth() {
    let packet = ConnectPacketBuilder::new("test-client")
        .authentication_method("OAuth2")
        .authentication_data(Bytes::from("token"))
        .build();

    assert_eq!(
        packet.properties.authentication_method,
        Some("OAuth2".to_string())
    );
    assert_eq!(
        packet.properties.authentication_data,
        Some(Bytes::from("token"))
    );
}

#[test]
fn test_publish_builder_basic() {
    let packet = PublishPacketBuilder::new("test/topic", Bytes::from("payload")).build();

    assert_eq!(packet.topic_name, "test/topic");
    assert_eq!(packet.payload, Bytes::from("payload"));
    assert_eq!(packet.qos, QoS::AtMostOnce);
}

#[test]
fn test_publish_builder_with_properties() {
    let packet = PublishPacketBuilder::new("test/topic", Bytes::from("payload"))
        .qos(QoS::AtLeastOnce)
        .packet_id(42)
        .message_expiry_interval(300)
        .content_type("application/json")
        .response_topic("reply/topic")
        .build();

    assert_eq!(packet.qos, QoS::AtLeastOnce);
    assert_eq!(packet.packet_id, Some(42));
    assert_eq!(packet.properties.message_expiry_interval, Some(300));
    assert_eq!(
        packet.properties.content_type,
        Some("application/json".to_string())
    );
    assert_eq!(
        packet.properties.response_topic,
        Some("reply/topic".to_string())
    );
}

#[test]
fn test_subscribe_builder() {
    let packet = SubscribePacketBuilder::new(1)
        .topic("topic/one", QoS::AtMostOnce)
        .topic_with_options("topic/two", QoS::ExactlyOnce, true, false, 1)
        .build()
        .unwrap();

    assert_eq!(packet.packet_id, 1);
    assert_eq!(packet.topics.len(), 2);
    assert_eq!(packet.topics[0].topic_filter, "topic/one");
    assert_eq!(packet.topics[0].qos, QoS::AtMostOnce);
    assert!(!packet.topics[0].no_local);
    assert_eq!(packet.topics[1].topic_filter, "topic/two");
    assert!(packet.topics[1].no_local);
    assert_eq!(packet.topics[1].retain_handling, 1);
}

#[test]
fn test_subscribe_builder_empty() {
    let packet = SubscribePacketBuilder::new(1).build();
    assert!(packet.is_none());
}

#[test]
fn test_unsubscribe_builder() {
    let packet = UnsubscribePacketBuilder::new(1)
        .topic("topic/one")
        .topic("topic/two")
        .build()
        .unwrap();

    assert_eq!(packet.packet_id, 1);
    assert_eq!(packet.topics.len(), 2);
}

#[test]
fn test_unsubscribe_builder_empty() {
    let packet = UnsubscribePacketBuilder::new(1).build();
    assert!(packet.is_none());
}
