//! Where Bedrock's credentials will come from, named offline: the AWS
//! credential chain's first source, by kind and name, from environment
//! variable presence and the section headers and key names of the AWS
//! config and credentials files. No value is ever read into a message —
//! only a profile's `region` — and nothing here calls AWS.

use std::path::{Path, PathBuf};

/// A credential source: its kind and the variable or profile that names
/// it, never its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// What kind of source.
    pub kind: Kind,
    /// The environment variable or the profile.
    pub name: String,
}

/// The kinds of source, in the order the chain tries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A Bedrock API key in `AWS_BEARER_TOKEN_BEDROCK` (wins over the rest).
    ApiKey,
    /// `AWS_ACCESS_KEY_ID` and its secret in the environment.
    EnvKeys,
    /// `AWS_WEB_IDENTITY_TOKEN_FILE` (e.g. Kubernetes service accounts).
    WebIdentity,
    /// A container role (`AWS_CONTAINER_CREDENTIALS_*_URI`).
    Container,
    /// A profile in the AWS config or credentials file.
    Profile(ProfileKind),
}

/// What a profile's keys say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileKind {
    /// An `aws login` console session (`login_session`).
    Login,
    /// IAM Identity Center (`sso_session` or `sso_start_url`).
    Sso,
    /// `credential_process`.
    Process,
    /// `role_arn`.
    AssumeRole,
    /// `aws_access_key_id`.
    StaticKeys,
    /// `web_identity_token_file`.
    WebIdentity,
    /// `credential_source = Ec2InstanceMetadata`.
    InstanceRole,
    /// `credential_source = EcsContainer` (or `Environment`).
    ContainerRole,
    /// None of these keys.
    Other,
}

/// Where the region was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionFrom {
    /// `AWS_REGION` or `AWS_DEFAULT_REGION`.
    Env(&'static str),
    /// The profile's `region`.
    Profile(String),
}

/// The variable a Bedrock API key is read from.
pub const API_KEY: &str = "AWS_BEARER_TOKEN_BEDROCK";

impl Source {
    /// The words `doctor`, `init` and the app show for this source.
    #[must_use]
    pub fn describe(&self) -> String {
        let p = &self.name;
        match self.kind {
            Kind::ApiKey => format!("Bedrock API key ({API_KEY})"),
            Kind::EnvKeys => "access keys in the environment".to_owned(),
            Kind::WebIdentity => "web identity".to_owned(),
            Kind::Container => "container role".to_owned(),
            Kind::Profile(ProfileKind::Login) => format!("aws login session (profile {p})"),
            Kind::Profile(ProfileKind::Sso) => format!("SSO (profile {p})"),
            Kind::Profile(ProfileKind::Other) => format!("profile {p}"),
            Kind::Profile(kind) => {
                let words = match kind {
                    ProfileKind::Process => "credential process",
                    ProfileKind::AssumeRole => "assume role",
                    ProfileKind::StaticKeys => "static keys",
                    ProfileKind::WebIdentity => "web identity",
                    ProfileKind::InstanceRole => "instance role",
                    _ => "container role",
                };
                format!("profile {p} ({words})")
            }
        }
    }

    /// The fix for a refused or expired source: the command that renews it.
    #[must_use]
    pub fn renew(&self) -> String {
        let p = &self.name;
        match self.kind {
            Kind::ApiKey => format!("→ create a new Bedrock API key and export {API_KEY}"),
            Kind::EnvKeys => "→ export fresh AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY".to_owned(),
            Kind::WebIdentity | Kind::Container => {
                "→ check the role's trust and session".to_owned()
            }
            Kind::Profile(ProfileKind::Login) => format!("aws login --profile {p}"),
            Kind::Profile(ProfileKind::Sso) => format!("aws sso login --profile {p}"),
            Kind::Profile(_) => format!("→ renew the credentials of profile {p}"),
        }
    }
}

/// The first source the chain will use, or `None`: an API key, keys in
/// the environment, web identity, a container role, then the profile
/// (`AWS_PROFILE`, else `default`). `env` answers a variable's value when
/// it is set and not empty; `home` holds `.aws/`.
#[must_use]
pub fn detect(env: &dyn Fn(&str) -> Option<String>, home: &Path) -> Option<Source> {
    let set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
    let named = |kind, name: &str| {
        Some(Source {
            kind,
            name: name.to_owned(),
        })
    };
    if set(API_KEY) {
        return named(Kind::ApiKey, API_KEY);
    }
    if set("AWS_ACCESS_KEY_ID") {
        return named(Kind::EnvKeys, "AWS_ACCESS_KEY_ID");
    }
    if set("AWS_WEB_IDENTITY_TOKEN_FILE") {
        return named(Kind::WebIdentity, "AWS_WEB_IDENTITY_TOKEN_FILE");
    }
    for v in [
        "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
    ] {
        if set(v) {
            return named(Kind::Container, v);
        }
    }
    profile(env, home)
}

