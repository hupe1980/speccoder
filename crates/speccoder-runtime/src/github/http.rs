//! GitHub's documented REST and GraphQL APIs, over rustls, within a time
//! limit, every failure said in words with what to do. Blocking for the
//! caller (the runtime's commands are); one small runtime per call.

use serde_json::Value;

use super::auth::{SCOPES, Secret};

/// How long one call may take.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("speccoder/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(LIMIT)
        .build()
        .map_err(|e| e.to_string())
}

fn block<T>(f: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    crate::pipeline::tokio_runtime()?.block_on(f)
}

/// Where a host's APIs answer: `api.github.com` for github.com, `/api/v3`
/// on an Enterprise server; `base` replaces both (tests, a proxy).
#[must_use]
pub fn endpoints(host: &str, base: Option<&str>) -> (String, String) {
    if let Some(base) = base.filter(|b| !b.trim().is_empty()) {
        let base = base.trim_end_matches('/').to_owned();
        return (base.clone(), format!("{base}/graphql"));
    }
    if host == "github.com" {
        (
            "https://api.github.com".to_owned(),
            "https://api.github.com/graphql".to_owned(),
        )
    } else {
        (
            format!("https://{host}/api/v3"),
            format!("https://{host}/api/graphql"),
        )
    }
}

/// Where a host's OAuth device flow lives.
#[must_use]
pub fn oauth(host: &str) -> String {
    std::env::var("SPECCODER_GITHUB_OAUTH")
        .ok()
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| format!("https://{host}"))
        .trim_end_matches('/')
        .to_owned()
}

/// A signed-in conversation with one host.
#[derive(Debug, Clone)]
pub struct Api {
    host: String,
    rest: String,
    graphql: String,
    token: Secret,
}

impl Api {
    /// Speaks to `host` as `token`, at `base` when given.
    #[must_use]
    pub fn new(host: &str, token: Secret, base: Option<&str>) -> Self {
        let (rest, graphql) = endpoints(host, base);
        Self {
            host: host.to_owned(),
            rest,
            graphql,
            token,
        }
    }

    fn request(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::RequestBuilder, String> {
        Ok(client()?
            .request(method, url)
            .bearer_auth(self.token.expose())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28"))
    }

    /// One REST call; `path` starts with `/`.
    ///
    /// # Errors
    ///
    /// The host's refusal, in words.
    pub fn rest(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let url = format!("{}{path}", self.rest);
        let mut req = self.request(method, &url)?;
        if let Some(b) = body {
            req = req.json(b);
        }
        block(async move {
            let resp = req.send().await.map_err(|e| unreachable(&self.host, &e))?;
            answer(&self.host, path, resp).await
        })
    }

    /// `GET path`.
    ///
    /// # Errors
    ///
    /// As [`Api::rest`].
    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.rest(reqwest::Method::GET, path, None)
    }

    /// `GET path` as a unified diff (a pull request's `.diff` media type):
    /// read-only, never a checkout.
    ///
    /// # Errors
    ///
    /// As [`Api::rest`].
    pub fn get_diff(&self, path: &str) -> Result<String, String> {
        let url = format!("{}{path}", self.rest);
        let req = self
            .request(reqwest::Method::GET, &url)?
            .header(reqwest::header::ACCEPT, "application/vnd.github.diff");
        let body = block(async move {
            let resp = req.send().await.map_err(|e| unreachable(&self.host, &e))?;
            answer(&self.host, path, resp).await
        })?;
        Ok(match body {
            Value::String(s) => s,
            Value::Null => String::new(),
            other => other.to_string(),
        })
    }

    /// One GraphQL document: its `data`.
    ///
    /// # Errors
    ///
    /// The host's refusal, or the first GraphQL error.
    pub fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        let req = self
            .request(reqwest::Method::POST, &self.graphql)?
            .json(&serde_json::json!({ "query": query, "variables": variables }));
        let body = block(async move {
            let resp = req.send().await.map_err(|e| unreachable(&self.host, &e))?;
            answer(&self.host, "graphql", resp).await
        })?;
        if let Some(e) = body["errors"].as_array().and_then(|e| e.first()) {
            return Err(format!(
                "GitHub: {}",
                e["message"].as_str().unwrap_or("the query failed")
            ));
        }
        Ok(body["data"].clone())
    }
}

/// A form POST answered in JSON (the OAuth endpoints).
pub(crate) fn form(url: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
    let req = client()?
        .post(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(fields);
    block(async move {
        let resp = req.send().await.map_err(|e| format!("{url}: {e}"))?;
        resp.json::<Value>()
            .await
            .map_err(|e| format!("{url}: {e}"))
    })
}

fn unreachable(host: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("{host} did not answer within {}s", LIMIT.as_secs())
    } else {
        format!("{host} could not be reached: {e}")
    }
}

/// The body of a 2xx; anything else as words with what to do.
async fn answer(host: &str, what: &str, resp: reqwest::Response) -> Result<Value, String> {
    let status = resp.status();
    let remaining = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let reset = resp
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let text = resp.text().await.unwrap_or_default();
    let body: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    if status.is_success() {
        return Ok(body);
    }
    let says = body["message"].as_str().unwrap_or_default().to_owned();
    Err(match status.as_u16() {
        401 => format!("{host} refused the token (expired or revoked): `speccoder auth login`"),
        403 | 429 if remaining.as_deref() == Some("0") => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let wait = reset.map_or(60, |r| r.saturating_sub(now));
            format!(
                "{host}'s rate limit is used up; it resets in {} min",
                wait.div_ceil(60).max(1)
            )
        }
        403 => format!(
            "the token may not do this ({says}); it needs `{SCOPES}`, or for a fine-grained token: Issues read, Pull requests read and write, Contents and Metadata read"
        ),
        404 if host == "github.com" => format!(
            "{what} was not found on {host}: the repository, or the token cannot see it — signed in with `speccoder auth login`? Install the {} app on the repository: {}",
            super::auth::APP_SLUG,
            super::auth::install_url()
        ),
        404 => {
            format!("{what} was not found on {host}: the repository, or the token cannot see it")
        }
        422 => {
            let detail: Vec<String> = body["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["message"].as_str().map(str::to_owned))
                .collect();
            if detail.is_empty() {
                format!("GitHub refused it: {says}")
            } else {
                format!("GitHub refused it: {says} ({})", detail.join("; "))
            }
        }
        code => format!("GitHub answered {code}: {says}"),
    })
}
