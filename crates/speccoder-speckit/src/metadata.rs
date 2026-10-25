//! Spec Kit's own metadata files and the constitution.

use serde_json::Value;
use speccoder_core::snapshot::{ConstitutionState, Diagnostic, Severity, SpeckitMeta};

use crate::workspace::{ReadError, Workspace};

const INIT_OPTIONS: &str = ".specify/init-options.json";
const INTEGRATION: &str = ".specify/integration.json";
const FEATURE: &str = ".specify/feature.json";
const CONSTITUTION: &str = ".specify/memory/constitution.md";

/// Placeholders that only an unfilled constitution template contains.
const TEMPLATE_MARKERS: &[&str] = &["[PROJECT_NAME]", "[PRINCIPLE_1_NAME]"];

/// Reads the Spec Kit metadata, reporting missing or invalid files.
pub fn read(ws: &Workspace, diags: &mut Vec<Diagnostic>) -> SpeckitMeta {
    let mut meta = SpeckitMeta::default();
    if let Some(init) = read_json(ws, INIT_OPTIONS, true, diags) {
        meta.version = string(&init, "speckit_version");
        meta.integration = string(&init, "integration").or_else(|| string(&init, "ai"));
        meta.script = string(&init, "script");
        meta.feature_numbering =
            string(&init, "feature_numbering").or_else(|| string(&init, "branch_numbering"));
    }
    if let Some(integration) = read_json(ws, INTEGRATION, true, diags) {
        if meta.integration.is_none() {
            meta.integration = string(&integration, "default_integration")
                .or_else(|| string(&integration, "integration"));
        }
        if meta.version.is_none() {
            meta.version = string(&integration, "version");
        }
    }
    if let Some(feature) = read_json(ws, FEATURE, false, diags) {
        meta.current_feature = string(&feature, "feature_directory");
    }
    meta
}

/// Classifies the constitution.
pub fn constitution(ws: &Workspace, diags: &mut Vec<Diagnostic>) -> ConstitutionState {
    match ws.read_text(CONSTITUTION) {
        Ok(text) if TEMPLATE_MARKERS.iter().any(|m| text.contains(m)) => {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "discover.constitution-unfilled",
                    "the constitution is still the Spec Kit template",
                )
                .at(CONSTITUTION)
                .hint("speccoder spec constitution fills it in (/speckit-constitution in your editor)"),
            );
            ConstitutionState::Unfilled
        }
        Ok(_) => ConstitutionState::Present,
        Err(ReadError::Missing) => {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "discover.constitution-missing",
                    "no project constitution",
                )
                .at(CONSTITUTION)
                .hint("speccoder spec constitution (/speckit-constitution in your editor)"),
            );
            ConstitutionState::Absent
        }
        Err(e) => {
            diags.push(read_error(CONSTITUTION, &e));
            ConstitutionState::Absent
        }
    }
}

fn read_json(
    ws: &Workspace,
    rel: &str,
    report_missing: bool,
    diags: &mut Vec<Diagnostic>,
) -> Option<Value> {
    match ws.read_text(rel) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) if v.is_object() => Some(v),
            Ok(_) | Err(_) => {
                diags.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "discover.metadata-invalid",
                        format!("{rel} is not a JSON object"),
                    )
                    .at(rel),
                );
                None
            }
        },
        Err(ReadError::Missing) => {
            if report_missing {
                diags.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "discover.metadata-missing",
                        format!("{rel} not found"),
                    )
                    .at(rel)
                    .hint("re-run `specify init --here` to restore Spec Kit metadata"),
                );
            }
            None
        }
        Err(e) => {
            diags.push(read_error(rel, &e));
            None
        }
    }
}

fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// The diagnostic for a failed read of `rel`.
pub fn read_error(rel: &str, e: &ReadError) -> Diagnostic {
    match e {
        ReadError::Escape => Diagnostic::new(
            Severity::Error,
            "discover.escape",
            format!("{rel} resolves outside the project root; not read"),
        )
        .at(rel),
        ReadError::TooLarge(n) => Diagnostic::new(
            Severity::Error,
            "discover.too-large",
            format!(
                "{rel} is {n} bytes, above the {} byte limit; not parsed",
                crate::workspace::MAX_FILE_BYTES
            ),
        )
        .at(rel),
        ReadError::NotUtf8 => Diagnostic::new(
            Severity::Error,
            "discover.not-utf8",
            format!("{rel} is not valid UTF-8"),
        )
        .at(rel),
        ReadError::Missing => Diagnostic::new(
            Severity::Warning,
            "discover.missing",
            format!("{rel} not found"),
        )
        .at(rel),
        ReadError::Io(msg) => Diagnostic::new(
            Severity::Error,
            "discover.io",
            format!("{rel} could not be read: {msg}"),
        )
        .at(rel),
    }
}
