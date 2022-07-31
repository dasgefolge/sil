use {
    std::{
        collections::HashSet,
        convert::Infallible as Never,
        sync::Arc,
        time::Duration as StdDuration,
    },
    chrono::{
        Duration,
        prelude::*,
    },
    chrono_tz::Tz,
    enum_iterator::{
        Sequence,
        all,
    },
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Sequence)]
enum Mode {
    BinaryTime,
    HexagesimalTime,
    Logo,
    NewYear,
}

impl Mode {
    fn state(&self, current_event: Option<&Event>) -> Option<(Priority, State)> {
        match self {
            Self::BinaryTime => {
                let timezone = current_event?.timezone;
                let now = Utc::now().with_timezone(&timezone);
                let tomorrow = now.date().succ();
                if tomorrow.month() == 1 && tomorrow.day() == 1 {
                    Some((Priority::Normal, State::BinaryTime(timezone)))
                } else {
                    None
                }
            }
            Self::HexagesimalTime => Some((Priority::Normal, State::HexagesimalTime(current_event?.timezone))),
            Self::Logo => None,
            Self::NewYear => {
                let timezone = current_event?.timezone;
                let now = Utc::now().with_timezone(&timezone);
                if now.month() == 1 && now.day() == 1 && now.hour() == 1 {
                    Some(Priority::Programm)
                } else {
                    let tomorrow = now.date().succ();
                    (tomorrow.month() == 1 && tomorrow.day() == 1).then(|| if tomorrow.and_hms(0, 0, 0) - now < Duration::hours(1).into() {
                        Priority::Programm
                    } else {
                        Priority::Normal
                    })
                }.map(|priority| (priority, State::NewYear(timezone)))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Fallback,
    Normal,
    Programm,
}

#[derive(Debug, Clone)]
pub(crate) enum State {
    BinaryTime(Tz),
    Error(Arc<Error>),
    HexagesimalTime(Tz),
    Logo {
        msg: &'static str,
        img: Option<Vec<u8>>,
    },
    NewYear(Tz),
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
        sleep(StdDuration::from_secs_f64(rng.gen_range(0.5..1.5))).await;
    }
    state.set_message("determining first mode");
    states_tx.send(state.clone()).await?;
    let mut seen_modes = HashSet::new();
    loop { //TODO keep listening to WebSocket
        let mut available_modes = all::<Mode>().filter_map(|mode| Some((mode, mode.state(current_event.as_ref())?))).collect::<Vec<_>>();
        let max_priority = available_modes.iter().map(|(_, (priority, _))| *priority).max().unwrap_or(Priority::Fallback);
        available_modes.retain(|(_, (iter_priority, _))| *iter_priority == max_priority);
        if available_modes.iter().any(|(mode, _)| !seen_modes.contains(mode)) {
            available_modes.retain(|(mode, _)| !seen_modes.contains(mode));
        } else {
            seen_modes.clear();
        }
        if let Some((mode, (_, new_state))) = available_modes.choose(&mut rng) {
            seen_modes.insert(*mode);
            state.set(new_state); //TODO reload image if necessary
        } else {
            state.set_message("no modes available");
        };
        states_tx.send(state.clone()).await?;
        sleep(StdDuration::from_secs(10)).await;
    }
}

pub(crate) async fn maintain(rng: impl Rng, current_event: Option<Event>, states_tx: mpsc::Sender<State>) {
    match maintain_inner(rng, current_event, states_tx.clone()).await {
        Ok(never) => match never {},
        Err(e) => { let _ = states_tx.send(State::Error(Arc::new(e))).await; }
    }
}
