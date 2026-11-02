use bytes::BytesMut;
use mqtt_codec::v4::{
    DisconnectPacket, MqttCodec, Packet, PingReqPacket, PingRespPacket, PubAckPacket,
    PublishPacket, QoS, SubAckPacket, SubAckReturnCode, SubscribePacket,
};
use mqtt_codec::{Decoder, Encoder};

#[test]
fn test_publish_large_payload() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let large_payload = bytes::Bytes::from(vec![0xAB; 1024 * 1024]);
    let publish_packet = Packet::Publish(PublishPacket {
        topic_name: "test/large".to_string(),
        packet_id: Some(1),
        payload: large_payload,
        qos: QoS::AtLeastOnce,
        duplicate: false,
        retain: false,
    });
    codec.encode(publish_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (publish_packet, decoded_packet) {
        (Packet::Publish(original), Packet::Publish(decoded)) => {
            assert_eq!(original.payload.len(), decoded.payload.len());
            assert_eq!(original.payload, decoded.payload);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_subscribe_with_many_topic_filters() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let mut topics = Vec::new();
    for i in 0..100 {
        topics.push((format!("sensor/{}/data", i), QoS::AtLeastOnce));
    }
    let subscribe_packet = Packet::Subscribe(SubscribePacket {
        packet_id: 1,
        topics,
    });
    codec.encode(subscribe_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (subscribe_packet, decoded_packet) {
        (Packet::Subscribe(original), Packet::Subscribe(decoded)) => {
            assert_eq!(original.topics.len(), decoded.topics.len());
            assert_eq!(100, decoded.topics.len());
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_publish_min_max_packet_id() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::Publish(PublishPacket {
        topic_name: "test/topic".to_string(),
        packet_id: Some(65535),
        payload: bytes::Bytes::from("data"),
        qos: QoS::AtLeastOnce,
        duplicate: false,
        retain: false,
    });
    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded) {
        (Packet::Publish(_orig), Packet::Publish(dec)) => assert_eq!(Some(65535), dec.packet_id),
        _ => panic!("Packet mismatch"),
    }
}

#[test]
fn test_connect_with_empty_will_message() {
    use mqtt_codec::v4::ConnectPacket;
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let connect_packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: true,
        will_qos: QoS::AtLeastOnce,
        will_retain: false,
        password_flag: false,
        username_flag: false,
        keep_alive: 60,
        client_id: "test".to_string(),
        will_topic: Some("will/topic".to_string()),
        will_message: Some(bytes::Bytes::new()),
        username: None,
        password: None,
    });
    codec.encode(connect_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (connect_packet, decoded_packet) {
        (Packet::Connect(original), Packet::Connect(decoded)) => {
            assert_eq!(original.will_message, decoded.will_message);
            assert!(decoded.will_message.as_ref().unwrap().is_empty());
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_suback_with_max_return_codes() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let mut return_codes = Vec::new();
    for _ in 0..100 {
        return_codes.push(SubAckReturnCode::SuccessQoS2);
    }
    let suback_packet = Packet::SubAck(SubAckPacket {
        packet_id: 1,
        return_codes,
    });
    codec.encode(suback_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (suback_packet, decoded_packet) {
        (Packet::SubAck(original), Packet::SubAck(decoded)) => {
            assert_eq!(original.return_codes.len(), decoded.return_codes.len());
            assert_eq!(100, decoded.return_codes.len());
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pingreq_minimal_size() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    codec
        .encode(Packet::PingReq(PingReqPacket), &mut buffer)
        .unwrap();
    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer[0], 0xC0);
    assert_eq!(buffer[1], 0x00);
}

#[test]
fn test_pingresp_minimal_size() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    codec
        .encode(Packet::PingResp(PingRespPacket), &mut buffer)
        .unwrap();
    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer[0], 0xD0);
    assert_eq!(buffer[1], 0x00);
}

#[test]
fn test_disconnect_minimal_size() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    codec
        .encode(Packet::Disconnect(DisconnectPacket), &mut buffer)
        .unwrap();
    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer[0], 0xE0);
    assert_eq!(buffer[1], 0x00);
}

#[test]
fn test_puback_size() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    codec
        .encode(Packet::PubAck(PubAckPacket { packet_id: 256 }), &mut buffer)
        .unwrap();
    assert_eq!(buffer.len(), 4);
    assert_eq!(buffer[0], 0x40);
    assert_eq!(buffer[1], 0x02);
}

#[test]
fn test_connect_minimal_size() {
    use mqtt_codec::v4::ConnectPacket;
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: false,
        username_flag: false,
        keep_alive: 60,
        client_id: "x".to_string(),
        will_topic: None,
        will_message: None,
        username: None,
        password: None,
    });
    codec.encode(packet, &mut buffer).unwrap();
    assert!(buffer.len() >= 15);
}

