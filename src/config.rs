use {
    std::io,
    serde::Deserialize,
    tokio::{
        fs::File,
        io::AsyncReadExt as _,
    },
};
#[cfg(unix)] use futures::{
    pin_mut,
    stream::{
        self,
        StreamExt as _,
    },
};
#[cfg(windows)] use {
    directories::ProjectDirs,
    wheel::traits::IoResultExt as _,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[cfg(windows)] #[error(transparent)] Wheel(#[from] wheel::Error),
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
    pub(crate) async fn new() -> Result<Config, Error> {
        let mut file = {
            #[cfg(unix)] {
                let dirs = stream::iter(xdg_basedir::get_config_home().into_iter().chain(xdg_basedir::get_config_dirs()));
                let files = dirs.filter_map(|cfg_dir| async move { File::open(cfg_dir.join("fidera/client-config.json")).await.ok() });
                pin_mut!(files);
                files.next().await.ok_or(Error::MissingConfig)?
            }
            #[cfg(windows)] {
                let config_path = ProjectDirs::from("org", "Gefolge", "sil").ok_or(Error::MissingHomeDir)?.config_dir().join("client-config.json");
                File::open(&config_path).await.at::<wheel::Error, _>(config_path)?
            }
        };
        let mut buf = String::default();
        file.read_to_string(&mut buf).await?;
        Ok(serde_json::from_str(&buf)?) //TODO use async-json instead
    }
}
