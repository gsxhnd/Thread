use bytes::Bytes;
use mqtt_codec::v5::*;
use mqtt_codec::{Decoder, Encoder};

fn main() {
    println!("=== MQTT v5.0 Builder Pattern Examples ===\n");

    let mut codec = MqttCodec::new();

    // 1. CONNECT with v5.0 properties
    let connect = ConnectPacketBuilder::new("v5-client")
        .keep_alive(60)
        .clean_start(true)
        .session_expiry_interval(3600)
        .receive_maximum(100)
        .maximum_packet_size(65535)
        .authentication_method("SCRAM-SHA-256")
        .authentication_data(Bytes::from("auth-data"))
        .username("user")
        .password(Bytes::from("pass"))
        .will("will/topic", "disconnected", QoS::AtLeastOnce, false)
        .will_delay_interval(300)
        .build();

    println!("1. CONNECT with v5.0 properties");
    println!("   Client ID: {}", connect.client_id);
    println!(
        "   Session Expiry: {:?}s",
        connect.properties.session_expiry_interval
    );
    println!(
        "   Receive Maximum: {:?}",
        connect.properties.receive_maximum
    );
    println!(
        "   Auth Method: {:?}",
        connect.properties.authentication_method
    );

    // 2. PUBLISH with properties
    let publish = PublishPacketBuilder::new("sensor/data", Bytes::from(r#"{"temp": 23.5}"#))
        .qos(QoS::AtLeastOnce)
        .packet_id(1)
        .message_expiry_interval(300)
        .content_type("application/json")
        .response_topic("response/data")
        .correlation_data(Bytes::from("corr-123"))
        .user_property("device-id", "dev-001")
        .user_property("location", "room-101")
        .build();

    println!("\n2. PUBLISH with v5.0 properties");
    println!("   Topic: {}", publish.topic_name);
    println!("   Content Type: {:?}", publish.properties.content_type);
    println!("   Response Topic: {:?}", publish.properties.response_topic);
    println!(
        "   User Properties: {:?}",
        publish.properties.user_properties
    );

    // 3. SUBSCRIBE with subscription options
    let subscribe = SubscribePacketBuilder::new(1)
        .topic_with_options(
            "home/+/temperature",
            QoS::AtLeastOnce,
            true,  // no_local
            false, // retain_as_published
            0,     // retain_handling
        )
        .topic_with_options("home/+/humidity", QoS::AtMostOnce, false, true, 1)
        .build()
        .expect("SUBSCRIBE should have at least one topic");

    println!("\n3. SUBSCRIBE with subscription options");
    println!("   Packet ID: {}", subscribe.packet_id);
    for (i, opt) in subscribe.topics.iter().enumerate() {
        println!(
            "   [{}] {} (QoS: {:?}, no_local: {}, retain_handling: {})",
            i + 1,
            opt.topic_filter,
            opt.qos,
            opt.no_local,
            opt.retain_handling
        );
    }

    // 4. UNSUBSCRIBE
    let unsubscribe = UnsubscribePacketBuilder::new(2)
        .topic("home/+/temperature")
        .topic("home/+/humidity")
        .build()
        .expect("UNSUBSCRIBE should have at least one topic");

    println!("\n4. UNSUBSCRIBE");
    println!("   Packet ID: {}", unsubscribe.packet_id);
    println!("   Topics: {:?}", unsubscribe.topics);

    // 5. DISCONNECT with reason code
    let disconnect = DisconnectPacket {
        reason_code: ReasonCode::Success,
        properties: Properties {
            reason_string: Some("Client shutting down".to_string()),
            session_expiry_interval: Some(0),
            ..Default::default()
        },
    };

    println!("\n5. DISCONNECT with reason code");
    println!("   Reason Code: {:?}", disconnect.reason_code);
    println!(
        "   Reason String: {:?}",
        disconnect.properties.reason_string
    );

    // 6. AUTH packet
    let auth = AuthPacket {
        reason_code: ReasonCode::ContinueAuthentication,
        properties: Properties {
            authentication_method: Some("SCRAM-SHA-256".to_string()),
            authentication_data: Some(Bytes::from("challenge-response")),
            ..Default::default()
        },
    };

    println!("\n6. AUTH for re-authentication");
    println!("   Reason Code: {:?}", auth.reason_code);
    println!(
        "   Auth Method: {:?}",
        auth.properties.authentication_method
    );

    // 7. Round-trip encode/decode with properties
    println!("\n7. Round-trip encode/decode test");

    let original = ConnectPacketBuilder::new("roundtrip-v5")
        .session_expiry_interval(7200)
        .receive_maximum(50)
        .topic_alias_maximum(10)
        .build();

    let mut buffer = bytes::BytesMut::new();
    codec
        .encode(Packet::Connect(original.clone()), &mut buffer)
        .unwrap();

    if let Some(Packet::Connect(decoded)) = codec.decode(&mut buffer).unwrap() {
        assert_eq!(original.client_id, decoded.client_id);
        assert_eq!(
            original.properties.session_expiry_interval,
            decoded.properties.session_expiry_interval
        );
        assert_eq!(
            original.properties.receive_maximum,
            decoded.properties.receive_maximum
        );
        println!("   Round-trip successful!");
        println!("   Client ID: {}", decoded.client_id);
        println!(
            "   Session Expiry: {:?}s",
            decoded.properties.session_expiry_interval
        );
        println!(
            "   Receive Maximum: {:?}",
            decoded.properties.receive_maximum
        );
    }

    println!("\n=== v5.0 Builder pattern examples completed ===");
}