#[test]
fn test_publish_payload_size_preserved() {
    let mut codec = MqttCodec::new();
    for payload_size in &[0, 1, 100, 1000, 10000, 100000] {
        let mut buffer = BytesMut::new();
        let payload = bytes::Bytes::from(vec![0xAA; *payload_size]);
        let packet = Packet::Publish(PublishPacket {
            topic_name: "test".to_string(),
            packet_id: Some(1),
            payload: payload.clone(),
            qos: QoS::AtLeastOnce,
            duplicate: false,
            retain: false,
        });
        codec.encode(packet.clone(), &mut buffer).unwrap();
        assert!(buffer.len() >= 9);
        let decoded = codec.decode(&mut buffer).unwrap().unwrap();
        match decoded {
            Packet::Publish(p) => {
                assert_eq!(p.payload.len(), *payload_size);
                assert_eq!(p.payload, payload);
            }
            _ => panic!("Expected PUBLISH packet"),
        }
    }
}

#[test]
fn test_topic_length_encoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    for topic_len in &[1, 10, 100, 256] {
        buffer.clear();
        let topic = "a".repeat(*topic_len);
        let packet = Packet::Publish(PublishPacket {
            topic_name: topic,
            packet_id: Some(1),
            payload: bytes::Bytes::new(),
            qos: QoS::AtLeastOnce,
            duplicate: false,
            retain: false,
        });
        codec.encode(packet.clone(), &mut buffer).unwrap();
        let decoded = codec.decode(&mut buffer).unwrap().unwrap();
        match decoded {
            Packet::Publish(p) => assert_eq!(p.topic_name.len(), *topic_len),
            _ => panic!("Expected PUBLISH packet"),
        }
    }
}

#[test]
fn test_subscribe_topic_filter_count() {
    let mut codec = MqttCodec::new();
    for filter_count in &[1, 5, 10, 20] {
        let mut buffer = BytesMut::new();
        let mut topics = Vec::new();
        for i in 0..*filter_count {
            topics.push((format!("topic{}", i), QoS::AtMostOnce));
        }
        let packet = Packet::Subscribe(SubscribePacket {
            packet_id: 1,
            topics,
        });
        codec.encode(packet.clone(), &mut buffer).unwrap();
        let decoded = codec.decode(&mut buffer).unwrap().unwrap();
        match decoded {
            Packet::Subscribe(p) => assert_eq!(p.topics.len(), *filter_count),
            _ => panic!("Expected SUBSCRIBE packet"),
        }
    }
}

#[test]
fn decode_should_wait_when_remaining_length_is_incomplete() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::from(&[0xC0, 0x80][..]);

    assert!(codec.decode(&mut buffer).unwrap().is_none());
    assert_eq!(buffer.as_ref(), &[0xC0, 0x80]);
}

#[test]
fn decode_should_reject_non_minimal_remaining_length() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::from(&[0xC0, 0x80, 0x00][..]);

    assert!(codec.decode(&mut buffer).is_err());
}

#[test]
fn decode_should_reject_pingreq_with_non_zero_remaining_length() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::from(&[0xC0, 0x01, 0x00][..]);

    assert!(codec.decode(&mut buffer).is_err());
}

#[test]
fn decode_should_reject_puback_with_trailing_bytes() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::from(&[0x40, 0x03, 0x00, 0x01, 0x00][..]);

    assert!(codec.decode(&mut buffer).is_err());
}
