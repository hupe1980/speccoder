//! `[model]` in `speccoder.toml`.

use serde::Deserialize;

/// Prompt-cache lifetime for breakpoints: agentplane's, read as `5m` or `1h`
/// (one hour survives a run paused for an approval).
pub use agentplane::model::CacheTtl;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    provider: String,
    model: String,
    max_output_tokens: Option<u32>,
    max_run_tokens: Option<u64>,
    cache_ttl: Option<CacheTtl>,
    price_input_per_mtok: Option<f64>,
    price_output_per_mtok: Option<f64>,
    price_cache_read_per_mtok: Option<f64>,
    price_cache_write_per_mtok: Option<f64>,
    price_cache_write_1h_per_mtok: Option<f64>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    structured: Option<String>,
}

impl ModelConfig {
    /// The environment variable holding this provider's API key.
    #[must_use]
    pub fn key_variable(&self) -> &str {
        match self.provider.as_str() {
            "openai" => "OPENAI_API_KEY",
            "openai-compatible" => self.api_key_env.as_deref().unwrap_or(""),
            _ => "ANTHROPIC_API_KEY",
        }
    }

    /// How prompts are laid out for this provider.
    #[must_use]
    pub fn prompt_style(&self) -> PromptStyle {
        match self.provider.as_str() {
            "openai" => PromptStyle::OpenAi,
            "openai-compatible" => PromptStyle::Chat,
            _ => PromptStyle::Anthropic,
        }
    }
}

/// Provider prompt dialects (same content, same stability order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStyle {
    /// Messages API: system blocks and content blocks with `cache_control` breakpoints.
    Anthropic,
    /// Responses API: `system` → `instructions` (a string), `input` items;
    /// prefix caching is automatic, so the order alone does the work.
    OpenAi,
    /// Chat Completions (`openai-compatible`: Ollama, vLLM, llama.cpp, LM
    /// Studio, …): the Anthropic layout without `cache_control` breakpoints,
    /// which such servers do not know.
    Chat,
}

/// Removes `cache_control` breakpoints from a prompt (for [`PromptStyle::Chat`]).
pub fn strip_cache_control(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(o) => {
            o.remove("cache_control");
            o.values_mut().for_each(strip_cache_control);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(strip_cache_control),
        _ => {}
    }
}

/// The model driver for `model`, and the secret to redact from messages.
/// Keys come only from the environment; a local `openai-compatible` server
/// needs none unless `api_key_env` names one.
pub fn open_driver(
    model: &ModelConfig,
) -> Result<
    (
        std::sync::Arc<dyn agentplane::model::ModelProvider>,
        Option<String>,
    ),
    String,
> {
    let variable = model.key_variable().to_owned();
    let key =
        if variable.is_empty() {
            None
        } else {
            Some(std::env::var(&variable).ok().filter(|k| !k.is_empty()).ok_or_else(|| {
            format!("{variable} is not set; speccoder reads the key only from the environment")
        })?)
        };
    let redact = |e: String| match &key {
        Some(k) if !k.is_empty() => e.replace(k.as_str(), "[redacted]"),
        _ => e,
    };
    let driver: std::sync::Arc<dyn agentplane::model::ModelProvider> = match model.provider.as_str()
    {
        "openai" => {
            let mut driver =
                agentplane::model::openai::OpenAi::new(key.clone().unwrap_or_default())
                    .map_err(|e| redact(e.to_string()))?;
            // A gateway in front of the Responses API.
            if let Some(base) = &model.base_url {
                driver = driver.base(base.trim_end_matches("/v1").to_owned());
            }
            std::sync::Arc::new(driver)
        }
        "openai-compatible" => {
            let base = model.base_url.clone().unwrap_or_default();
            let mut driver = agentplane::model::chat_completions::ChatCompletions::new(base)
                .map_err(|e| redact(e.to_string()))?;
            if let Some(k) = &key {
                driver = driver.bearer(k.clone());
            }
            if model.structured_native {
                driver = driver.structured_via(agentplane::model::SchemaMode::Native);
            }
            std::sync::Arc::new(driver)
        }
        _ => std::sync::Arc::new(
            agentplane::model::anthropic::Anthropic::new(key.clone().unwrap_or_default())
                .map_err(|e| redact(e.to_string()))?,
        ),
    };
    Ok((driver, key))
}

