//! Shared gRPC channel setup (plaintext and TLS via native roots).

use std::time::Duration;

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

use crate::error::{AppError, AppResult};

pub async fn connect_channel(
    host: &str,
    tls: bool,
    timeout: Option<Duration>,
) -> AppResult<Channel> {
    endpoint(host, tls, timeout)?
        .connect()
        .await
        .map_err(transport_error)
}

pub fn endpoint(host: &str, tls: bool, timeout: Option<Duration>) -> AppResult<Endpoint> {
    let uri = normalize_uri(host, tls)?;
    let use_tls = tls || uri.starts_with("https://");
    let mut endpoint = Endpoint::from_shared(uri.clone())
        .map_err(|error| AppError::Validation(format!("invalid gRPC host: {error}")))?;
    endpoint = endpoint.connect_timeout(Duration::from_secs(10));
    if let Some(timeout) = timeout {
        endpoint = endpoint.timeout(timeout);
    }
    if use_tls {
        let tls_config = ClientTlsConfig::new().with_native_roots();
        endpoint = endpoint
            .tls_config(tls_config)
            .map_err(|error| AppError::Tls(error.to_string()))?;
    }
    Ok(endpoint)
}

fn normalize_uri(host: &str, tls: bool) -> AppResult<String> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("gRPC host is required".into()));
    }
    if let Some(rest) = trimmed.strip_prefix("http://") {
        if tls {
            return Ok(format!("https://{rest}"));
        }
        return Ok(trimmed.to_string());
    }
    if trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    let scheme = if tls { "https" } else { "http" };
    Ok(format!("{scheme}://{trimmed}"))
}

pub fn transport_error(error: impl std::fmt::Display) -> AppError {
    let text = error.to_string();
    let lower = text.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline") {
        AppError::Timeout(text)
    } else if lower.contains("certificate") || lower.contains("tls") || lower.contains("ssl") {
        AppError::Tls(text)
    } else {
        AppError::Network(text)
    }
}
