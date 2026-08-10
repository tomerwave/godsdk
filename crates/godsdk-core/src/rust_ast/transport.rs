use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn render() -> TokenStream {
    let sections = [
        imports(),
        client_request(),
        body_helpers(),
        path_encoding(),
        super::parameter_serialization::render(),
    ];
    quote! { #(#sections)* }
}

fn imports() -> TokenStream {
    quote! {
        use std::time::Duration;
        use reqwest::{header, Method};
        use url::Url;
        use super::{
            apply_auth, is_idempotent, parse_retry_after, should_retry_status, sleep_before_retry,
            AuthRequirement, Client, SdkError,
        };

        pub(crate) struct HttpResponse {
            pub(crate) status: u16,
            pub(crate) body: String,
        }

        enum AttemptOutcome {
            Response(HttpResponse),
            Retry(Option<Duration>),
            Failure(SdkError),
        }

        impl AttemptOutcome {
            async fn from_body(response: reqwest::Response, limit: usize) -> Self {
                let status = response.status().as_u16();
                match read_body(response, limit).await {
                    Ok(body) => Self::Response(HttpResponse { status, body }),
                    Err(error) => Self::Failure(error),
                }
            }

            fn retry_or_failure(may_retry: bool, retry_after: Option<Duration>, error: SdkError) -> Self {
                if may_retry { Self::Retry(retry_after) } else { Self::Failure(error) }
            }
        }
    }
}

fn client_request() -> TokenStream {
    let sections = [
        request_method(),
        build_request(),
        send_once(),
        handle_http_failure(),
    ];
    quote! {
        #(#sections)*
    }
}

fn request_method() -> TokenStream {
    quote! {
        pub(crate) struct RequestOptions {
            pub(crate) query: Vec<(String, String)>,
            pub(crate) headers: Vec<(String, String)>,
            pub(crate) body: Option<String>,
            pub(crate) requirements: Option<&'static [&'static [AuthRequirement]]>,
        }

        impl Client {
            pub(crate) async fn request(
                &self,
                method: Method,
                path: &str,
                options: RequestOptions,
            ) -> Result<HttpResponse, SdkError> {
                let url = self
                    .base_url
                    .join(path)
                    .map_err(|error| SdkError::InvalidBaseUrl(error.to_string()))?;
                let can_retry = is_idempotent(&method) || self.retry_policy.retry_non_idempotent;
                for attempt in 0..=self.retry_policy.max_retries {
                    let may_retry = can_retry && attempt < self.retry_policy.max_retries;
                    match self
                        .send_once(&method, &url, &options, may_retry)
                        .await
                    {
                        AttemptOutcome::Response(response) => return Ok(response),
                        AttemptOutcome::Retry(retry_after) => {
                            sleep_before_retry(&self.retry_policy, attempt, retry_after).await;
                        }
                        AttemptOutcome::Failure(error) => return Err(error),
                    }
                }
                Err(SdkError::Transport("retry loop exhausted".to_string()))
            }

        }
    }
}

fn build_request() -> TokenStream {
    quote! {
        impl Client {
            fn build_request(
                &self,
                method: &Method,
                url: &Url,
                options: &RequestOptions,
            ) -> Result<reqwest::RequestBuilder, SdkError> {
                let request = apply_auth(
                    self.http.request(method.clone(), url.clone()),
                    &self.auth,
                    options.requirements,
                )?;
                let request = options.query.iter().fold(request, |request, (name, value)| {
                    request.query(&[(name, value)])
                });
                let request = options.headers.iter().fold(request, |request, (name, value)| {
                    request.header(name.clone(), value)
                });
                Ok(match options.body.as_deref() {
                    Some(body) => request.body(body.to_string()),
                    None => request,
                })
            }
        }
    }
}

fn send_once() -> TokenStream {
    quote! {
        impl Client {
            async fn send_once(
                &self,
                method: &Method,
                url: &Url,
                options: &RequestOptions,
                may_retry: bool,
            ) -> AttemptOutcome {
                let request = match self.build_request(method, url, options) {
                    Ok(request) => request,
                    Err(error) => return AttemptOutcome::Failure(error),
                };
                match request.send().await {
                    Ok(response) if response.status().is_success() => {
                        AttemptOutcome::from_body(response, self.max_error_body_bytes).await
                    }
                    Ok(response) => self.handle_http_failure(response, may_retry).await,
                    Err(error) if error.is_timeout() => {
                        AttemptOutcome::retry_or_failure(may_retry, None, SdkError::Timeout)
                    }
                    Err(error) => AttemptOutcome::retry_or_failure(
                        may_retry,
                        None,
                        SdkError::Transport(error.to_string()),
                    ),
                }
            }
        }
    }
}

fn handle_http_failure() -> TokenStream {
    quote! {
        impl Client {
            async fn handle_http_failure(
                &self,
                response: reqwest::Response,
                may_retry: bool,
            ) -> AttemptOutcome {
                let status = response.status();
                let retry_after = response
                    .headers()
                    .get(header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_retry_after);
                if may_retry && should_retry_status(status, &self.retry_policy) {
                    return AttemptOutcome::Retry(retry_after);
                }
                match read_error_body(response, self.max_error_body_bytes).await {
                    Ok(body) => AttemptOutcome::Response(HttpResponse {
                        status: status.as_u16(),
                        body,
                    }),
                    Err(error) => AttemptOutcome::Failure(error),
                }
            }
        }
    }
}

fn body_helpers() -> TokenStream {
    quote! {
        async fn read_body(response: reqwest::Response, limit: usize) -> Result<String, SdkError> {
            let body = response.bytes().await.map_err(|error| {
                if error.is_timeout() {
                    SdkError::Timeout
                } else {
                    SdkError::Transport(error.to_string())
                }
            })?;
            if body.len() > limit {
                return Err(SdkError::ResponseTooLarge);
            }
            String::from_utf8(body.to_vec()).map_err(|error| SdkError::Serialization(error.to_string()))
        }

        async fn read_error_body(response: reqwest::Response, limit: usize) -> Result<String, SdkError> {
            match read_body(response, limit).await {
                Ok(body) => Ok(body),
                Err(SdkError::ResponseTooLarge) => Ok("<response body omitted: limit exceeded>".to_string()),
                Err(error) => Err(error),
            }
        }
    }
}

fn path_encoding() -> TokenStream {
    quote! {
        pub(crate) fn encode_path_segment(value: &str) -> String {
            percent_encoding::percent_encode(value.as_bytes(), percent_encoding::NON_ALPHANUMERIC)
                .to_string()
                .replace("%2D", "-")
                .replace("%5F", "_")
                .replace("%2E", ".")
                .replace("%7E", "~")
        }
    }
}
