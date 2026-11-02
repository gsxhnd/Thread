//! MQTT v5.0 property wire-format parsing.
//!
//! Handles property length, duplicate detection, and forward-compatible skipping
//! of unknown property identifiers.

use bytes::{Buf, Bytes};
use std::collections::HashSet;

use super::packet::{Properties, QoS};
use super::property_id::PropertyType;
use crate::MqttError;

const CONTINUATION_BIT: u8 = 0x80;
const LENGTH_MASK: u8 = 0x7F;
const MAX_VARIABLE_MULTIPLIER: usize = 128 * 128 * 128;

/// Properties that may appear more than once in a property list.
fn is_multi_value_property(id: u8) -> bool {
    matches!(id, 0x0B | 0x26)
}

/// Parses the Properties field from `buf` and advances past it.
pub fn parse_properties(buf: &mut &[u8]) -> Result<Properties, MqttError> {
    let mut properties = Properties::default();

    if buf.is_empty() {
        return Ok(properties);
    }

    let properties_len = read_variable_length(buf)?;
    if buf.len() < properties_len {
        return Err(MqttError::incomplete(properties_len, buf.len()));
    }

    let mut props_buf = &buf[..properties_len];
    *buf = &buf[properties_len..];

    let mut seen = HashSet::new();

    while !props_buf.is_empty() {
        let property_id = props_buf.get_u8();

        if !is_multi_value_property(property_id) && !seen.insert(property_id) {
            return Err(MqttError::protocol_violation(
                format!("Duplicate property ID: 0x{:02X}", property_id),
                None,
            ));
        }

        match property_id {
            0x01 => {
                properties.payload_format_indicator = Some(read_byte(&mut props_buf)?);
            }
            0x02 => {
                properties.message_expiry_interval = Some(read_u32(&mut props_buf)?);
            }
            0x03 => {
                properties.content_type = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x08 => {
                properties.response_topic = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x09 => {
                properties.correlation_data = Some(read_binary_data(&mut props_buf)?);
            }
            0x0B => {
                properties
                    .subscription_identifiers
                    .push(read_variable_length(&mut props_buf)? as u32);
            }
            0x11 => {
                properties.session_expiry_interval = Some(read_u32(&mut props_buf)?);
            }
            0x12 => {
                properties.assigned_client_identifier = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x13 => {
                properties.server_keep_alive = Some(read_u16(&mut props_buf)?);
            }
            0x15 => {
                properties.authentication_method = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x16 => {
                properties.authentication_data = Some(read_binary_data(&mut props_buf)?);
            }
            0x17 => {
                properties.request_problem_information =
                    Some(read_boolean(&mut props_buf, property_id)?);
            }
            0x18 => {
                properties.will_delay_interval = Some(read_u32(&mut props_buf)?);
            }
            0x19 => {
                properties.request_response_information =
                    Some(read_boolean(&mut props_buf, property_id)?);
            }
            0x1A => {
                properties.response_information = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x1C => {
                properties.server_reference = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x1F => {
                properties.reason_string = Some(parse_utf8_string(&mut props_buf)?);
            }
            0x21 => {
                properties.receive_maximum = Some(read_u16(&mut props_buf)?);
            }
            0x22 => {
                properties.topic_alias_maximum = Some(read_u16(&mut props_buf)?);
            }
            0x23 => {
                properties.topic_alias = Some(read_u16(&mut props_buf)?);
            }
            0x24 => {
                let qos_byte = read_byte(&mut props_buf)?;
                properties.maximum_qos = Some(QoS::try_from(qos_byte).map_err(|e| {
                    MqttError::protocol_violation(format!("Invalid Maximum QoS: {}", e), None)
                })?);
            }
            0x25 => {
                properties.retain_available = Some(read_boolean(&mut props_buf, property_id)?);
            }
            0x26 => {
                let key = parse_utf8_string(&mut props_buf)?;
                let value = parse_utf8_string(&mut props_buf)?;
                properties.user_properties.push((key, value));
            }
            0x27 => {
                properties.maximum_packet_size = Some(read_u32(&mut props_buf)?);
            }
            0x28 => {
                properties.wildcard_subscription_available =
                    Some(read_boolean(&mut props_buf, property_id)?);
            }
            0x29 => {
                properties.subscription_identifiers_available =
                    Some(read_boolean(&mut props_buf, property_id)?);
            }
            0x2A => {
                properties.shared_subscription_available =
                    Some(read_boolean(&mut props_buf, property_id)?);
            }
            unknown => {
                if let Some(property_type) = PropertyType::from_id(unknown) {
                    property_type.skip_value(&mut props_buf)?;
                } else {
                    return Err(MqttError::malformed(format!(
                        "Unknown property ID: 0x{:02X}",
                        unknown
                    )));
                }
            }
        }
    }

    Ok(properties)
}

/// Parses a required MQTT v5 Properties field from `buf` and advances past it.
pub fn parse_required_properties(buf: &mut &[u8]) -> Result<Properties, MqttError> {
    if buf.is_empty() {
        return Err(MqttError::incomplete(1, 0));
    }
    parse_properties(buf)
}

fn read_byte(buf: &mut &[u8]) -> Result<u8, MqttError> {
    if buf.is_empty() {
        return Err(MqttError::incomplete(1, 0));
    }
    Ok(buf.get_u8())
}

fn read_boolean(buf: &mut &[u8], property_id: u8) -> Result<bool, MqttError> {
    match read_byte(buf)? {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(MqttError::invalid_property(
            property_id,
            format!("Boolean value must be 0 or 1, got {value}"),
        )),
    }
}

fn read_u16(buf: &mut &[u8]) -> Result<u16, MqttError> {
    if buf.len() < 2 {
        return Err(MqttError::incomplete(2, buf.len()));
    }
    Ok(buf.get_u16())
}

fn read_u32(buf: &mut &[u8]) -> Result<u32, MqttError> {
    if buf.len() < 4 {
        return Err(MqttError::incomplete(4, buf.len()));
    }
    Ok(buf.get_u32())
}

fn read_binary_data(buf: &mut &[u8]) -> Result<Bytes, MqttError> {
    let len = read_u16(buf)? as usize;
    if buf.len() < len {
        return Err(MqttError::incomplete(len, buf.len()));
    }
    let data = Bytes::copy_from_slice(&buf[..len]);
    buf.advance(len);
    Ok(data)
}

fn parse_utf8_string(buf: &mut &[u8]) -> Result<String, MqttError> {
    let len = read_u16(buf)? as usize;
    if buf.len() < len {
        return Err(MqttError::incomplete(len, buf.len()));
    }
    let s = std::str::from_utf8(&buf[..len])?.to_string();
    buf.advance(len);
    Ok(s)
}

fn read_variable_length(buf: &mut &[u8]) -> Result<usize, MqttError> {
    if buf.is_empty() {
        return Err(MqttError::incomplete(1, 0));
    }

    let mut multiplier = 1usize;
    let mut value = 0usize;

    loop {
        if buf.is_empty() {
            return Err(MqttError::incomplete(1, 0));
        }

        let encoded_byte = buf.get_u8();
        value += (encoded_byte & LENGTH_MASK) as usize * multiplier;

        if (encoded_byte & CONTINUATION_BIT) == 0 {
            return Ok(value);
        }

        multiplier *= 128;
        if multiplier > MAX_VARIABLE_MULTIPLIER {
            return Err(MqttError::InvalidRemainingLength { length: value });
        }
    }
}
