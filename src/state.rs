use {
    std::{
        collections::HashSet,
        convert::Infallible as Never,
        sync::Arc,
        time::Duration,
    },
    chrono_tz::Tz,
    enum_iterator::IntoEnumIterator,
    futures::{
        pin_mut,
        stream::{
            self,
            StreamExt as _,
        },
    },
    gefolge_websocket::event::Event,
    rand::prelude::*,
    tokio::{
        fs::{
            self,
            File,
        },
        io::AsyncReadExt as _,
        sync::mpsc,
        time::sleep,
    },
    crate::Error,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, IntoEnumIterator)]
enum Mode {
    HexagesimalTime,
    Logo,
}

impl Mode {
    fn state(&self, current_event: Option<&Event>) -> Option<State> {
        match self {
            Self::HexagesimalTime => current_event.map(|event| State::HexagesimalTime(event.timezone)),
            Self::Logo => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum State {
    Error(Arc<Error>),
    HexagesimalTime(Tz),
    Logo {
        msg: &'static str,
        img: Option<Vec<u8>>,
    },
}

impl State {
    fn set(&mut self, new_state: &State) {
        if let (State::Logo { img: Some(img), .. }, State::Logo { img: None, msg }) = (&self, new_state) {
            *self = State::Logo { img: Some(img.clone()), msg: *msg };
        } else {
            *self = new_state.clone();
        }
    }

    fn set_message(&mut self, new_msg: &'static str) {
        match self {
            State::Logo { msg, .. } => *msg = new_msg,
            _ => *self = State::Logo { msg: new_msg, img: None },
        }
    }
}

async fn maintain_inner(mut rng: impl Rng, current_event: Option<Event>, states_tx: mpsc::Sender<State>) -> Result<Never, Error> {
    let mut state = State::Logo {
        msg: "loading Gefolge logo",
        img: None,
    };
    states_tx.send(state.clone()).await?;
    if let State::Logo { ref mut img, .. } = state {
        let dirs = stream::iter(xdg_basedir::get_cache_home().into_iter());
        let files = dirs.filter_map(|cfg_dir| async move { File::open(cfg_dir.join("fidera/gefolge.png")).await.ok() });
        pin_mut!(files);
        if let Some(mut file) = files.next().await {
            let mut buf = Vec::default();
            file.read_to_end(&mut buf).await?;
            *img = Some(buf);
        } else {
            let cache_dir = xdg_basedir::get_cache_home()?.join("fidera");
            fs::create_dir_all(&cache_dir).await?;
            let buf = reqwest::get("https://gefolge.org/static/gefolge.png").await?
                .error_for_status()?
                .bytes().await?
                .to_vec();
            fs::write(cache_dir.join("gefolge.png"), &buf).await?;
            *img = Some(buf);
        }
        states_tx.send(state.clone()).await?;
    }
    if rng.gen_bool(0.1) {
        state.set_message("reticulating splines");
        states_tx.send(state.clone()).await?;
        sleep(Duration::from_secs_f64(rng.gen_range(0.5..1.5))).await;
    }
    state.set_message("determining first mode");
    states_tx.send(state.clone()).await?;
    let mut seen_modes = HashSet::new();
    loop { //TODO keep listening to WebSocket
        let mut available_modes = Mode::into_enum_iter().filter_map(|mode| Some((mode, mode.state(current_event.as_ref())?))).collect::<Vec<_>>();
        if available_modes.iter().any(|(mode, _)| !seen_modes.contains(mode)) {
            available_modes.retain(|(mode, _)| !seen_modes.contains(mode));
        } else {
            seen_modes.clear();
        }
        if let Some((mode, new_state)) = available_modes.choose(&mut rng) {
            seen_modes.insert(*mode);
            state.set(new_state);
        } else {
            state.set_message("no modes available");
        };
        states_tx.send(state.clone()).await?;
        sleep(Duration::from_secs(10)).await;
    }
}

pub(crate) async fn maintain(rng: impl Rng, current_event: Option<Event>, states_tx: mpsc::Sender<State>) {
    match maintain_inner(rng, current_event, states_tx.clone()).await {
        Ok(never) => match never {},
        Err(e) => { let _ = states_tx.send(State::Error(Arc::new(e))).await; }
    }
}
