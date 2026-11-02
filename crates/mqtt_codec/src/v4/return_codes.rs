//! Return code definitions for MQTT v3.1.1.
//!
//! This module defines the return codes used in CONNACK and SUBACK packets
//! according to the MQTT v3.1.1 specification.

use std::fmt;

/// Connect return codes used in CONNACK packets.
///
/// These codes indicate the result of a connection attempt.
/// According to MQTT v3.1.1 specification [MQTT-3.2.2.3].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ConnectReturnCode {
    /// Connection accepted (0x00).
    ///
    /// The connection has been accepted.
    Accepted = 0x00,

    /// Connection refused, unacceptable protocol version (0x01).
    ///
    /// The Server does not support the level of the MQTT protocol
    /// requested by the Client.
    UnacceptableProtocolVersion = 0x01,

    /// Connection refused, identifier rejected (0x02).
    ///
    /// The Client identifier is correct UTF-8 but not allowed by the Server.
    IdentifierRejected = 0x02,

    /// Connection refused, Server unavailable (0x03).
    ///
    /// The Network Connection has been made but the MQTT service is unavailable.
    ServerUnavailable = 0x03,

    /// Connection refused, bad user name or password (0x04).
    ///
    /// The data in the user name or password is malformed.
    BadUserNameOrPassword = 0x04,

    /// Connection refused, not authorized (0x05).
    ///
    /// The Client is not authorized to connect.
    NotAuthorized = 0x05,
}

impl ConnectReturnCode {
    /// Returns the byte value of this return code.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns true if this return code indicates success.
    pub fn is_success(self) -> bool {
        matches!(self, ConnectReturnCode::Accepted)
    }

    /// Returns true if this return code indicates an error.
    pub fn is_error(self) -> bool {
        !self.is_success()
    }

    /// Returns a human-readable description of this return code.
    pub fn description(self) -> &'static str {
        match self {
            ConnectReturnCode::Accepted => "Connection accepted",
            ConnectReturnCode::UnacceptableProtocolVersion => {
                "Connection refused: unacceptable protocol version"
            }
            ConnectReturnCode::IdentifierRejected => "Connection refused: identifier rejected",
            ConnectReturnCode::ServerUnavailable => "Connection refused: server unavailable",
            ConnectReturnCode::BadUserNameOrPassword => {
                "Connection refused: bad user name or password"
            }
            ConnectReturnCode::NotAuthorized => "Connection refused: not authorized",
        }
    }
}

impl TryFrom<u8> for ConnectReturnCode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(ConnectReturnCode::Accepted),
            0x01 => Ok(ConnectReturnCode::UnacceptableProtocolVersion),
            0x02 => Ok(ConnectReturnCode::IdentifierRejected),
            0x03 => Ok(ConnectReturnCode::ServerUnavailable),
            0x04 => Ok(ConnectReturnCode::BadUserNameOrPassword),
            0x05 => Ok(ConnectReturnCode::NotAuthorized),
            other => Err(other),
        }
    }
}

impl From<ConnectReturnCode> for u8 {
    fn from(code: ConnectReturnCode) -> Self {
        code.as_u8()
    }
}

impl fmt::Display for ConnectReturnCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Subscribe return codes used in SUBACK packets.
///
/// These codes indicate the result of each topic subscription request.
/// According to MQTT v3.1.1 specification [MQTT-3.9.3].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SubAckReturnCode {
    /// Success - Maximum QoS 0 (0x00).
    ///
    /// The subscription is accepted and the maximum QoS sent will be 0.
    /// This might be a lower QoS than was requested.
    SuccessQoS0 = 0x00,

    /// Success - Maximum QoS 1 (0x01).
    ///
    /// The subscription is accepted and the maximum QoS sent will be 1.
    /// This might be a lower QoS than was requested.
    SuccessQoS1 = 0x01,

    /// Success - Maximum QoS 2 (0x02).
    ///
    /// The subscription is accepted and any QoS will be sent.
    SuccessQoS2 = 0x02,

    /// Failure (0x80).
    ///
    /// The subscription is not accepted and the Server does not wish to
    /// reveal the reason or none of the reason codes apply.
    Failure = 0x80,
}

impl SubAckReturnCode {
    /// Returns the byte value of this return code.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns true if this return code indicates success.
    pub fn is_success(self) -> bool {
        matches!(
            self,
            SubAckReturnCode::SuccessQoS0
                | SubAckReturnCode::SuccessQoS1
                | SubAckReturnCode::SuccessQoS2
        )
    }

    /// Returns true if this return code indicates failure.
    pub fn is_failure(self) -> bool {
        matches!(self, SubAckReturnCode::Failure)
    }

    /// Returns the QoS level for successful subscriptions.
    ///
    /// Returns `None` for failure.
    pub fn qos(self) -> Option<super::packet::QoS> {
        match self {
            SubAckReturnCode::SuccessQoS0 => Some(super::packet::QoS::AtMostOnce),
            SubAckReturnCode::SuccessQoS1 => Some(super::packet::QoS::AtLeastOnce),
            SubAckReturnCode::SuccessQoS2 => Some(super::packet::QoS::ExactlyOnce),
            SubAckReturnCode::Failure => None,
        }
    }

    /// Returns a human-readable description of this return code.
    pub fn description(self) -> &'static str {
        match self {
            SubAckReturnCode::SuccessQoS0 => "Success - Maximum QoS 0",
            SubAckReturnCode::SuccessQoS1 => "Success - Maximum QoS 1",
            SubAckReturnCode::SuccessQoS2 => "Success - Maximum QoS 2",
            SubAckReturnCode::Failure => "Failure",
        }
    }
}

impl TryFrom<u8> for SubAckReturnCode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(SubAckReturnCode::SuccessQoS0),
            0x01 => Ok(SubAckReturnCode::SuccessQoS1),
            0x02 => Ok(SubAckReturnCode::SuccessQoS2),
            0x80 => Ok(SubAckReturnCode::Failure),
            other => Err(other),
        }
    }
}

impl From<SubAckReturnCode> for u8 {
    fn from(code: SubAckReturnCode) -> Self {
        code.as_u8()
    }
}

impl fmt::Display for SubAckReturnCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}
