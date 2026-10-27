use crate::Error;
use reqwest::Url;
use serde::{Deserialize, Deserializer};
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Profile {
    space_url: String,
    api_key_env: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Common {
    #[serde(default = "default_editor")]
    editor: Vec<String>,
    profiles: BTreeMap<String, Profile>,
}
fn default_editor() -> Vec<String> {
    vec!["vim".into()]
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default, deserialize_with = "present")]
    pub project: Option<String>,
    #[serde(default)]
    pub issue: IssueSettings,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSettings {
    #[serde(default)]
    pub create: CreateDefaults,
    #[serde(default)]
    pub list: ListDefaults,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateDefaults {
    #[serde(default, deserialize_with = "present")]
    pub issue_type: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub priority: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub assignee: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListDefaults {
    #[serde(default)]
    pub status: Vec<String>,
}

pub struct Config {
    pub origin: Url,
    pub api_key: String,
    pub editor: Vec<String>,
    pub settings: Settings,
}
impl Config {
    /// Load once from the environment and cwd; do not search parent directories.
    pub fn load(profile: &str) -> Result<Self, Error> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .filter(|p| Path::new(p).is_absolute())
            .or_else(|| env::var_os("HOME").map(|p| Path::new(&p).join(".config").into_os_string()))
            .ok_or_else(|| Error::new("config", "Configuration location is unavailable"))?;
        let text = read_config(&Path::new(&base).join("backlog/config.json"))
            .map_err(|_| Error::new("config", "Cannot read common configuration"))?
            .ok_or_else(|| Error::new("config", "Common configuration is absent"))?;
        let common: Common = serde_json::from_slice(&text)
            .map_err(|_| Error::new("config", "Invalid common configuration"))?;
        validate_editor(&common.editor)?;
        for (name, value) in &common.profiles {
            if name.trim().is_empty()
                || value.api_key_env.trim().is_empty()
                || value.api_key_env.contains(['\0', '='])
            {
                return Err(Error::new(
                    "config",
                    "Invalid profile or API key environment variable name",
                ));
            }
            origin(&value.space_url)?;
        }
        let selected = common
            .profiles
            .get(profile)
            .ok_or_else(|| Error::new("config", "Selected profile is missing"))?;
        let origin = origin(&selected.space_url)?;
        let api_key = env::var(&selected.api_key_env)
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                Error::new("config", "API key environment variable is missing or empty")
            })?;
        let settings: Settings = match read_config(Path::new(".backlog/settings.json"))
            .map_err(|_| Error::new("config", "Cannot read cwd settings"))?
        {
            Some(text) => serde_json::from_slice(&text)
                .map_err(|_| Error::new("config", "Invalid cwd settings"))?,
            None => Settings::default(),
        };
        let defaults = &settings.issue.create;
        if settings
            .project
            .iter()
            .chain(defaults.issue_type.iter())
            .chain(defaults.priority.iter())
            .chain(defaults.assignee.iter())
            .chain(settings.issue.list.status.iter())
            .any(|s| s.trim().is_empty())
        {
            return Err(Error::new("config", "Settings values must not be blank"));
        }
        Ok(Self {
            origin,
            api_key,
            editor: common.editor,
            settings,
        })
    }
}
fn read_config(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // An existing dangling symlink is a read failure, not absent settings.
            match fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(None),
                _ => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}
pub(crate) fn origin(value: &str) -> Result<Url, Error> {
    let error = || Error::new("config", "spaceUrl must be an HTTPS origin");
    // URL parsing normalizes dot segments and backslashes; reject them before normalization.
    let (scheme, authority) = value.split_once("://").ok_or_else(error)?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(error());
    }
    if authority.is_empty()
        || authority.contains(['\\', '\n', '\r', '\t'])
        || authority.split('/').next().is_some_and(|s| s.contains('@'))
        || authority.find('/').is_some_and(|i| &authority[i..] != "/")
    {
        return Err(error());
    }
    let url = Url::parse(value).map_err(|_| error())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error());
    }
    Ok(url)
}
fn validate_editor(argv: &[String]) -> Result<(), Error> {
    let error = || {
        Error::new(
            "config",
            "Editor must contain an executable program and valid argv",
        )
    };
    let program = argv.first().filter(|s| !s.is_empty()).ok_or_else(error)?;
    if argv.iter().any(|arg| arg.contains('\0')) {
        return Err(error());
    }
    let candidates: Vec<PathBuf> = if program.contains('/') {
        vec![PathBuf::from(program)]
    } else {
        let path = env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
        env::split_paths(&path)
            .map(|dir| dir.join(program))
            .collect()
    };
    if !candidates
        .iter()
        .any(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
    {
        return Err(error());
    }
    Ok(())
}
