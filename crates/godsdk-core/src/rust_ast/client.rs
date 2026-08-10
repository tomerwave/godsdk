use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn render_mod() -> TokenStream {
    quote! {
        mod auth;
        mod builder;
        mod error;
        mod retry;
        mod transport;

        pub use builder::{Client, ClientBuilder};
        pub use error::SdkError;
        pub use retry::RetryPolicy;

        pub(crate) use auth::{apply_auth, Auth};
        pub(crate) use retry::{
            is_idempotent, parse_retry_after, should_retry_status, sleep_before_retry,
        };
        pub(crate) use transport::encode_path_segment;
    }
}

pub(super) fn render_auth() -> TokenStream {
    quote! {
        #[derive(Clone, Debug)]
        pub(crate) enum Auth {
            None,
            Bearer(String),
            Header(String, String),
            Query(String, String),
            Basic(String, Option<String>),
        }

        pub(crate) fn apply_auth(
            request: reqwest::RequestBuilder,
            auth: &Auth,
        ) -> reqwest::RequestBuilder {
            match auth {
                Auth::None | Auth::Query(_, _) => request,
                Auth::Bearer(token) => request.bearer_auth(token),
                Auth::Header(name, value) => request.header(name, value),
                Auth::Basic(username, password) => request.basic_auth(username, password.as_deref()),
            }
        }
    }
}

pub(super) fn render_error() -> TokenStream {
    quote! {
        #[derive(Debug, thiserror::Error)]
        pub enum SdkError {
            #[error("invalid base URL: {0}")]
            InvalidBaseUrl(String),
            #[error("could not build HTTP client: {0}")]
            ClientBuild(String),
            #[error("HTTP transport failed: {0}")]
            Transport(String),
            #[error("HTTP request timed out")]
            Timeout,
            #[error("could not encode or decode JSON: {0}")]
            Serialization(String),
            #[error("HTTP {status} response: {body}")]
            Http { status: u16, body: String },
            #[error("response body exceeded the configured limit")]
            ResponseTooLarge,
        }
    }
}

pub(super) fn render_retry() -> TokenStream {
    let policy = retry_policy();
    let helpers = retry_helpers();
    quote! {
        #policy
        #helpers
    }
}

fn retry_policy() -> TokenStream {
    quote! {
        use std::time::Duration;

        #[derive(Clone, Debug)]
        pub struct RetryPolicy {
            pub max_retries: u32,
            pub initial_backoff: Duration,
            pub max_backoff: Duration,
            pub retry_statuses: Vec<u16>,
            pub retry_non_idempotent: bool,
        }

        impl Default for RetryPolicy {
            fn default() -> Self {
                Self {
                    max_retries: 0,
                    initial_backoff: Duration::from_millis(25),
                    max_backoff: Duration::from_secs(2),
                    retry_statuses: vec![408, 425, 429, 500, 502, 503, 504],
                    retry_non_idempotent: false,
                }
            }
        }

    }
}

fn retry_helpers() -> TokenStream {
    quote! {
        pub(crate) fn is_idempotent(method: &reqwest::Method) -> bool {
            matches!(
                *method,
                reqwest::Method::GET
                    | reqwest::Method::HEAD
                    | reqwest::Method::OPTIONS
                    | reqwest::Method::PUT
                    | reqwest::Method::DELETE
            )
        }

        pub(crate) fn should_retry_status(
            status: reqwest::StatusCode,
            policy: &RetryPolicy,
        ) -> bool {
            policy.retry_statuses.contains(&status.as_u16())
        }

        pub(crate) fn parse_retry_after(value: &str) -> Option<Duration> {
            value.parse::<u64>().ok().map(Duration::from_secs)
        }

        pub(crate) async fn sleep_before_retry(
            policy: &RetryPolicy,
            attempt: u32,
            retry_after: Option<Duration>,
        ) {
            let exponential = policy
                .initial_backoff
                .checked_mul(2_u32.saturating_pow(attempt))
                .unwrap_or(policy.max_backoff);
            tokio::time::sleep(retry_after.unwrap_or(exponential).min(policy.max_backoff)).await;
        }
    }
}
