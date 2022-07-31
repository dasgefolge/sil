use {
    std::io,
    futures::{
        pin_mut,
        stream::{
            self,
            StreamExt as _,
        },
    },
    serde::Deserialize,
    tokio::{
        fs::File,
        io::AsyncReadExt as _,
    },
};

const PATH: &str = "fidera/client-config.json";

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error("config file missing, create at $XDG_CONFIG_DIRS/fidera/client-config.json")]
    MissingConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub(crate) api_key: String,
}

impl Config {
    pub(crate) async fn new() -> Result<Config, Error> {
        let dirs = stream::iter(xdg_basedir::get_config_home().into_iter().chain(xdg_basedir::get_config_dirs()));
        let files = dirs.filter_map(|cfg_dir| async move { File::open(cfg_dir.join(PATH)).await.ok() });
        pin_mut!(files);
        let mut file = files.next().await.ok_or(Error::MissingConfig)?;
        let mut buf = String::default();
        file.read_to_string(&mut buf).await?;
        Ok(serde_json::from_str(&buf)?) //TODO use async-json instead
    }
}
