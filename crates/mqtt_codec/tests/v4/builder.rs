use bytes::Bytes;
use mqtt_codec::v4::{
    ConnectPacketBuilder, PublishPacketBuilder, QoS, SubscribePacketBuilder,
    UnsubscribePacketBuilder,
};

#[test]
fn test_connect_builder_basic() {
    let packet = ConnectPacketBuilder::new("test-client")
        .keep_alive(30)
        .clean_session(true)
        .build();

    assert_eq!(packet.client_id, "test-client");
    assert_eq!(packet.keep_alive, 30);
    assert!(packet.clean_session);
    assert!(!packet.will_flag);
    assert!(!packet.username_flag);
    assert!(!packet.password_flag);
}

#[test]
fn test_connect_builder_with_credentials() {
    let packet = ConnectPacketBuilder::new("test-client")
        .username("user")
        .password("pass")
        .build();

    assert_eq!(packet.username, Some("user".to_string()));
    assert_eq!(packet.password, Some(Bytes::from("pass")));
    assert!(packet.username_flag);
    assert!(packet.password_flag);
}

#[test]
fn test_connect_builder_with_will() {
    let packet = ConnectPacketBuilder::new("test-client")
        .will("will/topic", "will message", QoS::AtLeastOnce, true)
        .build();

    assert!(packet.will_flag);
    assert_eq!(packet.will_topic, Some("will/topic".to_string()));
    assert_eq!(packet.will_message, Some(Bytes::from("will message")));
    assert_eq!(packet.will_qos, QoS::AtLeastOnce);
    assert!(packet.will_retain);
}

#[test]
fn test_publish_builder_basic() {
    let packet = PublishPacketBuilder::new("test/topic", Bytes::from("payload")).build();

    assert_eq!(packet.topic_name, "test/topic");
    assert_eq!(packet.payload, Bytes::from("payload"));
    assert_eq!(packet.qos, QoS::AtMostOnce);
    assert!(packet.packet_id.is_none());
    assert!(!packet.duplicate);
    assert!(!packet.retain);
}

#[test]
fn test_publish_builder_qos1() {
    let packet = PublishPacketBuilder::new("test/topic", Bytes::from("payload"))
        .qos(QoS::AtLeastOnce)
        .packet_id(42)
        .build();

    assert_eq!(packet.qos, QoS::AtLeastOnce);
    assert_eq!(packet.packet_id, Some(42));
}

#[test]
fn test_subscribe_builder() {
    let packet = SubscribePacketBuilder::new(1)
        .topic("topic/one", QoS::AtMostOnce)
        .topic("topic/two", QoS::AtLeastOnce)
        .build()
        .unwrap();

    assert_eq!(packet.packet_id, 1);
    assert_eq!(packet.topics.len(), 2);
    assert_eq!(packet.topics[0], ("topic/one".to_string(), QoS::AtMostOnce));
    assert_eq!(
        packet.topics[1],
        ("topic/two".to_string(), QoS::AtLeastOnce)
    );
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
    assert_eq!(packet.topics[0], "topic/one");
    assert_eq!(packet.topics[1], "topic/two");
}

#[test]
fn test_unsubscribe_builder_empty() {
    let packet = UnsubscribePacketBuilder::new(1).build();
    assert!(packet.is_none());
}
