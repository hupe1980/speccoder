//! Feature directories: prefixes, ordering, selection.

use speccoder_core::snapshot::{Diagnostic, FeatureRef, Selection, SelectionReason, Severity};

/// The specs directory.
pub const SPECS: &str = "specs";

/// Splits `NNN-name` or `YYYYMMDD-HHMMSS-name` into its prefix, or `None`
/// when the directory has no numeric prefix.
#[must_use]
pub fn prefix(name: &str) -> Option<&str> {
    let digits = name.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &name[digits..];
    // Timestamp numbering: 8 digits, '-', 6 digits.
    if digits == 8 {
        let time = rest.strip_prefix('-').unwrap_or("");
        let t = time.bytes().take_while(u8::is_ascii_digit).count();
        if t == 6 && time[t..].starts_with('-') {
            return Some(&name[..digits + 1 + t]);
        }
    }
    rest.starts_with('-').then(|| &name[..digits])
}

/// Sort key: the prefix's digits as a number (the timestamp's two halves
/// concatenated), then the full name.
fn key(f: &FeatureRef) -> (u128, String) {
    let digits: String = f.prefix.chars().filter(char::is_ascii_digit).collect();
    (digits.parse().unwrap_or(u128::MAX), f.name.clone())
}

/// Builds and orders the feature list from directory names under `specs/`,
/// reporting directories without a numeric prefix.
pub fn collect(names: &[String], diags: &mut Vec<Diagnostic>) -> Vec<FeatureRef> {
    let mut features: Vec<FeatureRef> = names
        .iter()
        .filter_map(|name| match prefix(name) {
            Some(p) => Some(FeatureRef {
                dir: format!("{SPECS}/{name}"),
                name: name.clone(),
                prefix: p.to_owned(),
            }),
            None => {
                diags.push(
                    Diagnostic::new(
                        Severity::Info,
                        "discover.not-a-feature",
                        format!("{SPECS}/{name} has no numeric prefix; ignored"),
                    )
                    .at(format!("{SPECS}/{name}")),
                );
                None
            }
        })
        .collect();
    features.sort_by_key(key);
    features
}

/// Whether `arg` names `f`: its full name, its prefix, or the same number.
fn matches(f: &FeatureRef, arg: &str) -> bool {
    if f.name == arg || f.prefix == arg {
        return true;
    }
    match (arg.parse::<u128>(), f.prefix.parse::<u128>()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Selects a feature: argument > `SPECIFY_FEATURE_DIRECTORY` > recorded
/// `feature_directory` > the only feature.
pub fn select(
    features: &[FeatureRef],
    argument: Option<&str>,
    environment: Option<&str>,
    recorded: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) -> Selection {
    let available = || {
        features
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let none = Selection {
        reason: SelectionReason::None,
        feature: None,
    };

    if let Some(arg) = argument {
        return match features.iter().find(|f| matches(f, arg)) {
            Some(f) => Selection {
                reason: SelectionReason::Argument,
                feature: Some(f.name.clone()),
            },
            None => {
                diags.push(
                    Diagnostic::new(
                        Severity::Error,
                        "select.not-found",
                        format!("no feature matches `{arg}`"),
                    )
                    .hint(format!("available: {}", available())),
                );
                none
            }
        };
    }

    // Spec Kit's own order (`common.sh`): SPECIFY_FEATURE_DIRECTORY, then feature.json.
    let pointers = [
        (
            environment,
            SelectionReason::Environment,
            "SPECIFY_FEATURE_DIRECTORY",
        ),
        (recorded, SelectionReason::Recorded, ".specify/feature.json"),
    ];
    for (value, reason, source) in pointers {
        let Some(value) = value else { continue };
        let dir = value.trim_start_matches("./").trim_end_matches('/');
        return match features.iter().find(|f| f.dir == dir) {
            Some(f) => Selection {
                reason,
                feature: Some(f.name.clone()),
            },
            None => {
                let mut d = Diagnostic::new(
                    Severity::Error,
                    "select.not-found",
                    format!("{source} names `{value}`, which is not a feature directory inside the project"),
                )
                .hint(format!("pass --feature; available: {}", available()));
                if reason == SelectionReason::Recorded {
                    d = d.at(source);
                }
                diags.push(d);
                none
            }
        };
    }

    match features {
        [] => {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "select.no-features",
                    "the project has no features yet",
                )
                .hint("run /speckit-specify to create one"),
            );
            none
        }
        [only] => Selection {
            reason: SelectionReason::OnlyFeature,
            feature: Some(only.name.clone()),
        },
        _ => {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    "select.ambiguous",
                    "several features and none recorded as current",
                )
                .hint(format!("pass --feature; available: {}", available())),
            );
            none
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_sequential_and_timestamp_prefixes() {
        assert_eq!(prefix("002-speckit-inspect"), Some("002"));
        assert_eq!(prefix("20260319-143022-user-auth"), Some("20260319-143022"));
        assert_eq!(prefix("12345678-auth"), Some("12345678"));
        assert_eq!(prefix("notes"), None);
        assert_eq!(prefix("001"), None);
    }

    #[test]
    fn orders_numerically_not_lexically() {
        let names = ["010-c", "002-b", "001-a", "notes"].map(String::from);
        let mut diags = Vec::new();
        let order: Vec<_> = collect(&names, &mut diags)
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(order, ["001-a", "002-b", "010-c"]);
        assert_eq!(diags[0].code, "discover.not-a-feature");
    }
}
