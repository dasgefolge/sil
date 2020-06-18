use {
    std::fs::File,
    derive_more::From,
    serde::Deserialize
};

#[derive(Debug, From)]
pub(crate) enum Error {
    Json(serde_json::Error),
    MissingConfig
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub(crate) api_key: String
}

impl Config {
    pub(crate) fn new() -> Result<Config, Error> {
        let dirs = xdg_basedir::get_config_home().into_iter().chain(xdg_basedir::get_config_dirs());
        let file = dirs.filter_map(|cfg_dir| File::open(cfg_dir.join("bitbar/plugins/speedruncom.json")).ok())
            .next().ok_or(Error::MissingConfig)?;
        Ok(serde_json::from_reader(file)?)
    }
}
