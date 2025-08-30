use {
    serde::Deserialize,
    wheel::fs,
};
#[cfg(unix)] use xdg::BaseDirectories;
#[cfg(windows)] use directories::ProjectDirs;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)]
    #[error("config file missing, create at $XDG_CONFIG_DIRS/fidera/client-config.json")]
    MissingConfig,
    #[cfg(windows)]
    #[error("user folder not found")]
    MissingHomeDir,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub(crate) api_key: String,
}

impl Config {
    pub(crate) async fn load() -> Result<Config, Error> {
        #[cfg(unix)] {
            if let Some(config_path) = BaseDirectories::new().find_config_file("fidera/client-config.json") {
                Ok(fs::read_json(config_path).await?)
            } else {
                Err(Error::MissingConfig)
            }
        }
        #[cfg(windows)] {
            Ok(fs::read_json(ProjectDirs::from("org", "Gefolge", "sil").ok_or(Error::MissingHomeDir)?.config_dir().join("client-config.json")).await?)
        }
    }
}
