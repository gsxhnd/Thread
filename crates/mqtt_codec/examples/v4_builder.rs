use bytes::Bytes;
use mqtt_codec::v4::*;
use mqtt_codec::{Decoder, Encoder};

fn main() {
    println!("=== MQTT v3.1.1 Builder Pattern Examples ===\n");

    let mut codec = MqttCodec::new();

    // 1. CONNECT with Builder
    let connect = ConnectPacketBuilder::new("my-client-id")
        .keep_alive(30)
        .clean_session(true)
        .username("admin")
        .password(Bytes::from("secret"))
        .will("will/topic", "client crashed", QoS::AtLeastOnce, false)
        .build();

    println!("1. CONNECT with credentials and will message");
    println!("   Client ID: {}", connect.client_id);
    println!("   Keep Alive: {}s", connect.keep_alive);
    println!("   Has username: {}", connect.username.is_some());

    // Encode and decode
    let mut buffer = BytesMut::new();
    codec.encode(Packet::Connect(connect), &mut buffer).unwrap();
    println!("   Encoded size: {} bytes", buffer.len());

    // 2. PUBLISH with Builder - QoS 0
    let publish_qos0 = PublishPacketBuilder::new("sensor/temperature", Bytes::from("23.5"))
        .retain(true)
        .build();

    println!("\n2. PUBLISH QoS 0 with retain");
    println!("   Topic: {}", publish_qos0.topic_name);
    println!("   QoS: {:?}", publish_qos0.qos);
    println!("   Retain: {}", publish_qos0.retain);

    // 3. PUBLISH with Builder - QoS 1
    let publish_qos1 = PublishPacketBuilder::new("commands/restart", Bytes::from("now"))
        .qos(QoS::AtLeastOnce)
        .packet_id(42)
        .build();

    println!("\n3. PUBLISH QoS 1 with packet ID");
    println!("   Topic: {}", publish_qos1.topic_name);
    println!("   QoS: {:?}", publish_qos1.qos);
    println!("   Packet ID: {:?}", publish_qos1.packet_id);

    // 4. SUBSCRIBE with Builder
    let subscribe = SubscribePacketBuilder::new(1)
        .topic("home/+/temperature", QoS::AtMostOnce)
        .topic("home/+/humidity", QoS::AtLeastOnce)
        .topic("home/livingroom/#", QoS::ExactlyOnce)
        .build()
        .expect("SUBSCRIBE should have at least one topic");

    println!("\n4. SUBSCRIBE with multiple topics");
    println!("   Packet ID: {}", subscribe.packet_id);
    println!("   Topics count: {}", subscribe.topics.len());
    for (i, (topic, qos)) in subscribe.topics.iter().enumerate() {
        println!("   [{}] {} (QoS: {:?})", i + 1, topic, qos);
    }

    // 5. UNSUBSCRIBE with Builder
    let unsubscribe = UnsubscribePacketBuilder::new(2)
        .topic("home/+/temperature")
        .build()
        .expect("UNSUBSCRIBE should have at least one topic");

    println!("\n5. UNSUBSCRIBE");
    println!("   Packet ID: {}", unsubscribe.packet_id);
    println!("   Topics: {:?}", unsubscribe.topics);

    // 6. Full round-trip test
    println!("\n6. Round-trip encode/decode test");

    let original = ConnectPacketBuilder::new("roundtrip-client")
        .keep_alive(120)
        .clean_session(false)
        .username("testuser")
        .password(Bytes::from("testpass"))
        .build();

    let mut buffer = BytesMut::new();
    codec
        .encode(Packet::Connect(original.clone()), &mut buffer)
        .unwrap();

    if let Some(Packet::Connect(decoded)) = codec.decode(&mut buffer).unwrap() {
        assert_eq!(original.client_id, decoded.client_id);
        assert_eq!(original.keep_alive, decoded.keep_alive);
        assert_eq!(original.clean_session, decoded.clean_session);
        println!("   Round-trip successful!");
        println!("   Client ID: {}", decoded.client_id);
        println!("   Keep Alive: {}s", decoded.keep_alive);
        println!("   Clean Session: {}", decoded.clean_session);
    }

    println!("\n=== Builder pattern examples completed ===");
}

// Helper type for the example
type BytesMut = bytes::BytesMut;
