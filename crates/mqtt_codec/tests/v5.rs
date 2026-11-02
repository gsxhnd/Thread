use bytes::BytesMut;
use mqtt_codec::v5::*;
use mqtt_codec::{Decoder, Encoder};

#[path = "v5/builder.rs"]
mod builder;
#[path = "v5/codec.rs"]
mod codec;
#[path = "v5/validation.rs"]
mod validation;
