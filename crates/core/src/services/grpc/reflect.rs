//! Server reflection client.
//!
//! The reflection messages are declared with `prost` derives so loading a
//! `.proto` or talking to a reflection server does not need `protoc`.
//! v1 is tried first; v1alpha uses the same wire messages on a different path.

use std::time::Duration;

use futures_util::StreamExt;
use prost_reflect::DescriptorPool;
use tokio::sync::mpsc;
use tonic::Request;
use tonic::transport::Channel;

use crate::error::{AppError, AppResult};

use super::codec::ReflectionCodec;
use super::endpoint::connect_channel;

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServerReflectionRequest {
    #[prost(string, tag = "1")]
    pub host: String,
    #[prost(
        oneof = "server_reflection_request::MessageRequest",
        tags = "3, 4, 5, 6, 7"
    )]
    pub message_request: Option<server_reflection_request::MessageRequest>,
}

pub mod server_reflection_request {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum MessageRequest {
        #[prost(string, tag = "3")]
        FileByFilename(String),
        #[prost(string, tag = "4")]
        FileContainingSymbol(String),
        #[prost(message, tag = "5")]
        FileContainingExtension(super::ExtensionRequest),
        #[prost(string, tag = "6")]
        AllExtensionNumbersOfType(String),
        #[prost(string, tag = "7")]
        ListServices(String),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ExtensionRequest {
    #[prost(string, tag = "1")]
    pub containing_type: String,
    #[prost(int32, tag = "2")]
    pub extension_number: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServerReflectionResponse {
    #[prost(string, tag = "1")]
    pub valid_host: String,
    #[prost(message, optional, tag = "2")]
    pub original_request: Option<ServerReflectionRequest>,
    #[prost(
        oneof = "server_reflection_response::MessageResponse",
        tags = "4, 5, 6, 7"
    )]
    pub message_response: Option<server_reflection_response::MessageResponse>,
}

pub mod server_reflection_response {
    #[derive(Clone, PartialEq, prost::Oneof)]
    #[allow(clippy::enum_variant_names)] // prost 生成代码，命名由 .proto 决定
    pub enum MessageResponse {
        #[prost(message, tag = "4")]
        FileDescriptorResponse(super::FileDescriptorResponse),
        #[prost(message, tag = "5")]
        AllExtensionNumbersResponse(super::ExtensionNumberResponse),
        #[prost(message, tag = "6")]
        ListServicesResponse(super::ListServiceResponse),
        #[prost(message, tag = "7")]
        ErrorResponse(super::ErrorResponse),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct FileDescriptorResponse {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub file_descriptor_proto: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ExtensionNumberResponse {
    #[prost(string, tag = "1")]
    pub base_type_name: String,
    #[prost(int32, repeated, tag = "2")]
    pub extension_number: Vec<i32>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ListServiceResponse {
    #[prost(message, repeated, tag = "1")]
    pub service: Vec<ServiceResponse>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServiceResponse {
    #[prost(string, tag = "1")]
    pub name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ErrorResponse {
    #[prost(int32, tag = "1")]
    pub error_code: i32,
    #[prost(string, tag = "2")]
    pub error_message: String,
}

const REFLECTION_PATHS: [&str; 2] = [
    "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
    "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
];

pub async fn discover(
    host: &str,
    tls: bool,
    metadata: &[(String, String)],
) -> AppResult<DescriptorPool> {
    let channel = connect_channel(host, tls, Some(Duration::from_secs(20))).await?;
    let mut last_error = AppError::Proto("server reflection is not available".into());
    for path in REFLECTION_PATHS {
        match reflect_on(channel.clone(), path, metadata).await {
            Ok(pool) => return Ok(pool),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn reflect_on(
    channel: Channel,
    path: &'static str,
    metadata: &[(String, String)],
) -> AppResult<DescriptorPool> {
    let path = http::uri::PathAndQuery::from_static(path);
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(ServerReflectionRequest {
        host: String::new(),
        message_request: Some(server_reflection_request::MessageRequest::ListServices(
            "*".into(),
        )),
    })
    .map_err(|_| AppError::Network("reflection channel closed".into()))?;

    let mut client = tonic::client::Grpc::new(channel);
    client
        .ready()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    let mut call = Request::new(RequestStream { rx });
    super::apply_metadata(&mut call, metadata)?;
    let response = client
        .streaming(call, path, ReflectionCodec)
        .await
        .map_err(status_error)?;
    let mut inbound = response.into_inner();
    let listed = next_frame(&mut inbound).await?;
    let services = match listed.message_response {
        Some(server_reflection_response::MessageResponse::ListServicesResponse(list)) => list
            .service
            .into_iter()
            .map(|service| service.name)
            .filter(|name| !name.starts_with("grpc.reflection."))
            .collect::<Vec<_>>(),
        Some(server_reflection_response::MessageResponse::ErrorResponse(error)) => {
            return Err(AppError::Proto(error.error_message));
        }
        _ => {
            return Err(AppError::Proto(
                "reflection server did not list services".into(),
            ));
        }
    };

    let mut pool = DescriptorPool::new();
    for service in services {
        tx.send(ServerReflectionRequest {
            host: String::new(),
            message_request: Some(
                server_reflection_request::MessageRequest::FileContainingSymbol(service.clone()),
            ),
        })
        .map_err(|_| AppError::Network("reflection channel closed".into()))?;
        let frame = next_frame(&mut inbound).await?;
        match frame.message_response {
            Some(server_reflection_response::MessageResponse::FileDescriptorResponse(files)) => {
                add_files(&mut pool, &files.file_descriptor_proto)?;
            }
            Some(server_reflection_response::MessageResponse::ErrorResponse(error)) => {
                return Err(AppError::Proto(format!(
                    "{service}: {}",
                    error.error_message
                )));
            }
            _ => {
                return Err(AppError::Proto(format!(
                    "reflection returned no descriptors for {service}"
                )));
            }
        }
    }
    Ok(pool)
}

fn add_files(pool: &mut DescriptorPool, files: &[Vec<u8>]) -> AppResult<()> {
    for file in files {
        if let Err(error) = pool.decode_file_descriptor_proto(file.as_slice()) {
            let text = error.to_string().to_lowercase();
            if text.contains("duplicate") || text.contains("already") {
                continue;
            }
            return Err(AppError::Proto(error.to_string()));
        }
    }
    Ok(())
}

async fn next_frame(
    inbound: &mut tonic::codec::Streaming<ServerReflectionResponse>,
) -> AppResult<ServerReflectionResponse> {
    match tokio::time::timeout(Duration::from_secs(15), inbound.next()).await {
        Ok(Some(Ok(frame))) => Ok(frame),
        Ok(Some(Err(status))) => Err(status_error(status)),
        Ok(None) => Err(AppError::Proto("reflection stream ended".into())),
        Err(_) => Err(AppError::Timeout("reflection timed out".into())),
    }
}

fn status_error(status: tonic::Status) -> AppError {
    let text = format!("{}: {}", status.code(), status.message());
    if status.code() == tonic::Code::DeadlineExceeded {
        AppError::Timeout(text)
    } else if status.code() == tonic::Code::Unauthenticated {
        AppError::Tls(text)
    } else {
        AppError::Proto(text)
    }
}

struct RequestStream {
    rx: mpsc::UnboundedReceiver<ServerReflectionRequest>,
}

impl futures_util::Stream for RequestStream {
    type Item = ServerReflectionRequest;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.get_mut().rx.poll_recv(cx)
    }
}
