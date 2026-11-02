use mqtt_codec::v4::{
    ConnectPacket, QoS, validate_client_id, validate_credentials_flags, validate_packet_id,
    validate_protocol_level, validate_protocol_name, validate_qos_packet_id, validate_topic_filter,
    validate_topic_name, validate_will_message,
};

#[test]
fn test_connect_invalid_protocol_name() {
    assert!(validate_protocol_name("MQTT").is_ok());
    assert!(validate_protocol_name("XMPP").is_err());
    assert!(validate_protocol_name("").is_err());
    assert!(validate_protocol_name("mqtt").is_err());
}

#[test]
fn test_connect_invalid_protocol_level() {
    assert!(validate_protocol_level(4).is_ok());
    assert!(validate_protocol_level(3).is_err());
    assert!(validate_protocol_level(5).is_err());
    assert!(validate_protocol_level(0).is_err());
    assert!(validate_protocol_level(255).is_err());
}

#[test]
fn test_connect_empty_client_id_with_non_clean_session() {
    assert!(validate_client_id("", true).is_ok());
    assert!(validate_client_id("", false).is_err());
}

#[test]
fn test_connect_will_flag_inconsistency() {
    let mut packet = ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: false,
        username_flag: false,
        keep_alive: 60,
        client_id: "test".to_string(),
        will_topic: Some("topic".to_string()),
        will_message: None,
        username: None,
        password: None,
    };
    assert!(validate_will_message(&packet).is_err());
    packet.will_topic = None;
    packet.will_message = Some(bytes::Bytes::from("message"));
    assert!(validate_will_message(&packet).is_err());
    packet.will_flag = true;
    packet.will_message = Some(bytes::Bytes::from("message"));
    packet.will_topic = None;
    assert!(validate_will_message(&packet).is_err());
    packet.will_topic = Some("topic".to_string());
    packet.will_message = None;
    assert!(validate_will_message(&packet).is_err());
}

#[test]
fn test_connect_password_without_username() {
    let mut packet = ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: true,
        username_flag: false,
        keep_alive: 60,
        client_id: "test".to_string(),
        will_topic: None,
        will_message: None,
        username: None,
        password: Some(bytes::Bytes::from("pass")),
    };
    assert!(validate_credentials_flags(&packet).is_err());
    packet.username_flag = true;
    packet.username = Some("user".to_string());
    assert!(validate_credentials_flags(&packet).is_ok());
}

#[test]
fn test_publish_empty_topic_name() {
    assert!(validate_topic_name("").is_err());
}

#[test]
fn test_publish_topic_with_wildcard() {
    assert!(validate_topic_name("sensor/+/temperature").is_err());
    assert!(validate_topic_name("sensor/#").is_err());
    assert!(validate_topic_name("home/room/+").is_err());
}

#[test]
fn test_publish_topic_with_null_character() {
    assert!(validate_topic_name("topic\0name").is_err());
}

#[test]
fn test_valid_topic_names() {
    assert!(validate_topic_name("sensor/temperature").is_ok());
    assert!(validate_topic_name("home/room/light").is_ok());
    assert!(validate_topic_name("$SYS/broker/version").is_ok());
    assert!(validate_topic_name("a").is_ok());
}

#[test]
fn test_subscribe_empty_topic_filter() {
    assert!(validate_topic_filter("").is_err());
}

#[test]
fn test_subscribe_invalid_multilevel_wildcard() {
    assert!(validate_topic_filter("a/b/#/c").is_err());
    assert!(validate_topic_filter("a#").is_err());
}

#[test]
fn test_subscribe_invalid_single_level_wildcard() {
    assert!(validate_topic_filter("a+b").is_err());
    assert!(validate_topic_filter("a+").is_err());
    assert!(validate_topic_filter("+a").is_err());
}

#[test]
fn test_subscribe_valid_wildcard_filters() {
    assert!(validate_topic_filter("+").is_ok());
    assert!(validate_topic_filter("+/topic").is_ok());
    assert!(validate_topic_filter("topic/+").is_ok());
    assert!(validate_topic_filter("sensor/+/temperature").is_ok());
    assert!(validate_topic_filter("#").is_ok());
    assert!(validate_topic_filter("sensor/#").is_ok());
    assert!(validate_topic_filter("$SYS/#").is_ok());
}

#[test]
fn test_packet_id_cannot_be_zero() {
    assert!(validate_packet_id(0).is_err());
    assert!(validate_packet_id(1).is_ok());
    assert!(validate_packet_id(65535).is_ok());
}

#[test]
fn test_publish_qos_and_packet_id_consistency() {
    assert!(validate_qos_packet_id(QoS::AtMostOnce, None).is_ok());
    assert!(validate_qos_packet_id(QoS::AtMostOnce, Some(1)).is_err());
    assert!(validate_qos_packet_id(QoS::AtLeastOnce, Some(1)).is_ok());
    assert!(validate_qos_packet_id(QoS::AtLeastOnce, None).is_err());
    assert!(validate_qos_packet_id(QoS::AtLeastOnce, Some(0)).is_err());
    assert!(validate_qos_packet_id(QoS::ExactlyOnce, Some(100)).is_ok());
    assert!(validate_qos_packet_id(QoS::ExactlyOnce, None).is_err());
}
