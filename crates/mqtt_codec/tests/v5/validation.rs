use mqtt_codec::v5::{
    AuthPacket, ConnectPacket, PacketType, Properties, PublishPacket, QoS, ReasonCode,
    parse_shared_subscription, validate_auth_packet, validate_credentials_flags,
    validate_fixed_header_flags, validate_property_scope, validate_property_values,
    validate_protocol_level, validate_protocol_name, validate_publish_packet,
    validate_qos_packet_id, validate_reason_code_for_packet, validate_topic_filter,
};

#[test]
fn test_validate_protocol_name_valid() {
    assert!(validate_protocol_name("MQTT").is_ok());
}

#[test]
fn test_validate_protocol_name_invalid() {
    assert!(validate_protocol_name("MQIsdp").is_err());
}

#[test]
fn test_validate_protocol_level_valid() {
    assert!(validate_protocol_level(5).is_ok());
}

#[test]
fn test_validate_protocol_level_invalid() {
    assert!(validate_protocol_level(4).is_err());
}

#[test]
fn test_validate_reason_code_connack() {
    assert!(validate_reason_code_for_packet(ReasonCode::Success, PacketType::ConnAck).is_ok());
    assert!(validate_reason_code_for_packet(ReasonCode::GrantedQoS1, PacketType::ConnAck).is_err());
}

#[test]
fn test_validate_reason_code_suback() {
    assert!(validate_reason_code_for_packet(ReasonCode::GrantedQoS1, PacketType::SubAck).is_ok());
    assert!(validate_reason_code_for_packet(ReasonCode::Success, PacketType::SubAck).is_ok());
}

#[test]
fn test_validate_reason_code_auth() {
    assert!(
        validate_reason_code_for_packet(ReasonCode::ContinueAuthentication, PacketType::Auth)
            .is_ok()
    );
    assert!(
        validate_reason_code_for_packet(ReasonCode::UnspecifiedError, PacketType::Auth).is_err()
    );
}

#[test]
fn test_validate_fixed_header_flags() {
    assert!(validate_fixed_header_flags(PacketType::Connect, 0).is_ok());
    assert!(validate_fixed_header_flags(PacketType::PubRel, 0x02).is_ok());
    assert!(validate_fixed_header_flags(PacketType::Subscribe, 0x02).is_ok());
    assert!(validate_fixed_header_flags(PacketType::Connect, 1).is_err());
}

#[test]
fn test_validate_property_scope_publish() {
    // topic_alias is allowed in PUBLISH
    let props = Properties {
        topic_alias: Some(5),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Publish).is_ok());

    // session_expiry_interval is NOT allowed in PUBLISH
    let props = Properties {
        session_expiry_interval: Some(100),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Publish).is_err());
}

#[test]
fn test_validate_property_scope_connect() {
    // session_expiry_interval is allowed in CONNECT
    let props = Properties {
        session_expiry_interval: Some(100),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Connect).is_ok());

    // topic_alias is NOT allowed in CONNECT
    let props = Properties {
        topic_alias: Some(5),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Connect).is_err());
}

#[test]
fn test_validate_property_scope_auth() {
    // authentication_method is allowed in AUTH
    let props = Properties {
        authentication_method: Some("SCRAM".to_string()),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Auth).is_ok());

    // receive_maximum is NOT allowed in AUTH
    let props = Properties {
        receive_maximum: Some(100),
        ..Default::default()
    };
    assert!(validate_property_scope(&props, PacketType::Auth).is_err());
}

#[test]
fn test_validate_credentials_v5_password_without_username() {
    // In MQTT v5.0, password can be sent without username
    let packet = ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 5,
        clean_start: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: true,
        username_flag: false,
        keep_alive: 60,
        properties: Properties::default(),
        client_id: "test".to_string(),
        will_topic: None,
        will_message: None,
        will_properties: None,
        username: None,
        password: Some(bytes::Bytes::from("secret")),
    };
    assert!(validate_credentials_flags(&packet).is_ok());
}

