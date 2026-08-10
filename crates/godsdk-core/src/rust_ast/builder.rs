use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn render() -> TokenStream {
    let sections = [
        client_type(),
        client_debug(),
        builder_type(),
        client_impl(),
        builder_impl(),
    ];
    quote! {
        use std::time::Duration;
        use super::{Auth, AuthEntry, RetryPolicy, SdkError};
        #(#sections)*
    }
}

fn client_type() -> TokenStream {
    quote! {
        #[derive(Clone)]
        pub struct Client {
            pub(crate) base_url: url::Url,
            pub(crate) http: reqwest::Client,
            pub(crate) auth: Auth,
            pub(crate) max_error_body_bytes: usize,
            pub(crate) retry_policy: RetryPolicy,
        }
    }
}

fn client_debug() -> TokenStream {
    quote! {
        impl std::fmt::Debug for Client {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct("Client")
                    .field("base_url", &self.base_url)
                    .finish_non_exhaustive()
            }
        }
    }
}

fn builder_type() -> TokenStream {
    quote! {
        #[derive(Clone, Debug)]
        pub struct ClientBuilder {
            base_url: Option<url::Url>,
            auth: Auth,
            timeout: Duration,
            connect_timeout: Option<Duration>,
            max_error_body_bytes: usize,
            retry_policy: RetryPolicy,
        }
    }
}

fn client_impl() -> TokenStream {
    quote! {
        impl Client {
            pub fn new(base_url: impl AsRef<str>) -> Result<Self, SdkError> {
                Self::builder(base_url).build()
            }

            pub fn builder(base_url: impl AsRef<str>) -> ClientBuilder {
                ClientBuilder {
                    base_url: url::Url::parse(base_url.as_ref()).ok(),
            auth: Auth::default(),
                    timeout: Duration::from_secs(30),
                    connect_timeout: Some(Duration::from_secs(10)),
                    max_error_body_bytes: 64 * 1024,
                    retry_policy: RetryPolicy::default(),
                }
            }
        }
    }
}

fn builder_impl() -> TokenStream {
    let sections = [auth_methods(), config_methods(), build_method()];
    quote! {
        impl ClientBuilder {
            #(#sections)*
        }
    }
}

fn auth_methods() -> TokenStream {
    quote! {
            pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::Bearer { scheme: None, token: token.into() });
                self
            }

            pub fn api_key_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::ApiKeyHeader { scheme: None, name: name.into(), value: value.into() });
                self
            }

            pub fn api_key_query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::ApiKeyQuery { scheme: None, name: name.into(), value: value.into() });
                self
            }

            pub fn basic_auth(mut self, username: impl Into<String>, password: Option<String>) -> Self {
                self.auth.add(AuthEntry::Basic { scheme: None, username: username.into(), password });
                self
            }

            pub fn bearer_token_for(mut self, scheme: impl Into<String>, token: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::Bearer { scheme: Some(scheme.into()), token: token.into() });
                self
            }

            pub fn http_auth_for(mut self, scheme: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::Http { scheme: scheme.into(), value: value.into() });
                self
            }

            pub fn api_key_header_for(mut self, scheme: impl Into<String>, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::ApiKeyHeader { scheme: Some(scheme.into()), name: name.into(), value: value.into() });
                self
            }

            pub fn api_key_query_for(mut self, scheme: impl Into<String>, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::ApiKeyQuery { scheme: Some(scheme.into()), name: name.into(), value: value.into() });
                self
            }

            pub fn api_key_cookie_for(mut self, scheme: impl Into<String>, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.auth.add(AuthEntry::ApiKeyCookie { scheme: Some(scheme.into()), name: name.into(), value: value.into() });
                self
            }

            pub fn basic_auth_for(mut self, scheme: impl Into<String>, username: impl Into<String>, password: Option<String>) -> Self {
                self.auth.add(AuthEntry::Basic { scheme: Some(scheme.into()), username: username.into(), password });
                self
            }
    }
}

fn config_methods() -> TokenStream {
    quote! {
            pub fn timeout(mut self, timeout: Duration) -> Self {
                self.timeout = timeout;
                self
            }

            pub fn connect_timeout(mut self, timeout: Duration) -> Self {
                self.connect_timeout = Some(timeout);
                self
            }

            pub fn max_error_body_bytes(mut self, limit: usize) -> Self {
                self.max_error_body_bytes = limit;
                self
            }

            pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
                self.retry_policy = policy;
                self
            }
    }
}

fn build_method() -> TokenStream {
    quote! {
            pub fn build(self) -> Result<Client, SdkError> {
                let base_url = self
                    .base_url
                    .ok_or_else(|| SdkError::InvalidBaseUrl("URL could not be parsed".to_string()))?;
                if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
                    return Err(SdkError::InvalidBaseUrl(base_url.to_string()));
                }
                let mut builder = reqwest::Client::builder().timeout(self.timeout);
                if let Some(timeout) = self.connect_timeout {
                    builder = builder.connect_timeout(timeout);
                }
                let http = builder
                    .build()
                    .map_err(|error| SdkError::ClientBuild(error.to_string()))?;
                Ok(Client {
                    base_url,
                    http,
                    auth: self.auth,
                    max_error_body_bytes: self.max_error_body_bytes,
                    retry_policy: self.retry_policy,
                })
            }
    }
}
