use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn render_mod() -> TokenStream {
    quote! {
        mod auth;
        mod builder;
        mod error;
        mod parameter_serialization;
        mod retry;
        mod transport;

        pub use builder::{Client, ClientBuilder};
        pub use error::SdkError;
        pub use retry::RetryPolicy;

        pub(crate) use auth::{apply_auth, Auth, AuthEntry, AuthRequirement};
        pub(crate) use retry::{
            is_idempotent, parse_retry_after, should_retry_status, sleep_before_retry,
        };
        #[allow(unused_imports)]
        pub(crate) use parameter_serialization::{
            serialize_cookie_value, serialize_parameter_value, serialize_path_parameter_value,
        };
        pub(crate) use transport::{HttpResponse, RequestOptions};
    }
}

pub(super) fn render_auth() -> TokenStream {
    let types = auth_types();
    let matching = auth_matching();
    let application = auth_application();
    quote! {
        use super::SdkError;
        #types
        #matching
        #application
    }
}

fn auth_types() -> TokenStream {
    quote! {
        #[derive(Clone, Debug, Default)]
        pub(crate) struct Auth {
            entries: Vec<AuthEntry>,
        }

        #[derive(Clone, Debug)]
        pub(crate) enum AuthEntry {
            Bearer { scheme: Option<String>, token: String },
            Http { scheme: String, value: String },
            ApiKeyHeader { scheme: Option<String>, name: String, value: String },
            ApiKeyQuery { scheme: Option<String>, name: String, value: String },
            ApiKeyCookie { scheme: Option<String>, name: String, value: String },
            Basic { scheme: Option<String>, username: String, password: Option<String> },
        }

        #[allow(dead_code)]
        #[derive(Clone, Copy, Debug)]
        pub(crate) enum AuthRequirement {
            Bearer { scheme: &'static str },
            Http { scheme: &'static str },
            ApiKeyHeader { scheme: &'static str, name: &'static str },
            ApiKeyQuery { scheme: &'static str, name: &'static str },
            ApiKeyCookie { scheme: &'static str, name: &'static str },
            Basic { scheme: &'static str },
        }

        impl Auth {
            pub(crate) fn add(&mut self, entry: AuthEntry) {
                self.entries.push(entry);
            }
        }
    }
}

fn auth_matching() -> TokenStream {
    quote! {
        impl Auth {
            fn entry_for(&self, requirement: &AuthRequirement) -> Option<&AuthEntry> {
                self.entries
                    .iter()
                    .find(|entry| matches_requirement(entry, requirement))
            }
        }

        fn matches_requirement(entry: &AuthEntry, requirement: &AuthRequirement) -> bool {
            match (entry, requirement) {
                (AuthEntry::Bearer { scheme, .. }, AuthRequirement::Bearer { scheme: required }) =>
                    scheme_matches(scheme, required),
                (AuthEntry::Http { scheme, .. }, AuthRequirement::Http { scheme: required }) =>
                    scheme == required,
                (
                    AuthEntry::ApiKeyHeader { scheme, name, .. },
                    AuthRequirement::ApiKeyHeader { scheme: required, name: required_name },
                ) => scheme_matches(scheme, required) && name == required_name,
                (
                    AuthEntry::ApiKeyQuery { scheme, name, .. },
                    AuthRequirement::ApiKeyQuery { scheme: required, name: required_name },
                ) => scheme_matches(scheme, required) && name == required_name,
                (
                    AuthEntry::ApiKeyCookie { scheme, name, .. },
                    AuthRequirement::ApiKeyCookie { scheme: required, name: required_name },
                ) => scheme_matches(scheme, required) && name == required_name,
                (AuthEntry::Basic { scheme, .. }, AuthRequirement::Basic { scheme: required }) =>
                    scheme_matches(scheme, required),
                _ => false,
            }
        }

        fn scheme_matches(configured: &Option<String>, required: &str) -> bool {
            configured.as_deref().is_none_or(|scheme| scheme == required)
        }
    }
}

fn auth_application() -> TokenStream {
    let apply = auth_apply_function();
    let helpers = auth_apply_helpers();
    quote! {
        #apply
        #helpers
    }
}

fn auth_apply_function() -> TokenStream {
    quote! {
        pub(crate) fn apply_auth(
            request: reqwest::RequestBuilder,
            auth: &Auth,
            requirements: Option<&[Vec<AuthRequirement>]>,
        ) -> Result<reqwest::RequestBuilder, SdkError> {
            match requirements {
                None => Ok(apply_all(request, auth)),
                Some([]) => Ok(request),
                Some(alternatives) => apply_selected(request, auth, alternatives),
            }
        }
    }
}

fn auth_apply_helpers() -> TokenStream {
    let selection = auth_selection_helpers();
    let entry = auth_entry_helper();
    quote! {
        #selection
        #entry
    }
}

fn auth_selection_helpers() -> TokenStream {
    quote! {
        fn apply_all(
            request: reqwest::RequestBuilder,
            auth: &Auth,
        ) -> reqwest::RequestBuilder {
            auth.entries.iter().fold(request, apply_entry)
        }

        fn apply_selected(
            request: reqwest::RequestBuilder,
            auth: &Auth,
            alternatives: &[Vec<AuthRequirement>],
        ) -> Result<reqwest::RequestBuilder, SdkError> {
            let selected = alternatives.iter().find(|alternative| {
                alternative
                    .iter()
                    .all(|requirement| auth.entry_for(requirement).is_some())
            });
            let Some(selected) = selected else {
                return Err(SdkError::MissingAuthentication);
            };
            let entries = selected
                .iter()
                .filter_map(|requirement| auth.entry_for(requirement))
                .collect::<Vec<_>>();
            if entries.len() != selected.len() {
                return Err(SdkError::MissingAuthentication);
            }
            Ok(entries.into_iter().fold(request, apply_entry))
        }
    }
}

fn auth_entry_helper() -> TokenStream {
    quote! {
        fn auth_header_value(scheme: &str, value: &str) -> String {
            let mut output = String::with_capacity(scheme.len() + value.len() + 1);
            output.push_str(scheme);
            output.push(' ');
            output.push_str(value);
            output
        }

        fn cookie_header_value(name: &str, value: &str) -> String {
            let mut output = String::with_capacity(name.len() + value.len() + 1);
            output.push_str(name);
            output.push('=');
            output.push_str(value);
            output
        }

        fn apply_entry(
            request: reqwest::RequestBuilder,
            entry: &AuthEntry,
        ) -> reqwest::RequestBuilder {
            match entry {
                AuthEntry::Bearer { token, .. } => request.bearer_auth(token),
                AuthEntry::Http { scheme, value } => request.header(
                    reqwest::header::AUTHORIZATION,
                    auth_header_value(scheme, value),
                ),
                AuthEntry::ApiKeyHeader { name, value, .. } => request.header(name, value),
                AuthEntry::ApiKeyQuery { name, value, .. } => request.query(&[(name, value)]),
                AuthEntry::ApiKeyCookie { name, value, .. } => request.header(
                    reqwest::header::COOKIE,
                    cookie_header_value(name, value),
                ),
                AuthEntry::Basic { username, password, .. } => {
                    request.basic_auth(username, password.as_deref())
                }
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
            #[error("no configured credentials satisfy the operation's security requirements")]
            MissingAuthentication,
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
