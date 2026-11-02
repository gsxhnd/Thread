use mqtt_codec::v4::{ConnectReturnCode, SubAckReturnCode};

#[test]
fn test_connect_return_code_try_from() {
    assert_eq!(
        ConnectReturnCode::try_from(0x00),
        Ok(ConnectReturnCode::Accepted)
    );
    assert_eq!(
        ConnectReturnCode::try_from(0x01),
        Ok(ConnectReturnCode::UnacceptableProtocolVersion)
    );
    assert_eq!(
        ConnectReturnCode::try_from(0x05),
        Ok(ConnectReturnCode::NotAuthorized)
    );
    assert_eq!(ConnectReturnCode::try_from(0x06), Err(0x06));
    assert_eq!(ConnectReturnCode::try_from(0xFF), Err(0xFF));
}

#[test]
fn test_connect_return_code_into_u8() {
    let code: u8 = ConnectReturnCode::Accepted.into();
    assert_eq!(code, 0x00);

    let code: u8 = ConnectReturnCode::NotAuthorized.into();
    assert_eq!(code, 0x05);
}

#[test]
fn test_connect_return_code_is_success() {
    assert!(ConnectReturnCode::Accepted.is_success());
    assert!(!ConnectReturnCode::NotAuthorized.is_success());
}

#[test]
fn test_connect_return_code_is_error() {
    assert!(ConnectReturnCode::NotAuthorized.is_error());
    assert!(!ConnectReturnCode::Accepted.is_error());
}

#[test]
fn test_sub_ack_return_code_try_from() {
    assert_eq!(
        SubAckReturnCode::try_from(0x00),
        Ok(SubAckReturnCode::SuccessQoS0)
    );
    assert_eq!(
        SubAckReturnCode::try_from(0x01),
        Ok(SubAckReturnCode::SuccessQoS1)
    );
    assert_eq!(
        SubAckReturnCode::try_from(0x02),
        Ok(SubAckReturnCode::SuccessQoS2)
    );
    assert_eq!(
        SubAckReturnCode::try_from(0x80),
        Ok(SubAckReturnCode::Failure)
    );
    // Invalid codes
    assert_eq!(SubAckReturnCode::try_from(0x03), Err(0x03));
    assert_eq!(SubAckReturnCode::try_from(0x7F), Err(0x7F));
    assert_eq!(SubAckReturnCode::try_from(0x81), Err(0x81));
}

#[test]
fn test_sub_ack_return_code_into_u8() {
    let code: u8 = SubAckReturnCode::SuccessQoS0.into();
    assert_eq!(code, 0x00);

    let code: u8 = SubAckReturnCode::Failure.into();
    assert_eq!(code, 0x80);
}

#[test]
fn test_sub_ack_return_code_is_success() {
    assert!(SubAckReturnCode::SuccessQoS0.is_success());
    assert!(SubAckReturnCode::SuccessQoS1.is_success());
    assert!(SubAckReturnCode::SuccessQoS2.is_success());
    assert!(!SubAckReturnCode::Failure.is_success());
}

#[test]
fn test_sub_ack_return_code_qos() {
    use mqtt_codec::v4::QoS;

    assert_eq!(SubAckReturnCode::SuccessQoS0.qos(), Some(QoS::AtMostOnce));
    assert_eq!(SubAckReturnCode::SuccessQoS1.qos(), Some(QoS::AtLeastOnce));
    assert_eq!(SubAckReturnCode::SuccessQoS2.qos(), Some(QoS::ExactlyOnce));
    assert_eq!(SubAckReturnCode::Failure.qos(), None);
}

#[test]
fn test_display() {
    assert_eq!(
        format!("{}", ConnectReturnCode::Accepted),
        "Connection accepted"
    );
    assert_eq!(format!("{}", SubAckReturnCode::Failure), "Failure");
}
