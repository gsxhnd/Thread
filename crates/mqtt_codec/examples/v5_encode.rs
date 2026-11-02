use bytes::BytesMut;
use mqtt_codec::Encoder;
use mqtt_codec::v5::*;

fn main() {
    // Create a MQTT 5.0 Connect packet
    let properties = Properties {
        session_expiry_interval: Some(3600),
        maximum_packet_size: Some(1024 * 64),
        ..Default::default()
    };

    let connect_packet = Packet::Connect(ConnectPacket {
        protocol_name: "MQTT".to_string(),
        protocol_level: 5,
        clean_start: true,
        will_flag: false,
        will_qos: QoS::AtMostOnce,
        will_retain: false,
        password_flag: false,
        username_flag: false,
        keep_alive: 60,
        properties,
        client_id: "rust-mqtt-client".to_string(),
        will_topic: None,
        will_message: None,
        will_properties: None,
        username: None,
        password: None,
    });

    // Encode the packet
    let mut codec = MqttCodec::new();
    let mut buffer = BytesMut::new();

    match codec.encode(connect_packet, &mut buffer) {
        Ok(_) => {
            println!("Successfully encoded MQTT 5.0 Connect packet");
            println!("Encoded bytes: {} bytes", buffer.len());
        }
        Err(e) => {
            eprintln!("Error encoding packet: {}", e);
        }
    }

    // Example of Publish packet with properties
    let pub_properties = Properties {
        message_expiry_interval: Some(3600),
        content_type: Some("application/json".to_string()),
        ..Default::default()
    };

    let publish_packet = Packet::Publish(PublishPacket {
        topic_name: "sensor/temperature".to_string(),
        packet_id: Some(1),
        payload: bytes::Bytes::from(r#"{"temp": 23.5, "humidity": 65}"#),
        qos: QoS::AtLeastOnce,
        duplicate: false,
        retain: false,
        properties: pub_properties,
    });

    let mut buffer = BytesMut::new();
    match codec.encode(publish_packet, &mut buffer) {
        Ok(_) => {
            println!("\nSuccessfully encoded MQTT 5.0 Publish packet");
            println!("Encoded bytes: {} bytes", buffer.len());
        }
        Err(e) => {
            eprintln!("Error encoding packet: {}", e);
        }
    }
}
