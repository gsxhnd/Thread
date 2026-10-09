//! gRPC client against a local tonic server: unary and all three streaming modes.
//!
//! The server is hand-written (no `protoc`). The client loads a matching `.proto`
//! at runtime, which is the same path the app uses for user proto files.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use thread_lib::services::grpc::{GrpcInvoke, ProtoCatalog, StreamEvent};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::body::Body;
use tonic::server::NamedService;
use tonic::{Request, Response, Status, Streaming};
use tower::Service;

#[derive(Clone, PartialEq, prost::Message)]
struct HelloRequest {
    #[prost(string, tag = "1")]
    name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
struct HelloReply {
    #[prost(string, tag = "1")]
    message: String,
}

#[derive(Clone, Default)]
struct HelloCodec;

struct HelloEncoder;
struct HelloDecoder;

impl tonic::codec::Codec for HelloCodec {
    type Encode = HelloReply;
    type Decode = HelloRequest;
    type Encoder = HelloEncoder;
    type Decoder = HelloDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        HelloEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        HelloDecoder
    }
}

impl tonic::codec::Encoder for HelloEncoder {
    type Item = HelloReply;
    type Error = Status;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut tonic::codec::EncodeBuf<'_>,
    ) -> Result<(), Self::Error> {
        use prost::Message;
        item.encode(dst)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

impl tonic::codec::Decoder for HelloDecoder {
    type Item = HelloRequest;
    type Error = Status;

    fn decode(
        &mut self,
        src: &mut tonic::codec::DecodeBuf<'_>,
    ) -> Result<Option<Self::Item>, Self::Error> {
        use prost::Message;
        HelloRequest::decode(src)
            .map(Some)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

struct SayHello;
impl tonic::server::UnaryService<HelloRequest> for SayHello {
    type Response = HelloReply;
    type Future = Pin<Box<dyn Future<Output = Result<Response<HelloReply>, Status>> + Send>>;

    fn call(&mut self, request: Request<HelloRequest>) -> Self::Future {
        let name = request.into_inner().name;
        Box::pin(async move {
            Ok(Response::new(HelloReply {
                message: format!("Hello {name}"),
            }))
        })
    }
}

struct SayHelloStream;
impl tonic::server::ServerStreamingService<HelloRequest> for SayHelloStream {
    type Response = HelloReply;
    type ResponseStream =
        futures_util::stream::Iter<std::vec::IntoIter<Result<HelloReply, Status>>>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::ResponseStream>, Status>> + Send>>;

    fn call(&mut self, request: Request<HelloRequest>) -> Self::Future {
        let name = request.into_inner().name;
        Box::pin(async move {
            let messages = vec![
                Ok(HelloReply {
                    message: format!("Hello {name}"),
                }),
                Ok(HelloReply {
                    message: "again".into(),
                }),
            ];
            Ok(Response::new(futures_util::stream::iter(messages)))
        })
    }
}

struct SayHelloClient;
impl tonic::server::ClientStreamingService<HelloRequest> for SayHelloClient {
    type Response = HelloReply;
    type Future = Pin<Box<dyn Future<Output = Result<Response<HelloReply>, Status>> + Send>>;

    fn call(&mut self, request: Request<Streaming<HelloRequest>>) -> Self::Future {
        Box::pin(async move {
            let mut inbound = request.into_inner();
            let mut names = Vec::new();
            while let Some(item) = inbound.next().await {
                names.push(item?.name);
            }
            Ok(Response::new(HelloReply {
                message: names.join(","),
            }))
        })
    }
}

struct Chat;
impl tonic::server::StreamingService<HelloRequest> for Chat {
    type Response = HelloReply;
    type ResponseStream = futures_util::stream::BoxStream<'static, Result<HelloReply, Status>>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Self::ResponseStream>, Status>> + Send>>;

    fn call(&mut self, request: Request<Streaming<HelloRequest>>) -> Self::Future {
        Box::pin(async move {
            let mut inbound = request.into_inner();
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tokio::spawn(async move {
                while let Some(item) = inbound.next().await {
                    match item {
                        Ok(message) => {
                            if tx
                                .send(Ok(HelloReply {
                                    message: format!("echo {}", message.name),
                                }))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(status) => {
                            let _ = tx.send(Err(status)).await;
                            break;
                        }
                    }
                }
            });
            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            Ok(Response::new(Box::pin(stream)
                as futures_util::stream::BoxStream<
                    'static,
                    Result<HelloReply, Status>,
                >))
        })
    }
}

#[derive(Clone)]
struct Greeter;

impl Service<http::Request<Body>> for Greeter {
    type Response = http::Response<Body>;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let path = req.uri().path().to_string();
        Box::pin(async move {
            let response = match path.as_str() {
                "/hello.Greeter/SayHello" => {
                    tonic::server::Grpc::new(HelloCodec)
                        .unary(SayHello, req)
                        .await
                }
                "/hello.Greeter/SayHelloStream" => {
                    tonic::server::Grpc::new(HelloCodec)
                        .server_streaming(SayHelloStream, req)
                        .await
                }
                "/hello.Greeter/SayHelloClient" => {
                    tonic::server::Grpc::new(HelloCodec)
                        .client_streaming(SayHelloClient, req)
                        .await
                }
                "/hello.Greeter/Chat" => {
                    tonic::server::Grpc::new(HelloCodec)
                        .streaming(Chat, req)
                        .await
                }
                _ => {
                    return Ok(Status::not_found(path).into_http());
                }
            };
            Ok(response)
        })
    }
}

impl NamedService for Greeter {
    const NAME: &'static str = "hello.Greeter";
}

fn proto_file() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "thread-grpc-it-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
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
  rpc SayHelloClient (stream HelloRequest) returns (HelloReply);
  rpc Chat (stream HelloRequest) returns (stream HelloReply);
}
message HelloRequest { string name = 1; }
message HelloReply { string message = 1; }
"#,
    )
    .unwrap();
    path
}