/// The model speccoder calls.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelConfig {
    /// Driver name: `anthropic` or `openai`.
    pub provider: String,
    /// The provider's model id, pinned exactly.
    pub model: String,
    /// Per-call output ceiling.
    pub max_output_tokens: u32,
    /// Token budget of one run.
    pub max_run_tokens: u64,
    /// Lifetime of prompt-cache breakpoints.
    pub cache_ttl: CacheTtl,
    /// USD per million tokens, if configured (`speccoder report --cost`).
    pub prices: Option<Prices>,
    /// `openai-compatible`: the server, e.g. `http://localhost:11434/v1`.
    pub base_url: Option<String>,
    /// `openai-compatible`: the environment variable holding a key, if the
    /// server wants one.
    pub api_key_env: Option<String>,
    /// `openai-compatible`: the server enforces `json_schema` output
    /// (`structured = "native"`, the default); `"tool"` carries it in a
    /// forced tool instead, which works only for steps without tools.
    pub structured_native: bool,
}

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    /// Input tokens neither written to nor read from the cache.
    pub input: f64,
    /// Output tokens.
    pub output: f64,
    /// Input tokens read from the cache.
    pub cache_read: f64,
    /// Input tokens written to the five-minute cache.
    pub cache_write: f64,
    /// Input tokens written to the one-hour cache.
    pub cache_write_1h: f64,
    /// Whether this model's cache writes go to the one-hour tier
    /// (`cache_ttl = "1h"`).
    pub one_hour: bool,
}

impl Prices {
    /// USD for these token counts (`input` counts every input token, cached
    /// ones included), priced by agentplane's [`Pricing`]: rounded up to a
    /// micro-dollar, cache writes at their own rate.
    ///
    /// [`Pricing`]: agentplane::model::Pricing
    #[must_use]
    pub fn usd(&self, input: u64, output: u64, cache_read: u64, cache_write: u64) -> f64 {
        // Rates in micro-dollars per million tokens: integers, as agentplane prices.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let micro = |usd: f64| (usd * 1_000_000.0).round().max(0.0) as u64;
        let pricing = agentplane::model::Pricing {
            input: micro(self.input),
            output: micro(self.output),
            cache_read: micro(self.cache_read),
            cache_write: micro(self.cache_write),
            cache_write_1h: micro(self.cache_write_1h),
        };
        let usage = agentplane::model::Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            cache_write_1h_tokens: if self.one_hour { cache_write } else { 0 },
            ..agentplane::model::Usage::default()
        };
        #[allow(clippy::cast_precision_loss)] // micro-dollars far below 2^52
        let usd = pricing.price(&usage) as f64 / 1_000_000.0;
        usd
    }
}

