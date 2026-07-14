use std::{
    fmt,
    sync::{Arc, RwLock},
};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rand::{rngs::OsRng, Rng};
use subtle::{Choice, ConstantTimeEq};
use tokio::sync::{OwnedRwLockWriteGuard, RwLock as AsyncRwLock};

pub const CLIENT_API_KEY_LENGTH: usize = 67;
const CLIENT_API_KEY_PREFIX: &str = "sk-";
const CLIENT_API_KEY_RANDOM_LENGTH: usize = 64;
const CLIENT_API_KEY_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ClientApiKey(Box<str>);

impl ClientApiKey {
    pub fn parse(value: &str) -> Result<Self, InvalidClientApiKey> {
        validate_api_key(value)
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

pub fn validate_api_key(value: &str) -> Result<ClientApiKey, InvalidClientApiKey> {
    let bytes = value.as_bytes();
    if bytes.len() != CLIENT_API_KEY_LENGTH
        || !value.starts_with(CLIENT_API_KEY_PREFIX)
        || !bytes[CLIENT_API_KEY_PREFIX.len()..]
            .iter()
            .all(u8::is_ascii_alphanumeric)
    {
        return Err(InvalidClientApiKey);
    }

    Ok(ClientApiKey(value.into()))
}

pub fn generate_api_key() -> ClientApiKey {
    let mut rng = OsRng;
    let mut value = String::with_capacity(CLIENT_API_KEY_LENGTH);
    value.push_str(CLIENT_API_KEY_PREFIX);
    for _ in 0..CLIENT_API_KEY_RANDOM_LENGTH {
        let index = rng.gen_range(0..CLIENT_API_KEY_ALPHABET.len());
        value.push(char::from(CLIENT_API_KEY_ALPHABET[index]));
    }
    ClientApiKey(value.into())
}

impl fmt::Debug for ClientApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientApiKey([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct InvalidClientApiKey;

impl fmt::Display for InvalidClientApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("client API key must use the required sk- format")
    }
}

impl std::error::Error for InvalidClientApiKey {}

#[derive(Clone)]
pub struct ClientAuthSnapshot {
    keys: Option<Arc<[ClientApiKey]>>,
}

impl ClientAuthSnapshot {
    pub fn disabled() -> Self {
        Self { keys: None }
    }

    pub fn enabled(
        candidates: impl IntoIterator<Item = ClientApiKey>,
    ) -> Result<Self, EmptyClientApiKeySet> {
        let mut keys = Vec::new();
        for key in candidates {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            return Err(EmptyClientApiKeySet);
        }

        Ok(Self {
            keys: Some(keys.into()),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.keys.is_some()
    }

    pub fn key_count(&self) -> usize {
        self.keys.as_deref().map_or(0, <[ClientApiKey]>::len)
    }

    fn authorizes(&self, candidate: &[u8]) -> bool {
        let Some(keys) = &self.keys else {
            return true;
        };

        let mut normalized = [0_u8; CLIENT_API_KEY_LENGTH];
        let copy_length = candidate.len().min(CLIENT_API_KEY_LENGTH);
        normalized[..copy_length].copy_from_slice(&candidate[..copy_length]);

        let valid_length = Choice::from((candidate.len() == CLIENT_API_KEY_LENGTH) as u8);
        let mut matched = Choice::from(0);
        for key in keys.iter() {
            matched |= key.expose_secret().as_bytes().ct_eq(&normalized);
        }

        bool::from(valid_length & matched)
    }
}

impl Default for ClientAuthSnapshot {
    fn default() -> Self {
        Self::disabled()
    }
}

impl fmt::Debug for ClientAuthSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientAuthSnapshot")
            .field("enabled", &self.is_enabled())
            .field("key_count", &self.key_count())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EmptyClientApiKeySet;

impl fmt::Display for EmptyClientApiKeySet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("client API authentication requires at least one key")
    }
}

impl std::error::Error for EmptyClientApiKeySet {}

#[derive(Clone)]
pub struct ClientAuthRuntime {
    current: Arc<RwLock<Arc<ClientAuthSnapshot>>>,
    policy_gate: Arc<AsyncRwLock<()>>,
}

impl ClientAuthRuntime {
    pub fn new(snapshot: ClientAuthSnapshot) -> Self {
        Self {
            current: Arc::new(RwLock::new(Arc::new(snapshot))),
            policy_gate: Arc::new(AsyncRwLock::new(())),
        }
    }

    pub async fn policy_snapshot(&self) -> Arc<ClientAuthSnapshot> {
        let _guard = self.policy_gate.read().await;
        self.snapshot()
    }

    pub async fn begin_update(&self) -> ClientAuthUpdateGuard {
        ClientAuthUpdateGuard {
            runtime: self.clone(),
            _policy_guard: self.policy_gate.clone().write_owned().await,
        }
    }

    pub fn snapshot(&self) -> Arc<ClientAuthSnapshot> {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub async fn replace(&self, snapshot: ClientAuthSnapshot) {
        self.begin_update().await.replace(snapshot);
    }

    fn replace_current(&self, snapshot: ClientAuthSnapshot) {
        *self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(snapshot);
    }

    pub fn is_enabled(&self) -> bool {
        self.snapshot().is_enabled()
    }
}

pub struct ClientAuthUpdateGuard {
    runtime: ClientAuthRuntime,
    _policy_guard: OwnedRwLockWriteGuard<()>,
}

impl ClientAuthUpdateGuard {
    pub fn replace(&self, snapshot: ClientAuthSnapshot) {
        self.runtime.replace_current(snapshot);
    }
}

impl Default for ClientAuthRuntime {
    fn default() -> Self {
        Self::new(ClientAuthSnapshot::disabled())
    }
}

impl fmt::Debug for ClientAuthRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientAuthRuntime")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

#[derive(Debug)]
enum ClientAuthFailure {
    Missing,
    Invalid,
}

impl IntoResponse for ClientAuthFailure {
    fn into_response(self) -> Response {
        let (message, code) = match self {
            Self::Missing => ("Missing bearer authentication in header", None),
            Self::Invalid => ("Incorrect API key provided.", Some("invalid_api_key")),
        };
        let mut response = (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "message": message,
                    "type": "invalid_request_error",
                    "param": null,
                    "code": code,
                }
            })),
        )
            .into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"OpenAI API\""),
        );
        response
    }
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<&[u8], ClientAuthFailure> {
    let mut authorization_values = headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = authorization_values.next() else {
        return Err(ClientAuthFailure::Missing);
    };
    if authorization_values.next().is_some() {
        return Err(ClientAuthFailure::Invalid);
    }

    let value = value.to_str().map_err(|_| ClientAuthFailure::Invalid)?;
    let separator = value.find(' ').ok_or(ClientAuthFailure::Invalid)?;
    let scheme = &value[..separator];
    let token = value[separator..].trim_start_matches(' ');
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(ClientAuthFailure::Invalid);
    }

    Ok(token.as_bytes())
}