fn invoke(host: &str, pool_id: &str, method: &str, payload: &str) -> GrpcInvoke {
    GrpcInvoke {
        host: host.to_string(),
        tls: false,
        metadata: Vec::new(),
        pool_id: pool_id.to_string(),
        service: "hello.Greeter".into(),
        method: method.into(),
        payload: payload.into(),
        timeout: Duration::from_secs(5),
    }
}

async fn spawn_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(Greeter)
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    format!("127.0.0.1:{}", address.port())
}

async fn collect_until_done(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<StreamEvent>,
) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("stream timed out")
    {
        let done = event.done;
        events.push(event);
        if done {
            break;
        }
    }
    events
}

#[tokio::test]
async fn unary_and_streaming_round_trip() {
    let host = spawn_server().await;
    let path = proto_file();
    let catalog = Arc::new(ProtoCatalog::new());
    let definition = catalog.load_file(&path).unwrap();
    let pool_id = definition.pool_id.clone();

    let unary = catalog
        .unary(&invoke(&host, &pool_id, "SayHello", r#"{"name":"Ada"}"#))
        .await
        .unwrap();
    assert_eq!(unary.response["message"], "Hello Ada");

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sink = Arc::new(move |event: StreamEvent| {
        let _ = tx.send(event);
    });
    catalog
        .open_stream(
            invoke(&host, &pool_id, "SayHelloStream", r#"{"name":"Lin"}"#),
            sink,
        )
        .unwrap();
    let events = collect_until_done(rx).await;
    let incoming: Vec<_> = events
        .iter()
        .filter_map(|event| event.data.as_ref())
        .filter(|_| true)
        .collect();
    let texts: Vec<String> = events
        .iter()
        .filter(|event| event.direction == "in")
        .filter_map(|event| {
            event
                .data
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .collect();
    assert_eq!(texts, vec!["Hello Lin".to_string(), "again".to_string()]);
    assert!(events.iter().any(|event| event.done));
    assert!(events.iter().all(|event| event.error.is_none()));
    let _ = incoming;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sink = Arc::new(move |event: StreamEvent| {
        let _ = tx.send(event);
    });
    let client_id = catalog
        .open_stream(
            invoke(&host, &pool_id, "SayHelloClient", r#"{"name":"A"}"#),
            sink,
        )
        .unwrap();
    catalog.send_stream(&client_id, r#"{"name":"B"}"#).unwrap();
    catalog.close_stream(&client_id).unwrap();
    let events = collect_until_done(rx).await;
    let reply = events.iter().find_map(|event| {
        event
            .data
            .as_ref()
            .filter(|_| event.direction == "in")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
    });
    assert_eq!(reply.as_deref(), Some("A,B"));

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sink = Arc::new(move |event: StreamEvent| {
        let _ = tx.send(event);
    });
    let chat_id = catalog
        .open_stream(invoke(&host, &pool_id, "Chat", ""), sink)
        .unwrap();
    catalog.send_stream(&chat_id, r#"{"name":"hi"}"#).unwrap();
    let echo = tokio::time::timeout(Duration::from_secs(5), async {
        let mut inbound = rx;
        loop {
            let event = inbound.recv().await.expect("chat closed");
            if event.direction == "in" {
                if let Some(text) = event
                    .data
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(|value| value.as_str())
                {
                    return text.to_string();
                }
            }
        }
    })
    .await
    .expect("bidi echo timed out");
    assert_eq!(echo, "echo hi");
    assert!(catalog.cancel_stream(&chat_id));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

use std::future::Future;
