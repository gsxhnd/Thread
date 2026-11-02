use super::*;

#[test]
fn test_connect_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    let connect_packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: false,
        username_flag: false,
        keep_alive: 60,
        client_id: "test_client".to_string(),
        will_topic: None,
        will_message: None,
        username: None,
        password: None,
    });

    codec.encode(connect_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (connect_packet, decoded_packet) {
        (Packet::Connect(original), Packet::Connect(decoded)) => {
            assert_eq!(original.protocol_name, decoded.protocol_name);
            assert_eq!(original.protocol_level, decoded.protocol_level);
            assert_eq!(original.clean_session, decoded.clean_session);
            assert_eq!(original.client_id, decoded.client_id);
            assert_eq!(original.keep_alive, decoded.keep_alive);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_connack_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    let connack_packet = Packet::ConnAck(ConnAckPacket {
        session_present: true,
        return_code: ConnectReturnCode::Accepted,
    });

    codec.encode(connack_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (connack_packet, decoded_packet) {
        (Packet::ConnAck(original), Packet::ConnAck(decoded)) => {
            assert_eq!(original.session_present, decoded.session_present);
            assert_eq!(original.return_code, decoded.return_code);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_publish_packet_qos0_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    let payload = bytes::Bytes::from("Hello, MQTT!");
    let publish_packet = Packet::Publish(PublishPacket {
        topic_name: "test/topic".to_string(),
        packet_id: None,
        payload: payload.clone(),
        qos: QoS::AtMostOnce,
        duplicate: false,
        retain: false,
    });

    codec.encode(publish_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (publish_packet, decoded_packet) {
        (Packet::Publish(original), Packet::Publish(decoded)) => {
            assert_eq!(original.topic_name, decoded.topic_name);
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.payload, decoded.payload);
            assert_eq!(original.qos, decoded.qos);
            assert_eq!(original.duplicate, decoded.duplicate);
            assert_eq!(original.retain, decoded.retain);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_puback_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    let puback_packet = Packet::PubAck(PubAckPacket { packet_id: 1234 });

    codec.encode(puback_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (puback_packet, decoded_packet) {
        (Packet::PubAck(original), Packet::PubAck(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_publish_packet_qos1_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    let payload = bytes::Bytes::from("QoS 1 message");
    let publish_packet = Packet::Publish(PublishPacket {
        topic_name: "test/qos1".to_string(),
        packet_id: Some(100),
        payload: payload.clone(),
        qos: QoS::AtLeastOnce,
        duplicate: false,
        retain: true,
    });

    codec.encode(publish_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (publish_packet, decoded_packet) {
        (Packet::Publish(original), Packet::Publish(decoded)) => {
            assert_eq!(original.topic_name, decoded.topic_name);
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.payload, decoded.payload);
            assert_eq!(original.qos, decoded.qos);
            assert_eq!(original.retain, decoded.retain);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_publish_packet_qos2_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let payload = bytes::Bytes::from("QoS 2 message");

    let publish_packet = Packet::Publish(PublishPacket {
        topic_name: "test/qos2".to_string(),
        packet_id: Some(200),
        payload: payload.clone(),
        qos: QoS::ExactlyOnce,
        duplicate: true,
        retain: false,
    });

    codec.encode(publish_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (publish_packet, decoded_packet) {
        (Packet::Publish(original), Packet::Publish(decoded)) => {
            assert_eq!(original.topic_name, decoded.topic_name);
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.payload, decoded.payload);
            assert_eq!(original.qos, decoded.qos);
            assert_eq!(original.duplicate, decoded.duplicate);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pubrec_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::PubRec(PubRecPacket { packet_id: 300 });

    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::PubRec(original), Packet::PubRec(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pubrel_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::PubRel(PubRelPacket { packet_id: 300 });

    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::PubRel(original), Packet::PubRel(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pubcomp_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::PubComp(PubCompPacket { packet_id: 300 });

    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::PubComp(original), Packet::PubComp(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_subscribe_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let subscribe_packet = Packet::Subscribe(SubscribePacket {
        packet_id: 1,
        topics: vec![
            ("sensor/+/temperature".to_string(), QoS::AtMostOnce),
            ("sensor/+/humidity".to_string(), QoS::AtLeastOnce),
            ("home/#".to_string(), QoS::ExactlyOnce),
        ],
    });

    codec.encode(subscribe_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();

    match (subscribe_packet, decoded_packet) {
        (Packet::Subscribe(original), Packet::Subscribe(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.topics.len(), decoded.topics.len());
            for (i, (orig_topic, orig_qos)) in original.topics.iter().enumerate() {
                let (dec_topic, dec_qos) = &decoded.topics[i];
                assert_eq!(orig_topic, dec_topic);
                assert_eq!(orig_qos, dec_qos);
            }
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_suback_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let suback_packet = Packet::SubAck(SubAckPacket {
        packet_id: 1,
        return_codes: vec![
            SubAckReturnCode::SuccessQoS0,
            SubAckReturnCode::SuccessQoS1,
            SubAckReturnCode::SuccessQoS2,
        ],
    });

    codec.encode(suback_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (suback_packet, decoded_packet) {
        (Packet::SubAck(original), Packet::SubAck(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.return_codes, decoded.return_codes);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_suback_packet_failure_codes() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let suback_packet = Packet::SubAck(SubAckPacket {
        packet_id: 2,
        return_codes: vec![SubAckReturnCode::SuccessQoS1, SubAckReturnCode::Failure],
    });

    codec.encode(suback_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (suback_packet, decoded_packet) {
        (Packet::SubAck(original), Packet::SubAck(decoded)) => {
            assert_eq!(original.return_codes[1], SubAckReturnCode::Failure);
            assert_eq!(original.return_codes, decoded.return_codes);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_unsubscribe_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::Unsubscribe(UnsubscribePacket {
        packet_id: 5,
        topics: vec![
            "sensor/+/temperature".to_string(),
            "home/livingroom/#".to_string(),
        ],
    });

    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::Unsubscribe(original), Packet::Unsubscribe(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
            assert_eq!(original.topics, decoded.topics);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_unsuback_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::UnsubAck(UnsubAckPacket { packet_id: 5 });
    codec.encode(packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::UnsubAck(original), Packet::UnsubAck(decoded)) => {
            assert_eq!(original.packet_id, decoded.packet_id);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pingreq_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::PingReq(PingReqPacket);
    codec.encode(packet.clone(), &mut buffer).unwrap();
    assert_eq!(buffer.len(), 2);
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::PingReq(_), Packet::PingReq(_)) => {}
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_pingresp_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::PingResp(PingRespPacket);
    codec.encode(packet.clone(), &mut buffer).unwrap();
    assert_eq!(buffer.len(), 2);
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::PingResp(_), Packet::PingResp(_)) => {}
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_disconnect_packet_encoding_decoding() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packet = Packet::Disconnect(DisconnectPacket);
    codec.encode(packet.clone(), &mut buffer).unwrap();
    assert_eq!(buffer.len(), 2);
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (packet, decoded_packet) {
        (Packet::Disconnect(_), Packet::Disconnect(_)) => {}
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_connect_with_credentials() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let connect_packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: false,
        will_flag: true,
        will_qos: QoS::AtLeastOnce,
        will_retain: false,
        password_flag: true,
        username_flag: true,
        keep_alive: 300,
        client_id: "client-with-auth".to_string(),
        will_topic: Some("client/will".to_string()),
        will_message: Some(bytes::Bytes::from("goodbye")),
        username: Some("admin".to_string()),
        password: Some(bytes::Bytes::from("secret123")),
    });

    codec.encode(connect_packet.clone(), &mut buffer).unwrap();
    let decoded_packet = codec.decode(&mut buffer).unwrap().unwrap();
    match (connect_packet, decoded_packet) {
        (Packet::Connect(original), Packet::Connect(decoded)) => {
            assert_eq!(original.client_id, decoded.client_id);
            assert_eq!(original.username, decoded.username);
            assert_eq!(original.password, decoded.password);
            assert_eq!(original.will_topic, decoded.will_topic);
            assert_eq!(original.will_message, decoded.will_message);
            assert_eq!(original.clean_session, decoded.clean_session);
        }
        _ => panic!("Packets don't match"),
    }
}

#[test]
fn test_multiple_packets_in_sequence() {
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();
    let packets = vec![
        Packet::Connect(ConnectPacket {
            protocol_name: "MQTT".to_string(),
            protocol_level: 4,
            clean_session: true,
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
            password_flag: false,
            username_flag: false,
            keep_alive: 60,
            client_id: "multi-test".to_string(),
            will_topic: None,
            will_message: None,
            username: None,
            password: None,
        }),
        Packet::ConnAck(ConnAckPacket {
            session_present: false,
            return_code: ConnectReturnCode::Accepted,
        }),
        Packet::Subscribe(SubscribePacket {
            packet_id: 1,
            topics: vec![("test/topic".to_string(), QoS::AtMostOnce)],
        }),
        Packet::SubAck(SubAckPacket {
            packet_id: 1,
            return_codes: vec![SubAckReturnCode::SuccessQoS0],
        }),
    ];

    for packet in &packets {
        codec.encode(packet.clone(), &mut buffer).unwrap();
    }
    for expected in packets {
        let decoded = codec.decode(&mut buffer).unwrap().unwrap();
        assert_eq!(expected.packet_type(), decoded.packet_type());
    }
    assert!(buffer.is_empty());
}
