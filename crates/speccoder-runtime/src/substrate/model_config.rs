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
    reasoning_summary: Option<bool>,
    region: Option<String>,
}

impl ModelConfig {
    /// The environment variable holding this provider's API key.
    #[must_use]
    pub fn key_variable(&self) -> &str {
        match self.provider.as_str() {
            "openai" => "OPENAI_API_KEY",
            "openai-compatible" => self.api_key_env.as_deref().unwrap_or(""),
            // The variable hints name; the AWS chain may use another source.
            "bedrock" => "AWS_BEARER_TOKEN_BEDROCK",
            _ => "ANTHROPIC_API_KEY",
        }
    }

    /// Whether prompts carry cache breakpoints for this model: always, but
    /// on Bedrock only for Claude models (other families take no cache
    /// points).
    #[must_use]
    pub fn caches(&self) -> bool {
        self.provider != "bedrock" || self.model.contains("anthropic.")
    }

    /// How prompts are laid out for this provider.
    #[must_use]
    pub fn prompt_style(&self) -> PromptStyle {
        match self.provider.as_str() {
            "openai" => PromptStyle::OpenAi,
            "openai-compatible" => PromptStyle::Chat,
            "bedrock" => PromptStyle::Parts {
                caches: self.caches(),
            },
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
    /// Bedrock's Converse: the Anthropic layout, converted by [`to_parts`]
    /// to agentplane's shared text parts, whose `cache` marks become cache
    /// points (`caches`: Claude models only).
    Parts {
        /// Whether breakpoints are kept as `cache` marks.
        caches: bool,
    },
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

/// Converts an Anthropic-layout prompt to agentplane's shared shape (for
/// [`PromptStyle::Parts`]): `system` blocks and the first message's content
/// become text parts, a `cache_control` breakpoint a `cache` mark — `"1h"`
/// for the hour, `"5m"` otherwise — or none when `caches` is off. A string
/// `system` stays as it is; a prompt without `messages` is left untouched.
pub fn to_parts(v: &mut serde_json::Value, caches: bool) {
    use serde_json::{Value, json};
    let Some(o) = v.as_object_mut() else {
        return;
    };
    let Some(messages) = o.remove("messages") else {
        return;
    };
    let part = |block: &Value| {
        let text = block["text"].as_str().unwrap_or_default();
        match block.get("cache_control").filter(|_| caches) {
            Some(c) if c["ttl"] == "1h" => json!({ "type": "text", "text": text, "cache": "1h" }),
            Some(_) => json!({ "type": "text", "text": text, "cache": "5m" }),
            None => json!({ "type": "text", "text": text }),
        }
    };
    if let Some(Value::Array(blocks)) = o.get_mut("system") {
        *blocks = blocks.iter().map(part).collect();
    }
    let input: Vec<Value> = match &messages[0]["content"] {
        Value::String(text) => vec![json!({ "type": "text", "text": text })],
        content => content
            .as_array()
            .map(|blocks| blocks.iter().map(part).collect())
            .unwrap_or_default(),
    };
    o.insert("input".to_owned(), Value::Array(input));
}

/// Why the model could not be opened, as a result: a missing key is
/// `KEY-MISSING` with the exact `export` (and the other ways to get a
/// model), anything else `CONFIG-INVALID`. Every command that calls a model
/// says it the same way.
#[must_use]
pub fn refused(command: &str, error: &str) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus};
    let missing = error.split_whitespace().next().filter(|v| {
        error.contains("is not set")
            && v.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    });
    match missing {
        Some(variable) => CommandResult::new(command, CommandStatus::ConfigError, error)
            .with_code("KEY-MISSING")
            .with_fix(if variable == crate::aws::API_KEY {
                bedrock_fixes()
            } else {
                vec![
                    format!("export {variable}=…"),
                    "→ or another provider, or a local model: speccoder config set model.provider openai-compatible (see Settings › Model)".to_owned(),
                ]
            }),
        None => CommandResult::new(command, CommandStatus::ConfigError, error).with_code("CONFIG-INVALID"),
    }
}

/// What gets Bedrock credentials when none is found: a Bedrock API key, an
/// `aws login` session, or another provider.
#[must_use]
pub fn bedrock_fixes() -> Vec<String> {
    vec![
        format!("export {}=…", crate::aws::API_KEY),
        "aws login".to_owned(),
        "→ or another provider: speccoder config set model.provider anthropic (see Settings › Model)".to_owned(),
    ]
}

/// The fix for a provider refusing the account (`PROVIDER-REFUSED`): the
/// catalog's, after — for Bedrock — the command that renews `source`, the
/// credentials the calls were signed with.
#[must_use]
pub fn provider_refused_fix(
    model: &ModelConfig,
    source: Option<&crate::aws::Source>,
    feature: Option<&str>,
) -> Vec<String> {
    let mut fix: Vec<String> = source
        .filter(|_| model.provider == "bedrock")
        .map(crate::aws::Source::renew)
        .into_iter()
        .collect();
    if let Some(entry) = speccoder_core::codes::lookup("PROVIDER-REFUSED") {
        fix.extend(speccoder_core::codes::fixes(entry, "run", None, feature));
    }
    fix
}

/// The model driver for `model`, and the secret to redact from messages.
/// Keys come only from the environment; a local `openai-compatible` server
/// needs none unless `api_key_env` names one, and Bedrock's come from the
/// AWS credential chain (`AWS_BEARER_TOKEN_BEDROCK` or a profile).
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
    let key = if model.provider == "bedrock" {
        // The AWS chain finds the credentials, from sources speccoder cannot
        // see offline too (an instance role): the first call decides, and
        // `doctor` gives the advice. An API key, when set, is still the
        // secret to redact.
        crate::aws::var(&variable)
    } else if variable.is_empty() {
        None
    } else {
        Some(crate::aws::var(&variable).ok_or_else(|| {
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
        "bedrock" => {
            let region = model.region.clone().unwrap_or_default();
            // The AWS configuration loads asynchronously, and callers may sit
            // inside a runtime: on a thread of its own, with a runtime of its
            // own. The client makes its connections on the runtime of each
            // later call.
            let loaded = std::thread::scope(|s| {
                s.spawn(|| {
                    crate::pipeline::tokio_runtime()?
                        .block_on(agentplane::model::bedrock::Bedrock::from_env(region))
                })
                .join()
                .unwrap_or_else(|_| Err("loading the AWS configuration failed".to_owned()))
            });
            std::sync::Arc::new(loaded.map_err(|e| redact(format!("Amazon Bedrock: {e}")))?)
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
    /// Driver name: `anthropic`, `openai`, `openai-compatible` or `bedrock`.
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
    /// Ask for a summary of the model's reasoning on each attempt turn
    /// (`reasoning_summary`, default on): shown as its account, never as
    /// evidence. Providers without a way to ask (Chat Completions) give none.
    pub reasoning_summary: bool,
    /// `bedrock`: the AWS region calls go to (the journal records it).
    pub region: Option<String>,
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
        "anthropic" | "openai" | "openai-compatible" | "bedrock"
    ) {
        return Err(format!(
            "[model]: provider `{}` is not supported (anthropic, openai, openai-compatible and bedrock are)",
            raw.provider
        ));
    }
    let bedrock = raw.provider == "bedrock";
    if bedrock {
        // Credentials and the endpoint come from AWS; the region is the
        // file's, so the journal attests where the calls went.
        if raw.region.as_deref().is_none_or(|r| r.trim().is_empty()) {
            return Err("[model]: provider `bedrock` needs `region`, e.g. \"eu-central-1\" — speccoder config set model.region <region>".to_owned());
        }
        for (key, set) in [
            ("base_url", raw.base_url.is_some()),
            ("api_key_env", raw.api_key_env.is_some()),
            ("structured", raw.structured.is_some()),
        ] {
            if set {
                return Err(format!(
                    "[model]: `{key}` does not apply to provider `bedrock` (its credentials and endpoint come from AWS) — remove it"
                ));
            }
        }
    } else if raw.region.is_some() {
        return Err(format!(
            "[model]: `region` applies only to provider `bedrock`, not `{}` — speccoder config unset model.region",
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
    // Claude bills cache writes alike wherever it is served.
    let anthropic = raw.provider == "anthropic" || (bedrock && raw.model.contains("anthropic."));
    Ok(Some(ModelConfig {
        provider: raw.provider,
        model: raw.model,
        reasoning_summary: raw.reasoning_summary.unwrap_or(true),
        max_output_tokens: raw.max_output_tokens.unwrap_or(4096),
        max_run_tokens: raw.max_run_tokens.unwrap_or(200_000),
        cache_ttl: raw.cache_ttl.unwrap_or(CacheTtl::FiveMinutes),
        // Prices are the user's to state (they change, and differ by
        // contract); without input and output prices, cost is tokens only.
        // Unstated cache-write rates follow the provider's published rule:
        // Anthropic bills 1.25× input for the five-minute cache and 2× for
        // the hour (Claude on Bedrock too); OpenAI, compatible servers and
        // Bedrock's other families bill writes as input.
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
        region: raw.region.map(|r| r.trim().to_owned()),
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

    fn bedrock(extra: &str) -> Result<Option<ModelConfig>, String> {
        parse(&format!(
            "[model]\nprovider = \"bedrock\"\nmodel = \"global.anthropic.claude-sonnet-5-5\"\n{extra}"
        ))
    }

    #[test]
    fn bedrock_needs_a_region_and_takes_no_endpoint_or_key_settings() {
        let m = bedrock("region = \"eu-central-1\"\n").unwrap().unwrap();
        assert_eq!(m.region.as_deref(), Some("eu-central-1"));
        assert_eq!(m.key_variable(), "AWS_BEARER_TOKEN_BEDROCK");
        assert_eq!(m.prompt_style(), PromptStyle::Parts { caches: true });
        for missing in ["", "region = \"  \"\n"] {
            let e = bedrock(missing).unwrap_err();
            assert_eq!(
                e,
                "[model]: provider `bedrock` needs `region`, e.g. \"eu-central-1\" — speccoder config set model.region <region>"
            );
            assert_eq!(
                refused("run", &e).code().map(|c| c.name),
                Some("CONFIG-INVALID")
            );
        }
        for key in [
            "base_url = \"https://x\"",
            "api_key_env = \"K\"",
            "structured = \"tool\"",
        ] {
            let e = bedrock(&format!("region = \"us-east-1\"\n{key}\n")).unwrap_err();
            assert!(e.contains("does not apply to provider `bedrock`"), "{e}");
        }
        let e = parse("[model]\nprovider = \"anthropic\"\nmodel = \"m\"\nregion = \"us-east-1\"\n")
            .unwrap_err();
        assert!(
            e.contains("`region` applies only to provider `bedrock`"),
            "{e}"
        );
        let e = parse("[model]\nprovider = \"mistral\"\nmodel = \"m\"\n").unwrap_err();
        assert!(
            e.contains("anthropic, openai, openai-compatible and bedrock are"),
            "{e}"
        );
    }

    #[test]
    fn only_claude_on_bedrock_caches_and_prices_cache_writes_as_anthropic_does() {
        let nova = parse("[model]\nprovider = \"bedrock\"\nmodel = \"amazon.nova-pro-v1:0\"\nregion = \"us-east-1\"\nprice_input_per_mtok = 1.0\nprice_output_per_mtok = 4.0\n")
            .unwrap()
            .unwrap();
        assert!(!nova.caches());
        assert_eq!(nova.prompt_style(), PromptStyle::Parts { caches: false });
        // Writes at the input rate: no inferred Claude rule.
        assert!((nova.prices.unwrap().cache_write - 1.0).abs() < 1e-9);
        let claude = bedrock(
            "region = \"us-east-1\"\nprice_input_per_mtok = 3.0\nprice_output_per_mtok = 15.0\n",
        )
        .unwrap()
        .unwrap();
        assert!(claude.caches());
        let p = claude.prices.unwrap();
        assert!((p.cache_write - 3.75).abs() < 1e-9);
        assert!((p.cache_write_1h - 6.0).abs() < 1e-9);
    }

    #[test]
    fn the_anthropic_layout_becomes_text_parts_with_cache_marks() {
        let layout = serde_json::json!({
            "system": [
                { "type": "text", "text": "policy" },
                { "type": "text", "text": "constitution", "cache_control": { "type": "ephemeral", "ttl": "1h" } },
            ],
            "messages": [{ "role": "user", "content": [
                { "type": "text", "text": "plan", "cache_control": { "type": "ephemeral" } },
                { "type": "text", "text": "task" },
            ] }],
        });
        let mut cached = layout.clone();
        to_parts(&mut cached, true);
        assert_eq!(
            cached,
            serde_json::json!({
                "system": [
                    { "type": "text", "text": "policy" },
                    { "type": "text", "text": "constitution", "cache": "1h" },
                ],
                "input": [
                    { "type": "text", "text": "plan", "cache": "5m" },
                    { "type": "text", "text": "task" },
                ],
            })
        );
        let mut plain = layout;
        to_parts(&mut plain, false);
        assert!(!plain.to_string().contains("cache"), "{plain}");
        assert_eq!(plain["input"][1]["text"], "task");
        // A string instruction stays; a prompt without messages is untouched.
        let mut bare =
            serde_json::json!({ "system": "be brief", "input": [{ "type": "text", "text": "x" }] });
        let before = bare.clone();
        to_parts(&mut bare, true);
        assert_eq!(bare, before);
    }

    #[test]
    fn no_bedrock_credentials_is_key_missing_with_aws_s_ways_in() {
        let r = refused(
            "run",
            "AWS_BEARER_TOKEN_BEDROCK is not set and no AWS profile was found",
        );
        assert_eq!(r.code().map(|c| c.name), Some("KEY-MISSING"));
        assert_eq!(
            r.fix(),
            [
                "export AWS_BEARER_TOKEN_BEDROCK=…",
                "aws login",
                "→ or another provider: speccoder config set model.provider anthropic (see Settings › Model)",
            ]
        );
    }

    #[test]
    fn a_refused_bedrock_account_names_the_renewal_first() {
        let m = bedrock("region = \"us-east-1\"\n").unwrap().unwrap();
        let login = crate::aws::Source {
            kind: crate::aws::Kind::Profile(crate::aws::ProfileKind::Login),
            name: "dev".to_owned(),
        };
        let fix = provider_refused_fix(&m, Some(&login), Some("001-x"));
        assert_eq!(fix[0], "aws login --profile dev");
        assert_eq!(
            fix.last().map(String::as_str),
            Some("speccoder run --feature 001-x")
        );
        let other = parse("[model]\nprovider = \"openai\"\nmodel = \"m\"\n")
            .unwrap()
            .unwrap();
        assert!(!provider_refused_fix(&other, Some(&login), None)[0].starts_with("aws"));
    }
}
