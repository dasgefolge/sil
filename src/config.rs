use {
    std::{
        fmt,
        io,
    },
    derive_more::From,
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

#[derive(Debug, From)]
pub(crate) enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    MissingConfig,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Json(e) => write!(f, "JSON error: {}", e),
            Error::MissingConfig => write!(f, "config file missing"),
        }
    }
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