pub(crate) async fn authenticate_openai_request(
    State(runtime): State<ClientAuthRuntime>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let policy = runtime.policy_snapshot().await;
    if !policy.is_enabled() {
        return next.run(request).await;
    }

    match bearer_token(request.headers()) {
        Ok(token) if policy.authorizes(token) => next.run(request).await,
        Ok(_) | Err(ClientAuthFailure::Invalid) => ClientAuthFailure::Invalid.into_response(),
        Err(ClientAuthFailure::Missing) => ClientAuthFailure::Missing.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use super::*;

    #[test]
    fn generated_api_keys_have_the_required_shape_and_are_unique() {
        let mut generated = HashSet::new();

        for _ in 0..256 {
            let key = generate_api_key();
            let value = key.expose_secret();
            assert_eq!(value.len(), CLIENT_API_KEY_LENGTH);
            assert!(value.starts_with(CLIENT_API_KEY_PREFIX));
            assert!(value[CLIENT_API_KEY_PREFIX.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric()));
            assert!(generated.insert(key));
        }
    }

    #[test]
    fn bearer_parser_allows_separator_spaces_but_rejects_token_whitespace() {
        let key = format!("sk-{}", "W".repeat(64));
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer    {key}")).expect("multiple separator spaces"),
        );
        let parsed = match bearer_token(&headers) {
            Ok(token) => token,
            Err(_) => panic!("valid bearer token"),
        };
        assert!(parsed == key.as_bytes());

        for value in [format!("Bearer {key} "), format!("Bearer {key} suffix")] {
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&value).expect("header with token whitespace"),
            );
            assert!(matches!(
                bearer_token(&headers),
                Err(ClientAuthFailure::Invalid)
            ));
        }
    }

    #[tokio::test]
    async fn policy_reads_wait_until_an_update_replaces_the_snapshot() {
        let runtime = ClientAuthRuntime::default();
        let update = runtime.begin_update().await;
        let reader_runtime = runtime.clone();
        let mut reader = tokio::spawn(async move { reader_runtime.policy_snapshot().await });

        assert!(tokio::time::timeout(Duration::from_millis(25), &mut reader)
            .await
            .is_err());

        let key = ClientApiKey::parse(&format!("sk-{}", "G".repeat(64))).expect("valid key");
        update.replace(ClientAuthSnapshot::enabled([key]).expect("enabled snapshot"));
        drop(update);

        let policy = tokio::time::timeout(Duration::from_secs(1), reader)
            .await
            .expect("policy read unblocked")
            .expect("policy reader task");
        assert!(policy.is_enabled());
        assert_eq!(policy.key_count(), 1);
    }
}
