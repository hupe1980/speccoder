//! The judge's configuration as one hash: the policy a VSA names.

use serde_json::{Value, json};

/// The sections of `speccoder.toml` that decide a verdict.
const JUDGE: &[&str] = &[
    "checks",
    "acceptance",
    "tests",
    "mutation",
    "monitor",
    "sandbox",
    "executor",
];

/// sha256 over the canonical JSON of what decides a verdict: speccoder's
/// version, the judge's sections, the model (provider and name only) and
/// the signer. Comments, prices and key-variable names do not change it.
pub fn judge_config_sha256(config: &str) -> Result<String, String> {
    let table: toml::Table = toml::from_str(config).map_err(|e| e.to_string())?;
    let section = |k: &str| {
        table
            .get(k)
            .and_then(|v| serde_json::to_value(v).ok())
            .unwrap_or(Value::Null)
    };
    let mut judge = serde_json::Map::new();
    for k in JUDGE {
        judge.insert((*k).to_owned(), section(k));
    }
    let model = table.get("model");
    let field = |t: Option<&toml::Value>, k: &str| {
        t.and_then(|m| m.get(k))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };
    judge.insert(
        "model".to_owned(),
        json!({ "provider": field(model, "provider"), "model": field(model, "model") }),
    );
    judge.insert(
        "attest".to_owned(),
        json!({ "signer": field(table.get("attest"), "signer").unwrap_or_else(|| "git".to_owned()) }),
    );
    judge.insert(
        "speccoder_version".to_owned(),
        json!(env!("CARGO_PKG_VERSION")),
    );
    Ok(crate::integrity::sha256_hex(&super::statement::canonical(
        &Value::Object(judge),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "[model]\nprovider = \"openai\"\nmodel = \"m\"\nprice_input_per_mtok = 1.0\n\n[checks.test]\nargv = [\"cargo\", \"test\"]\n";

    #[test]
    fn comments_and_prices_keep_the_hash_and_a_check_changes_it() {
        let base = judge_config_sha256(CONFIG).unwrap();
        let commented = format!("# a note\n{}", CONFIG.replace("1.0", "2.5"));
        assert_eq!(judge_config_sha256(&commented).unwrap(), base);
        let keyed = CONFIG.replace(
            "model = \"m\"\n",
            "model = \"m\"\napi_key_env = \"MY_KEY\"\n",
        );
        assert_eq!(judge_config_sha256(&keyed).unwrap(), base);
        let other = CONFIG.replace("\"test\"]", "\"test\", \"--all\"]");
        assert_ne!(judge_config_sha256(&other).unwrap(), base);
        let model = CONFIG.replace("model = \"m\"", "model = \"n\"");
        assert_ne!(judge_config_sha256(&model).unwrap(), base);
    }
}