#[test]
fn test_parse_shared_subscription_valid() {
    let result = parse_shared_subscription("$share/consumer-group/sensor/+/data");
    assert!(result.is_some());
    let shared = result.unwrap();
    assert_eq!(shared.share_name, "consumer-group");
    assert_eq!(shared.topic_filter, "sensor/+/data");
}

#[test]
fn test_parse_shared_subscription_simple() {
    let result = parse_shared_subscription("$share/g1/topic");
    assert!(result.is_some());
    let shared = result.unwrap();
    assert_eq!(shared.share_name, "g1");
    assert_eq!(shared.topic_filter, "topic");
}

#[test]
fn test_parse_shared_subscription_with_multilevel_wildcard() {
    let result = parse_shared_subscription("$share/group/sensor/#");
    assert!(result.is_some());
    let shared = result.unwrap();
    assert_eq!(shared.share_name, "group");
    assert_eq!(shared.topic_filter, "sensor/#");
}

#[test]
fn test_parse_shared_subscription_not_shared() {
    assert!(parse_shared_subscription("sensor/data").is_none());
    assert!(parse_shared_subscription("$SYS/broker/info").is_none());
}

#[test]
fn test_parse_shared_subscription_empty_share_name() {
    // $share//topic - empty share name
    assert!(parse_shared_subscription("$share//topic").is_none());
}

#[test]
fn test_parse_shared_subscription_empty_filter() {
    // $share/group/ - empty topic filter
    assert!(parse_shared_subscription("$share/group/").is_none());
}

#[test]
fn test_parse_shared_subscription_no_filter_separator() {
    // $share/group - no second slash
    assert!(parse_shared_subscription("$share/group").is_none());
}

#[test]
fn test_validate_publish_empty_topic_without_alias_fails() {
    let packet = PublishPacket {
        topic_name: String::new(),
        packet_id: None,
        payload: bytes::Bytes::new(),
        qos: QoS::AtMostOnce,
        duplicate: false,
        retain: false,
        properties: Properties::default(),
    };
    assert!(validate_publish_packet(&packet).is_err());
}

#[test]
fn test_validate_qos_packet_id_mismatch() {
    assert!(validate_qos_packet_id(QoS::AtMostOnce, Some(1)).is_err());
    assert!(validate_qos_packet_id(QoS::AtLeastOnce, None).is_err());
    assert!(validate_qos_packet_id(QoS::AtLeastOnce, Some(1)).is_ok());
}

#[test]
fn test_validate_shared_subscription_topic_filter() {
    // Valid shared subscription
    assert!(validate_topic_filter("$share/group/sensor/+/data").is_ok());
    // Invalid: share name contains wildcard
    assert!(validate_topic_filter("$share/gro+up/sensor/data").is_err());
    // Invalid: share name contains #
    assert!(validate_topic_filter("$share/gro#up/sensor/data").is_err());
    // Invalid: share name contains /
    assert!(validate_topic_filter("$share/gro/up/sensor/data").is_ok()); // This is actually "gro" as share name, "up/sensor/data" as filter
}

#[test]
fn validate_property_values_should_reject_subscription_identifier_zero() {
    let props = Properties {
        subscription_identifiers: vec![0],
        ..Default::default()
    };
    assert!(validate_property_values(&props).is_err());
}

#[test]
fn validate_auth_packet_should_require_authentication_method() {
    let packet = AuthPacket {
        reason_code: ReasonCode::Success,
        properties: Properties::default(),
    };
    assert!(validate_auth_packet(&packet).is_err());

    let packet = AuthPacket {
        reason_code: ReasonCode::Success,
        properties: Properties {
            authentication_method: Some("SCRAM-SHA-256".into()),
            ..Default::default()
        },
    };
    assert!(validate_auth_packet(&packet).is_ok());
}