/// The profile source alone (`AWS_PROFILE`, else `default`), when the
/// config or credentials file has that profile.
#[must_use]
pub fn profile(env: &dyn Fn(&str) -> Option<String>, home: &Path) -> Option<Source> {
    let name = profile_name(env);
    let section = read(env, home, &name)?;
    let has = |key: &str| section.keys.iter().any(|k| k == key);
    let kind = if has("login_session") {
        ProfileKind::Login
    } else if has("sso_session") || has("sso_start_url") {
        ProfileKind::Sso
    } else if has("credential_process") {
        ProfileKind::Process
    } else if has("role_arn") {
        ProfileKind::AssumeRole
    } else if has("web_identity_token_file") {
        ProfileKind::WebIdentity
    } else if let Some(source) = &section.credential_source {
        if source.eq_ignore_ascii_case("Ec2InstanceMetadata") {
            ProfileKind::InstanceRole
        } else {
            ProfileKind::ContainerRole
        }
    } else if has("aws_access_key_id") {
        ProfileKind::StaticKeys
    } else {
        ProfileKind::Other
    };
    Some(Source {
        kind: Kind::Profile(kind),
        name,
    })
}

/// The region the AWS tools would use: `AWS_REGION`, `AWS_DEFAULT_REGION`,
/// then the profile's `region`.
#[must_use]
pub fn region(env: &dyn Fn(&str) -> Option<String>, home: &Path) -> Option<(String, RegionFrom)> {
    for v in ["AWS_REGION", "AWS_DEFAULT_REGION"] {
        if let Some(r) = env(v)
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty())
        {
            return Some((r, RegionFrom::Env(v)));
        }
    }
    let name = profile_name(env);
    let r = read(env, home, &name)?.region?;
    Some((r, RegionFrom::Profile(name)))
}

/// [`detect`] in this process's environment and home.
#[must_use]
pub fn detect_here() -> Option<Source> {
    detect(&var, &home())
}

/// [`region`] in this process's environment and home.
#[must_use]
pub fn region_here() -> Option<(String, RegionFrom)> {
    region(&var, &home())
}

/// A variable of this process, when set and not empty.
#[must_use]
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn profile_name(env: &dyn Fn(&str) -> Option<String>) -> String {
    env("AWS_PROFILE")
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "default".to_owned())
}

/// What a profile's sections hold: key names, and the two values detection
/// needs (`region`, `credential_source`) — never a credential.
#[derive(Default)]
struct Section {
    keys: Vec<String>,
    region: Option<String>,
    credential_source: Option<String>,
}

