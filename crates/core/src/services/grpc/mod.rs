//! Dynamic gRPC client: `.proto` files, server reflection, and all four call modes.
//!
//! Payloads are canonical protobuf JSON. Unary calls return one JSON value.
//! Streaming calls push each message through a sink and can be cancelled.

mod codec;
mod endpoint;
mod reflect;

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use parking_lot::Mutex;
use prost_reflect::{
    DescriptorPool, DeserializeOptions, DynamicMessage, MessageDescriptor, MethodDescriptor,
    SerializeOptions, ServiceDescriptor,
};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};

use self::codec::DynamicCodec;
use self::endpoint::connect_channel;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodInfo {
    pub name: String,
    pub full_name: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub request_type: String,
    pub response_type: String,
    pub request_template: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoDefinition {
    pub pool_id: String,
    pub services: Vec<ServiceInfo>,
}

#[derive(Debug, Clone)]
pub struct GrpcInvoke {
    pub host: String,
    pub tls: bool,
    pub metadata: Vec<(String, String)>,
    pub pool_id: String,
    pub service: String,
    pub method: String,
    pub payload: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcResult {
    pub service: String,
    pub method: String,
    pub response: Value,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    pub stream_id: String,
    pub direction: String,
    pub data: Option<Value>,
    pub done: bool,
    pub error: Option<String>,
}

pub type StreamSink = Arc<dyn Fn(StreamEvent) + Send + Sync>;

struct StreamSession {
    input: MessageDescriptor,
    outgoing: Option<mpsc::UnboundedSender<DynamicMessage>>,
    abort: tokio::task::AbortHandle,
    client_streaming: bool,
}

/// Descriptor pools and live streams. Pools survive until the process exits;
/// streams are removed when their task finishes or is cancelled.
#[derive(Default)]
pub struct ProtoCatalog {
    pools: Mutex<std::collections::BTreeMap<String, Arc<DescriptorPool>>>,
    streams: Mutex<std::collections::BTreeMap<String, StreamSession>>,
    /// Runtime used when `open_stream` is called from the UI thread.
    io: Mutex<Option<tokio::runtime::Handle>>,
}

impl ProtoCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn bind_runtime(&self, handle: tokio::runtime::Handle) {
        crate::services::bind_io(&self.io, handle);
    }

    pub fn load_file(&self, path: &Path) -> AppResult<ProtoDefinition> {
        if !path.is_file() {
            return Err(AppError::NotFound(format!(
                "proto file not found: {}",
                path.display()
            )));
        }
        let include = path.parent().unwrap_or(Path::new("."));
        let set = protox::compile([path], [include])
            .map_err(|error| AppError::Proto(error.to_string()))?;
        let pool = DescriptorPool::from_file_descriptor_set(set)
            .map_err(|error| AppError::Proto(error.to_string()))?;
        Ok(self.insert_pool(pool))
    }

    pub async fn reflect(
        &self,
        host: &str,
        tls: bool,
        metadata: &[(String, String)],
    ) -> AppResult<ProtoDefinition> {
        let pool = reflect::discover(host, tls, metadata).await?;
        if pool.services().next().is_none() {
            return Err(AppError::Proto(
                "reflection returned no application services".into(),
            ));
        }
        Ok(self.insert_pool(pool))
    }

    pub async fn unary(&self, request: &GrpcInvoke) -> AppResult<GrpcResult> {
        let method = self.method(request)?;
        if method.is_client_streaming() || method.is_server_streaming() {
            return Err(AppError::Validation(
                "streaming methods use grpc_open_stream".into(),
            ));
        }
        let message = message_from_json(method.input(), &request.payload)?;
        let started = Instant::now();
        let channel = connect_channel(&request.host, request.tls, Some(request.timeout)).await?;
        let mut client = tonic::client::Grpc::new(channel);
        client.ready().await.map_err(endpoint::transport_error)?;
        let path = method_path(&method)?;
        let codec = DynamicCodec {
            request: method.input(),
            response: method.output(),
        };
        let mut call = tonic::Request::new(message);
        call.set_timeout(request.timeout);
        apply_metadata(&mut call, &request.metadata)?;
        let response = client
            .unary(call, path, codec)
            .await
            .map_err(status_error)?;
        Ok(GrpcResult {
            service: request.service.clone(),
            method: request.method.clone(),
            response: message_to_json(response.get_ref())?,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub fn open_stream(
        self: &Arc<Self>,
        request: GrpcInvoke,
        sink: StreamSink,
    ) -> AppResult<String> {
        let method = self.method(&request)?;
        if !method.is_client_streaming() && !method.is_server_streaming() {
            return Err(AppError::Validation("unary methods use grpc_invoke".into()));
        }
        let first = if method.is_client_streaming() && request.payload.trim().is_empty() {
            None
        } else {
            Some(message_from_json(method.input(), &request.payload)?)
        };
        let id = crate::db::new_id();
        let (tx, rx) = mpsc::unbounded_channel();
        if let Some(message) = first {
            tx.send(message)
                .map_err(|_| AppError::Network("stream closed".into()))?;
        }
        let input = method.input();
        let client_streaming = method.is_client_streaming();
        let outgoing = if client_streaming { Some(tx) } else { None };
        let catalog = Arc::clone(self);
        let id_task = id.clone();
        let handle = crate::services::spawn_io(&self.io, async move {
            let result = drive_stream(request, method, rx, &id_task, &sink).await;
            if let Err(error) = result {
                sink(StreamEvent {
                    stream_id: id_task.clone(),
                    direction: "in".into(),
                    data: None,
                    done: false,
                    error: Some(error.to_string()),
                });
            }
            sink(StreamEvent {
                stream_id: id_task.clone(),
                direction: "in".into(),
                data: None,
                done: true,
                error: None,
            });
            catalog.streams.lock().remove(&id_task);
        })?;
        self.streams.lock().insert(
            id.clone(),
            StreamSession {
                input,
                outgoing,
                abort: handle.abort_handle(),
                client_streaming,
            },
        );
        Ok(id)
    }

    pub fn send_stream(&self, id: &str, payload: &str) -> AppResult<()> {
        let streams = self.streams.lock();
        let Some(session) = streams.get(id) else {
            return Err(AppError::NotFound(format!("gRPC stream not found: {id}")));
        };
        if !session.client_streaming {
            return Err(AppError::Validation(
                "this method does not accept further client messages".into(),
            ));
        }
        let Some(outgoing) = session.outgoing.as_ref() else {
            return Err(AppError::Validation(
                "client stream is already half-closed".into(),
            ));
        };
        let message = message_from_json(session.input.clone(), payload)?;
        outgoing
            .send(message)
            .map_err(|_| AppError::Network("gRPC stream is closed".into()))
    }

    pub fn close_stream(&self, id: &str) -> AppResult<()> {
        let mut streams = self.streams.lock();
        let Some(session) = streams.get_mut(id) else {
            return Err(AppError::NotFound(format!("gRPC stream not found: {id}")));
        };
        session.outgoing.take();
        Ok(())
    }

    pub fn cancel_stream(&self, id: &str) -> bool {
        let Some(session) = self.streams.lock().remove(id) else {
            return false;
        };
        session.abort.abort();
        true
    }

    fn insert_pool(&self, pool: DescriptorPool) -> ProtoDefinition {
        let definition = describe(&pool);
        let id = crate::db::new_id();
        self.pools.lock().insert(id.clone(), Arc::new(pool));
        ProtoDefinition {
            pool_id: id,
            services: definition,
        }
    }

    fn method(&self, request: &GrpcInvoke) -> AppResult<MethodDescriptor> {
        let pools = self.pools.lock();
        let pool = pools.get(&request.pool_id).ok_or_else(|| {
            AppError::NotFound(
                "descriptor pool expired; reload the proto file or run reflection again".into(),
            )
        })?;
        find_method(pool, &request.service, &request.method)
    }
}

impl Drop for ProtoCatalog {
    fn drop(&mut self) {
        for session in self.streams.lock().values() {
            session.abort.abort();
        }
    }
}

async fn drive_stream(
    request: GrpcInvoke,
    method: MethodDescriptor,
    rx: mpsc::UnboundedReceiver<DynamicMessage>,
    stream_id: &str,
    sink: &StreamSink,
) -> AppResult<()> {
    let channel = connect_channel(&request.host, request.tls, None).await?;
    let mut client = tonic::client::Grpc::new(channel);
    client.ready().await.map_err(endpoint::transport_error)?;
    let path = method_path(&method)?;
    let codec = DynamicCodec {
        request: method.input(),
        response: method.output(),
    };
    let mut call = tonic::Request::new(MessageStream { rx });
    call.set_timeout(request.timeout);
    apply_metadata(&mut call, &request.metadata)?;
    let response = client
        .streaming(call, path, codec)
        .await
        .map_err(status_error)?;
    let mut inbound = response.into_inner();
    while let Some(item) = inbound.next().await {
        let message = item.map_err(status_error)?;
        sink(StreamEvent {
            stream_id: stream_id.to_string(),
            direction: "in".into(),
            data: Some(message_to_json(&message)?),
            done: false,
            error: None,
        });
    }
    Ok(())
}

struct MessageStream {
    rx: mpsc::UnboundedReceiver<DynamicMessage>,
}

impl futures_util::Stream for MessageStream {
    type Item = DynamicMessage;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().rx.poll_recv(cx)
    }
}

fn describe(pool: &DescriptorPool) -> Vec<ServiceInfo> {
    pool.services()
        .filter(|service| !service.full_name().starts_with("grpc.reflection."))
        .map(describe_service)
        .collect()
}

fn describe_service(service: ServiceDescriptor) -> ServiceInfo {
    ServiceInfo {
        name: service.full_name().to_string(),
        methods: service.methods().map(describe_method).collect(),
    }
}

fn describe_method(method: MethodDescriptor) -> MethodInfo {
    MethodInfo {
        name: method.name().to_string(),
        full_name: method.full_name().to_string(),
        client_streaming: method.is_client_streaming(),
        server_streaming: method.is_server_streaming(),
        request_type: method.input().full_name().to_string(),
        response_type: method.output().full_name().to_string(),
        request_template: request_template(&method.input()),
    }
}

fn request_template(descriptor: &MessageDescriptor) -> String {
    let message = DynamicMessage::new(descriptor.clone());
    let options = SerializeOptions::new().skip_default_fields(false);
    let mut buf = Vec::new();
    let mut serializer = serde_json::Serializer::new(&mut buf);
    if message
        .serialize_with_options(&mut serializer, &options)
        .is_err()
    {
        return "{}".into();
    }
    String::from_utf8(buf).unwrap_or_else(|_| "{}".into())
}

fn find_method(pool: &DescriptorPool, service: &str, method: &str) -> AppResult<MethodDescriptor> {
    let service_desc = pool
        .get_service_by_name(service)
        .ok_or_else(|| AppError::Proto(format!("service not found: {service}")))?;
    service_desc
        .methods()
        .find(|item| item.name() == method || item.full_name() == method)
        .ok_or_else(|| AppError::Proto(format!("method not found: {service}/{method}")))
}

fn method_path(method: &MethodDescriptor) -> AppResult<http::uri::PathAndQuery> {
    let service = method.parent_service().full_name();
    let path = format!("/{service}/{}", method.name());
    http::uri::PathAndQuery::from_maybe_shared(path)
        .map_err(|error| AppError::Proto(error.to_string()))
}

fn message_from_json(descriptor: MessageDescriptor, payload: &str) -> AppResult<DynamicMessage> {
    let text = if payload.trim().is_empty() {
        "{}"
    } else {
        payload
    };
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let options = DeserializeOptions::new().deny_unknown_fields(false);
    let message = DynamicMessage::deserialize_with_options(descriptor, &mut deserializer, &options)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| AppError::Validation(error.to_string()))?;
    Ok(message)
}

fn message_to_json(message: &DynamicMessage) -> AppResult<Value> {
    serde_json::to_value(message).map_err(|error| AppError::Proto(error.to_string()))
}

fn apply_metadata<T>(
    request: &mut tonic::Request<T>,
    metadata: &[(String, String)],
) -> AppResult<()> {
    for (key, value) in metadata {
        let key = tonic::metadata::MetadataKey::from_bytes(key.as_bytes())
            .map_err(|error| AppError::Validation(format!("invalid metadata {key}: {error}")))?;
        let value = tonic::metadata::MetadataValue::try_from(value.as_str())
            .map_err(|error| AppError::Validation(format!("invalid metadata value: {error}")))?;
        request.metadata_mut().insert(key, value);
    }
    Ok(())
}

fn status_error(status: tonic::Status) -> AppError {
    let text = format!("{}: {}", status.code(), status.message());
    match status.code() {
        tonic::Code::DeadlineExceeded => AppError::Timeout(text),
        tonic::Code::Cancelled => AppError::Cancelled,
        tonic::Code::InvalidArgument => AppError::Validation(text),
        _ => endpoint::transport_error(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_file_lists_methods_and_a_json_template() {
        let dir = std::env::temp_dir().join(format!("thread-proto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.proto");
        std::fs::write(
            &path,
            r#"syntax = "proto3";
package hello;
service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc SayHelloStream (HelloRequest) returns (stream HelloReply);
}
message HelloRequest { string name = 1; }
message HelloReply { string message = 1; }
"#,
        )
        .unwrap();

        let catalog = ProtoCatalog::new();
        let definition = catalog.load_file(&path).unwrap();
        assert_eq!(definition.services.len(), 1);
        assert_eq!(definition.services[0].name, "hello.Greeter");
        let methods = &definition.services[0].methods;
        assert!(
            methods
                .iter()
                .any(|method| method.name == "SayHello" && !method.server_streaming)
        );
        let stream = methods
            .iter()
            .find(|method| method.name == "SayHelloStream")
            .unwrap();
        assert!(stream.server_streaming);
        assert!(stream.request_template.contains("name"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
