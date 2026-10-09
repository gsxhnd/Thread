//! Service layer: protocol clients, history, variables, secrets.

pub mod graphql;
pub mod grpc;
pub mod history;
pub mod http;
pub mod secrets;
mod transport;
pub mod variables;
pub mod websocket;
