//! HTTP/1 client built on hyper and rustls.
//!
//! Callers share two connectors: one verifies server certificates, the other
//! accepts any certificate when a request turns verification off. Redirects
//! and the overall timeout live here so the rest of the app does not depend
//! on a higher-level HTTP client.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use http::header::{
    AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION, PROXY_AUTHORIZATION, USER_AGENT,
};
use http::{HeaderMap, Method, Request, StatusCode, Uri};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Either, Empty, Full};
use hyper::body::Incoming;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::error::{AppError, AppResult};

const THREAD_AGENT: http::HeaderValue =
    http::HeaderValue::from_static(concat!("Thread/", env!("CARGO_PKG_VERSION")));

type ExchangeBody = Either<Full<Bytes>, Empty<Bytes>>;
type Connector = HttpsConnector<HttpConnector>;
type HttpClient = Client<Connector, ExchangeBody>;

/// Error type shared by every body the capture proxy forwards in either direction.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
/// Body type used by the capture proxy for both requests and responses.
pub type ProxyBody = UnsyncBoxBody<Bytes, BoxError>;
pub type ProxyClient = Client<Connector, ProxyBody>;

pub struct Exchange {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub timeout: Duration,
    pub verify_ssl: bool,
    /// `0` returns the first response, including a redirect.
    pub max_redirects: usize,
}

pub struct Fetched {
    pub status: StatusCode,
    pub final_url: String,
    pub headers: HeaderMap,
    pub redirect_count: usize,
    pub body: http_body_util::BodyDataStream<Incoming>,
}

pub async fn exchange(request: Exchange) -> AppResult<Fetched> {
    let timeout = request.timeout.max(Duration::from_millis(1));
    match tokio::time::timeout(timeout, follow(request)).await {
        Ok(result) => result,
        Err(_) => Err(AppError::Timeout("request timed out".into())),
    }
}

async fn follow(request: Exchange) -> AppResult<Fetched> {
    let client = shared_client(request.verify_ssl)?;
    let mut method = request.method;
    let mut url = request.url;
    let mut headers = request.headers;
    let mut body = request.body;
    let mut redirect_count = 0usize;

    if !headers.contains_key(USER_AGENT) {
        headers.insert(USER_AGENT, THREAD_AGENT.clone());
    }

    loop {
        let response = send_once(client, &method, &url, &headers, body.clone()).await?;
        let status = response.status();
        if redirect_count >= request.max_redirects || !is_redirect(status) {
            let (parts, incoming) = response.into_parts();
            return Ok(Fetched {
                status: parts.status,
                final_url: url,
                headers: parts.headers,
                redirect_count,
                body: incoming.into_data_stream(),
            });
        }

        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Network("redirect is missing a Location header".into()))?
            .to_string();
        let next = resolve_location(&url, &location)?;
        if !same_origin(&url, &next) {
            headers.remove(AUTHORIZATION);
            headers.remove(COOKIE);
            headers.remove(PROXY_AUTHORIZATION);
        }
        if redirect_becomes_get(status) {
            method = Method::GET;
            body = None;
            headers.remove(CONTENT_TYPE);
            headers.remove(CONTENT_LENGTH);
        }
        BodyExt::collect(response.into_body())
            .await
            .map_err(|error| AppError::from_transport(&error))?;
        url = next;
        redirect_count += 1;
    }
}

async fn send_once(
    client: &HttpClient,
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> AppResult<http::Response<Incoming>> {
    let uri: Uri = url
        .parse()
        .map_err(|error| AppError::Validation(format!("invalid url: {error}")))?;
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    let payload = match body {
        Some(bytes) => Either::Left(Full::new(bytes)),
        None => Either::Right(Empty::new()),
    };
    let request = builder
        .body(payload)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    client
        .request(request)
        .await
        .map_err(|error| AppError::from_transport(&error))
}

fn shared_client(verify_ssl: bool) -> AppResult<&'static HttpClient> {
    if verify_ssl {
        static SECURE: OnceLock<HttpClient> = OnceLock::new();
        if let Some(client) = SECURE.get() {
            return Ok(client);
        }
        let client = build_client(true)?;
        return Ok(SECURE.get_or_init(|| client));
    }
    static INSECURE: OnceLock<HttpClient> = OnceLock::new();
    if let Some(client) = INSECURE.get() {
        return Ok(client);
    }
    let client = build_client(false)?;
    Ok(INSECURE.get_or_init(|| client))
}

fn build_client(verify_ssl: bool) -> AppResult<HttpClient> {
    Ok(Client::builder(TokioExecutor::new()).build(build_connector(verify_ssl)?))
}

/// HTTP/HTTPS connector shared by the request client and the capture proxy.
fn build_connector(verify_ssl: bool) -> AppResult<Connector> {
    let _provider = crypto_provider();
    let connector = if verify_ssl {
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| AppError::Tls(error.to_string()))?
            .https_or_http()
            .enable_http1()
            .build()
    } else {
        let provider = crypto_provider();
        let verifier = Arc::new(AcceptAnyCert { provider });
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(config)
            .https_or_http()
            .enable_http1()
            .build()
    };
    Ok(connector)
}

/// Client used by the capture proxy to reach real servers. It always verifies upstream
/// certificates: the proxy's own certificate is only presented to the local client.
pub fn proxy_client() -> AppResult<&'static ProxyClient> {
    static PROXY: OnceLock<ProxyClient> = OnceLock::new();
    if let Some(client) = PROXY.get() {
        return Ok(client);
    }
    let client = Client::builder(TokioExecutor::new()).build(build_connector(true)?);
    Ok(PROXY.get_or_init(|| client))
}

fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    if let Some(installed) = rustls::crypto::CryptoProvider::get_default() {
        return Arc::clone(installed);
    }
    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) => rustls::crypto::CryptoProvider::get_default()
            .cloned()
            .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider())),
        Err(existing) => existing,
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn redirect_becomes_get(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
    )
}

fn resolve_location(current: &str, location: &str) -> AppResult<String> {
    let base = url::Url::parse(current)
        .map_err(|error| AppError::Network(format!("invalid redirect base: {error}")))?;
    let next = base
        .join(location)
        .map_err(|error| AppError::Network(format!("invalid redirect location: {error}")))?;
    Ok(next.to_string())
}

fn same_origin(left: &str, right: &str) -> bool {
    let Ok(left) = url::Url::parse(left) else {
        return false;
    };
    let Ok(right) = url::Url::parse(right) else {
        return false;
    };
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[derive(Debug)]
struct AcceptAnyCert {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