/// Reads `[model]` from the configuration text; `None` when absent.
pub fn parse(text: &str) -> Result<Option<ModelConfig>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(value) = table.get("model") else {
        return Ok(None);
    };
    let raw: Raw = value
        .clone()
        .try_into()
        .map_err(|e| format!("[model]: {e}"))?;
    if !matches!(
        raw.provider.as_str(),
        "anthropic" | "openai" | "openai-compatible"
    ) {
        return Err(format!(
            "[model]: provider `{}` is not supported (anthropic, openai and openai-compatible are)",
            raw.provider
        ));
    }
    if raw.provider == "openai-compatible" && raw.base_url.as_deref().is_none_or(str::is_empty) {
        return Err(
            "[model]: provider `openai-compatible` needs `base_url`, e.g. \"http://localhost:11434/v1\""
                .to_owned(),
        );
    }
    // Native by default: a forced-tool schema cannot be combined with the
    // tools an attempt declares, so only tool-less steps could use it.
    let structured_native = match raw.structured.as_deref() {
        None | Some("native") => true,
        Some("tool") => false,
        Some(other) => {
            return Err(format!(
                "[model]: structured = \"{other}\" (native or tool)"
            ));
        }
    };
    if raw.model.trim().is_empty() {
        return Err("[model]: `model` is empty".to_owned());
    }
    let anthropic = raw.provider == "anthropic";
    Ok(Some(ModelConfig {
        provider: raw.provider,
        model: raw.model,
        max_output_tokens: raw.max_output_tokens.unwrap_or(4096),
        max_run_tokens: raw.max_run_tokens.unwrap_or(200_000),
        cache_ttl: raw.cache_ttl.unwrap_or(CacheTtl::FiveMinutes),
        // Prices are the user's to state (they change, and differ by
        // contract); without input and output prices, cost is tokens only.
        // Unstated cache-write rates follow the provider's published rule:
        // Anthropic bills 1.25× input for the five-minute cache and 2× for
        // the hour; OpenAI and compatible servers bill writes as input.
        prices: match (raw.price_input_per_mtok, raw.price_output_per_mtok) {
            (Some(input), Some(output)) => Some(Prices {
                input,
                output,
                cache_read: raw.price_cache_read_per_mtok.unwrap_or(input),
                cache_write: raw.price_cache_write_per_mtok.unwrap_or(if anthropic {
                    input * 1.25
                } else {
                    input
                }),
                cache_write_1h: raw.price_cache_write_1h_per_mtok.unwrap_or(if anthropic {
                    input * 2.0
                } else {
                    input
                }),
                one_hour: raw.cache_ttl == Some(CacheTtl::OneHour),
            }),
            _ => None,
        },
        base_url: raw.base_url,
        api_key_env: raw.api_key_env,
        structured_native,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_writes_are_priced_at_their_own_rate() {
        let m = |extra: &str| {
            parse(&format!("[model]\nprovider = \"anthropic\"\nmodel = \"m\"\nprice_input_per_mtok = 3.0\nprice_output_per_mtok = 15.0\nprice_cache_read_per_mtok = 0.3\n{extra}"))
                .unwrap()
                .unwrap()
                .prices
                .unwrap()
        };
        // 1M written to the five-minute cache: 1.25 × $3 (input × 1.25 unless stated).
        assert!((m("").usd(1_000_000, 0, 0, 1_000_000) - 3.75).abs() < 1e-9);
        // The one-hour tier: 2 × $3.
        assert!((m("cache_ttl = \"1h\"\n").usd(1_000_000, 0, 0, 1_000_000) - 6.0).abs() < 1e-9);
        // Stated rates win; reads and fresh input as before.
        let p = m("price_cache_write_per_mtok = 4.0\n");
        assert!(
            (p.usd(2_000_000, 1_000_000, 500_000, 500_000) - (3.0 + 15.0 + 0.15 + 2.0)).abs()
                < 1e-9
        );
    }

    #[test]
    fn parses_model_with_defaults() {
        let m = parse("[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n")
            .unwrap()
            .unwrap();
        assert_eq!(m.model, "claude-sonnet-5-5");
        assert_eq!(m.max_output_tokens, 4096);
        assert_eq!(m.cache_ttl, CacheTtl::FiveMinutes);
        let one_hour =
            parse("[model]\nprovider = \"anthropic\"\nmodel = \"m\"\ncache_ttl = \"1h\"\n")
                .unwrap()
                .unwrap();
        assert_eq!(one_hour.cache_ttl, CacheTtl::OneHour);
    }

    #[test]
    fn rejects_unknown_keys_and_providers() {
        assert!(parse("[model]\nprovider = \"mistral\"\nmodel = \"m\"\n").is_err());
        let openai = parse("[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n")
            .unwrap()
            .unwrap();
        assert_eq!(openai.key_variable(), "OPENAI_API_KEY");
        assert_eq!(openai.prompt_style(), PromptStyle::OpenAi);
        assert!(parse("[model]\nprovider = \"anthropic\"\nmodel = \"m\"\nkey = \"x\"\n").is_err());
        assert_eq!(parse("[checks.t]\nargv = [\"true\"]\n").unwrap(), None);
    }

    #[test]
    fn an_openai_compatible_server_needs_a_base_url_and_a_key_only_if_named() {
        let m = parse("[model]\nprovider = \"openai-compatible\"\nmodel = \"qwen3\"\nbase_url = \"http://localhost:11434/v1\"\n")
            .unwrap()
            .unwrap();
        assert_eq!(m.prompt_style(), PromptStyle::Chat);
        assert!(m.structured_native, "native by default");
        assert_eq!(m.key_variable(), "");
        assert!(open_driver(&m).is_ok_and(|(_, key)| key.is_none()));
        assert!(parse("[model]\nprovider = \"openai-compatible\"\nmodel = \"x\"\n").is_err());
        let keyed = parse("[model]\nprovider = \"openai-compatible\"\nmodel = \"x\"\nbase_url = \"https://router.example/v1\"\napi_key_env = \"SPECCODER_TEST_NO_SUCH_KEY\"\n")
            .unwrap()
            .unwrap();
        assert!(
            open_driver(&keyed)
                .err()
                .unwrap_or_default()
                .contains("SPECCODER_TEST_NO_SUCH_KEY is not set")
        );
        let mut v = serde_json::json!({ "messages": [{ "content": [{ "type": "text", "text": "x", "cache_control": { "type": "ephemeral" } }] }] });
        strip_cache_control(&mut v);
        assert!(!v.to_string().contains("cache_control"));
    }
}
