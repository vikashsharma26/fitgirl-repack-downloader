use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, HOST, USER_AGENT};
use reqwest::{Client, Method, RequestBuilder};

use crate::error::Result;

/// HTTP client that sends requests the way browsers and curl do.
///
/// Some Cloudflare-protected hosts (fuckingfast.co among them) challenge
/// clients whose HTTP/1.1 requests put `host` last in lowercase, which is
/// hyper's default. This client always leads with `Host`, `User-Agent` and
/// `Accept`, in title case, and uses HTTP/1.1 so every download segment gets
/// its own TCP connection.
#[derive(Clone, Debug)]
pub struct HttpClient {
    client: Client,
    user_agent: HeaderValue,
}

impl HttpClient {
    pub fn new(user_agent: &str) -> Result<Self> {
        let client = Client::builder()
            .http1_only()
            .http1_title_case_headers()
            .tcp_nodelay(true)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(64)
            .build()?;
        Ok(Self {
            client,
            user_agent: HeaderValue::from_str(user_agent)
                .unwrap_or_else(|_| HeaderValue::from_static("fitdl")),
        })
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        let mut headers = HeaderMap::with_capacity(4);
        if let Some(host) = host_header(url) {
            headers.insert(HOST, host);
        }
        headers.insert(USER_AGENT, self.user_agent.clone());
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        self.client.request(method, url).headers(headers)
    }
}

fn host_header(url: &str) -> Option<HeaderValue> {
    let u = reqwest::Url::parse(url).ok()?;
    let host = u.host_str()?;
    let value = match u.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    HeaderValue::from_str(&value).ok()
}
