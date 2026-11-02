use bytes::BytesMut;
use mqtt_codec::Encoder;
use mqtt_codec::v4::*;

fn main() {
    println!("=== MQTT v3.1.1 Encoding Examples ===\n");

    // 1. CONNECT packet
    let connect_packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 4,
        clean_session: true,
        will_flag: true,
        will_qos: QoS::AtLeastOnce,
        will_retain: false,
        password_flag: true,
        username_flag: true,
        keep_alive: 60,
        client_id: "rust-mqtt-client".to_string(),
        will_topic: Some("client/will".to_string()),
        will_message: Some(bytes::Bytes::from("client disconnected")),
        username: Some("user".to_string()),
        password: Some(bytes::Bytes::from("pass")),
    });

    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    match codec.encode(connect_packet, &mut buffer) {
        Ok(_) => println!("1. CONNECT packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding CONNECT: {}", e),
    }

    // 2. CONNACK packet
    let connack_packet = Packet::ConnAck(ConnAckPacket {
        session_present: false,
        return_code: ConnectReturnCode::Accepted,
    });

    buffer.clear();
    match codec.encode(connack_packet, &mut buffer) {
        Ok(_) => println!("2. CONNACK packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding CONNACK: {}", e),
    }

    // 3. PUBLISH packet with QoS 0
    let publish_qos0 = Packet::Publish(PublishPacket {
        topic_name: "sensor/temperature".to_string(),
        packet_id: None,
        payload: bytes::Bytes::from(r#"{"temp": 23.5}"#),
        qos: QoS::AtMostOnce,
        duplicate: false,
        retain: true,
    });

    buffer.clear();
    match codec.encode(publish_qos0, &mut buffer) {
        Ok(_) => println!("3. PUBLISH QoS 0 packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding PUBLISH: {}", e),
    }

    // 4. PUBLISH packet with QoS 1
    let publish_qos1 = Packet::Publish(PublishPacket {
        topic_name: "sensor/humidity".to_string(),
        packet_id: Some(1),
        payload: bytes::Bytes::from("65%"),
        qos: QoS::AtLeastOnce,
        duplicate: false,
        retain: false,
    });

    buffer.clear();
    match codec.encode(publish_qos1, &mut buffer) {
        Ok(_) => println!("4. PUBLISH QoS 1 packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding PUBLISH: {}", e),
    }

    // 5. PUBACK packet
    let puback_packet = Packet::PubAck(PubAckPacket { packet_id: 1 });

    buffer.clear();
    match codec.encode(puback_packet, &mut buffer) {
        Ok(_) => println!("5. PUBACK packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding PUBACK: {}", e),
    }

    // 6. SUBSCRIBE packet
    let subscribe_packet = Packet::Subscribe(SubscribePacket {
        packet_id: 1,
        topics: vec![
            ("sensor/+/temperature".to_string(), QoS::AtMostOnce),
            ("sensor/+/humidity".to_string(), QoS::AtLeastOnce),
        ],
    });

    buffer.clear();
    match codec.encode(subscribe_packet, &mut buffer) {
        Ok(_) => println!("6. SUBSCRIBE packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding SUBSCRIBE: {}", e),
    }

    // 7. SUBACK packet
    let suback_packet = Packet::SubAck(SubAckPacket {
        packet_id: 1,
        return_codes: vec![SubAckReturnCode::SuccessQoS0, SubAckReturnCode::SuccessQoS1],
    });

    buffer.clear();
    match codec.encode(suback_packet, &mut buffer) {
        Ok(_) => println!("7. SUBACK packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding SUBACK: {}", e),
    }

    // 8. UNSUBSCRIBE packet
    let unsubscribe_packet = Packet::Unsubscribe(UnsubscribePacket {
        packet_id: 2,
        topics: vec!["sensor/+/temperature".to_string()],
    });

    buffer.clear();
    match codec.encode(unsubscribe_packet, &mut buffer) {
        Ok(_) => println!("8. UNSUBSCRIBE packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding UNSUBSCRIBE: {}", e),
    }

    // 9. UNSUBACK packet
    let unsuback_packet = Packet::UnsubAck(UnsubAckPacket { packet_id: 2 });

    buffer.clear();
    match codec.encode(unsuback_packet, &mut buffer) {
        Ok(_) => println!("9. UNSUBACK packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding UNSUBACK: {}", e),
    }

    // 10. PINGREQ packet
    let pingreq_packet = Packet::PingReq(PingReqPacket);

    buffer.clear();
    match codec.encode(pingreq_packet, &mut buffer) {
        Ok(_) => println!("10. PINGREQ packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding PINGREQ: {}", e),
    }

    // 11. PINGRESP packet
    let pingresp_packet = Packet::PingResp(PingRespPacket);

    buffer.clear();
    match codec.encode(pingresp_packet, &mut buffer) {
        Ok(_) => println!("11. PINGRESP packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding PINGRESP: {}", e),
    }

    // 12. DISCONNECT packet
    let disconnect_packet = Packet::Disconnect(DisconnectPacket);

    buffer.clear();
    match codec.encode(disconnect_packet, &mut buffer) {
        Ok(_) => println!("12. DISCONNECT packet encoded: {} bytes", buffer.len()),
        Err(e) => eprintln!("Error encoding DISCONNECT: {}", e),
    }

    println!("\n=== All v3.1.1 packets encoded successfully ===");
}
