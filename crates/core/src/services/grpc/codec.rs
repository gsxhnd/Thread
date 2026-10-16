//! Codecs for dynamic protobuf messages and the hand-written reflection types.

use prost::Message as ProstMessage;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::Status;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};

#[derive(Clone)]
pub struct DynamicCodec {
    pub request: MessageDescriptor,
    pub response: MessageDescriptor,
}

pub struct DynamicEncoder {
    _request: MessageDescriptor,
}

pub struct DynamicDecoder {
    response: MessageDescriptor,
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder {
            _request: self.request.clone(),
        }
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            response: self.response.clone(),
        }
    }
}

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        DynamicMessage::decode(self.response.clone(), src)
            .map(Some)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

#[derive(Clone, Default)]
pub struct ReflectionCodec;

pub struct ReflectionEncoder;
pub struct ReflectionDecoder;

impl Codec for ReflectionCodec {
    type Encode = super::reflect::ServerReflectionRequest;
    type Decode = super::reflect::ServerReflectionResponse;
    type Encoder = ReflectionEncoder;
    type Decoder = ReflectionDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        ReflectionEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        ReflectionDecoder
    }
}

impl Encoder for ReflectionEncoder {
    type Item = super::reflect::ServerReflectionRequest;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

impl Decoder for ReflectionDecoder {
    type Item = super::reflect::ServerReflectionResponse;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        super::reflect::ServerReflectionResponse::decode(src)
            .map(Some)
            .map_err(|error| Status::internal(error.to_string()))
    }
}