/// The profile `name` from both files, or `None` when neither has it.
fn read(env: &dyn Fn(&str) -> Option<String>, home: &Path, name: &str) -> Option<Section> {
    // Without a home, only a file a variable names.
    let file = |var: &str, default: &str| {
        let home = Some(home).filter(|h| !h.as_os_str().is_empty());
        match env(var) {
            Some(p) => match (p.strip_prefix("~/"), home) {
                (Some(rest), Some(h)) => Some(h.join(rest)),
                (Some(_), None) => None,
                (None, _) => Some(PathBuf::from(p)),
            },
            None => home.map(|h| h.join(".aws").join(default)),
        }
    };
    let config = file("AWS_CONFIG_FILE", "config");
    let credentials = file("AWS_SHARED_CREDENTIALS_FILE", "credentials");
    let mut found = false;
    let mut section = Section::default();
    // The config file names `[profile P]` (or `[default]`); the
    // credentials file names `[P]`.
    let in_config = |h: &str| {
        let words: Vec<&str> = h.split_whitespace().collect();
        words == ["profile", name] || (name == "default" && words == ["default"])
    };
    let in_credentials = |h: &str| h.trim() == name;
    for (path, is_ours) in [
        (&config, &in_config as &dyn Fn(&str) -> bool),
        (&credentials, &in_credentials),
    ] {
        let Some(Ok(text)) = path.as_ref().map(std::fs::read_to_string) else {
            continue;
        };
        let mut ours = false;
        for line in text.lines() {
            // Indented lines are a nested value's sub-keys.
            if line.starts_with([' ', '\t']) {
                continue;
            }
            let line = line.trim();
            if line.is_empty() || line.starts_with(['#', ';']) {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                ours = is_ours(header);
                found |= ours;
                continue;
            }
            let Some((key, value)) = line.split_once('=').filter(|_| ours) else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            // The only values read: neither is a credential.
            match key.as_str() {
                "region" if section.region.is_none() => {
                    section.region = Some(value.trim().to_owned()).filter(|r| !r.is_empty());
                }
                "credential_source" => {
                    section.credential_source = Some(value.trim().to_owned());
                }
                _ => {}
            }
            section.keys.push(key);
        }
    }
    found.then_some(section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Home {
        dir: tempfile::TempDir,
    }

    impl Home {
        fn new(config: &str, credentials: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join(".aws")).unwrap();
            if !config.is_empty() {
                std::fs::write(dir.path().join(".aws/config"), config).unwrap();
            }
            if !credentials.is_empty() {
                std::fs::write(dir.path().join(".aws/credentials"), credentials).unwrap();
            }
            Self { dir }
        }
    }

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k: &str| map.get(k).cloned().filter(|v| !v.is_empty())
    }

    fn described(vars: &[(&str, &str)], config: &str, credentials: &str) -> Option<String> {
        let home = Home::new(config, credentials);
        detect(&env(vars), home.dir.path()).map(|s| s.describe())
    }

    #[test]
    fn sources_are_named_in_the_chains_order() {
        let login = "[profile dev]\nlogin_session = arn:aws:iam::123456789012:user/me\nregion = eu-central-1\n";
        // (environment, config file, credentials file, the words)
        type Case<'a> = (&'a [(&'a str, &'a str)], &'a str, &'a str, Option<&'a str>);
        let cases: &[Case] = &[
            // The API key wins over a profile.
            (
                &[(API_KEY, "k"), ("AWS_PROFILE", "dev")],
                login,
                "",
                Some("Bedrock API key (AWS_BEARER_TOKEN_BEDROCK)"),
            ),
            (
                &[("AWS_ACCESS_KEY_ID", "AKIA"), ("AWS_PROFILE", "dev")],
                login,
                "",
                Some("access keys in the environment"),
            ),
            (
                &[("AWS_WEB_IDENTITY_TOKEN_FILE", "/t")],
                "",
                "",
                Some("web identity"),
            ),
            (
                &[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/v2")],
                "",
                "",
                Some("container role"),
            ),
            (
                &[("AWS_PROFILE", "dev")],
                login,
                "",
                Some("aws login session (profile dev)"),
            ),
            (
                &[],
                "[default]\nsso_session = corp\n[sso-session corp]\nsso_start_url = https://x\n",
                "",
                Some("SSO (profile default)"),
            ),
            (
                &[("AWS_PROFILE", "dev")],
                "[profile  dev]\nsso_start_url = https://x\n",
                "",
                Some("SSO (profile dev)"),
            ),
            (
                &[],
                "[default]\ncredential_process = /bin/creds\n",
                "",
                Some("profile default (credential process)"),
            ),
            (
                &[],
                "[default]\nrole_arn = arn:aws:iam::1:role/r\nsource_profile = base\n",
                "",
                Some("profile default (assume role)"),
            ),
            (
                &[],
                "[default]\ncredential_source = Ec2InstanceMetadata\n",
                "",
                Some("profile default (instance role)"),
            ),
            (
                &[("AWS_PROFILE", "ci")],
                "",
                "[ci]\naws_access_key_id = AKIA\naws_secret_access_key = SECRETVALUE\n",
                Some("profile ci (static keys)"),
            ),
            (
                &[],
                "[default]\nregion = us-east-1\n",
                "",
                Some("profile default"),
            ),
            // A profile neither file has is no source.
            (&[("AWS_PROFILE", "gone")], login, "", None),
            (&[], "", "", None),
            // An empty variable is not set.
            (&[(API_KEY, "")], "", "", None),
        ];
        for (vars, config, credentials, want) in cases {
            assert_eq!(
                described(vars, config, credentials).as_deref(),
                *want,
                "{vars:?} {config:?} {credentials:?}"
            );
        }
    }

    #[test]
    fn overridden_file_paths_are_read() {
        let home = Home::new("", "");
        let elsewhere = tempfile::tempdir().unwrap();
        let config = elsewhere.path().join("cfg");
        std::fs::write(
            &config,
            "[profile dev]\nlogin_session = x\nregion = ap-southeast-2\n",
        )
        .unwrap();
        let creds = elsewhere.path().join("creds");
        std::fs::write(&creds, "[ops]\naws_access_key_id = AKIA\n").unwrap();
        let (c, k) = (config.to_string_lossy(), creds.to_string_lossy());
        let vars = env(&[
            ("AWS_PROFILE", "dev"),
            ("AWS_CONFIG_FILE", &c),
            ("AWS_SHARED_CREDENTIALS_FILE", &k),
        ]);
        let s = detect(&vars, home.dir.path()).unwrap();
        assert_eq!(s.kind, Kind::Profile(ProfileKind::Login));
        assert_eq!(s.renew(), "aws login --profile dev");
        assert_eq!(
            region(&vars, home.dir.path()),
            Some((
                "ap-southeast-2".to_owned(),
                RegionFrom::Profile("dev".to_owned())
            ))
        );
        let ops = env(&[("AWS_PROFILE", "ops"), ("AWS_SHARED_CREDENTIALS_FILE", &k)]);
        assert_eq!(
            detect(&ops, home.dir.path()).map(|s| s.kind),
            Some(Kind::Profile(ProfileKind::StaticKeys))
        );
    }

    #[test]
    fn the_region_comes_from_the_environment_then_the_profile() {
        let home = Home::new(
            "[default]\nregion = eu-west-1\n[profile dev]\nregion = us-west-2\n",
            "",
        );
        let at = |vars: &[(&str, &str)]| region(&env(vars), home.dir.path());
        assert_eq!(
            at(&[
                ("AWS_REGION", "eu-central-1"),
                ("AWS_DEFAULT_REGION", "us-east-1")
            ]),
            Some(("eu-central-1".to_owned(), RegionFrom::Env("AWS_REGION")))
        );
        assert_eq!(
            at(&[("AWS_DEFAULT_REGION", "us-east-1")]),
            Some((
                "us-east-1".to_owned(),
                RegionFrom::Env("AWS_DEFAULT_REGION")
            ))
        );
        assert_eq!(
            at(&[("AWS_PROFILE", "dev")]),
            Some((
                "us-west-2".to_owned(),
                RegionFrom::Profile("dev".to_owned())
            ))
        );
        assert_eq!(
            at(&[]),
            Some((
                "eu-west-1".to_owned(),
                RegionFrom::Profile("default".to_owned())
            ))
        );
        assert_eq!(region(&env(&[]), Home::new("", "").dir.path()), None);
    }

    #[test]
    fn no_written_value_reaches_the_words() {
        let secrets = "[default]\naws_access_key_id = AKIAEXAMPLEID\naws_secret_access_key = SECRETVALUE\naws_session_token = TOKENVALUE\n";
        let home = Home::new(
            &format!("{secrets}login_session = arn:aws:iam::1:user/SESSIONVALUE\n"),
            secrets,
        );
        let s = detect(&env(&[]), home.dir.path()).unwrap();
        for words in [s.describe(), s.renew(), format!("{s:?}")] {
            for value in ["AKIAEXAMPLEID", "SECRETVALUE", "TOKENVALUE", "SESSIONVALUE"] {
                assert!(!words.contains(value), "{words}");
            }
        }
        let key = detect(&env(&[(API_KEY, "sk-test-secret-123")]), home.dir.path()).unwrap();
        assert!(
            !format!("{} {} {key:?}", key.describe(), key.renew()).contains("sk-test-secret-123")
        );
    }

    #[test]
    fn renewals_name_the_command_for_the_source() {
        let s = |kind, name: &str| Source {
            kind,
            name: name.to_owned(),
        };
        assert_eq!(
            s(Kind::ApiKey, API_KEY).renew(),
            "→ create a new Bedrock API key and export AWS_BEARER_TOKEN_BEDROCK"
        );
        assert_eq!(
            s(Kind::Profile(ProfileKind::Sso), "dev").renew(),
            "aws sso login --profile dev"
        );
        assert_eq!(
            s(Kind::Profile(ProfileKind::Process), "dev").renew(),
            "→ renew the credentials of profile dev"
        );
        assert_eq!(
            s(Kind::EnvKeys, "AWS_ACCESS_KEY_ID").renew(),
            "→ export fresh AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY"
        );
        assert_eq!(
            s(Kind::Container, "x").renew(),
            "→ check the role's trust and session"
        );
    }
}
