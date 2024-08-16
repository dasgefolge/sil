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
    tiny_skia::Pixmap,
    tokio::{
        fs::{
            self,
            File,
        },
        io::AsyncReadExt as _,
        time::sleep,
    },
    winit::event_loop::EventLoopProxy,
    crate::{
        Error,
        UserEvent,
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Sequence)]
enum Mode {
    BinaryTime,
    CloseWindows,
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
                let tomorrow = now.date_naive().succ_opt().expect("date overflow");
                if tomorrow.month() == 1 && tomorrow.day() == 1 {
                    Some((Priority::Normal, State::BinaryTime(timezone)))
                } else {
                    None
                }
            }
            Self::CloseWindows => {
                let timezone = current_event?.timezone;
                let now = Utc::now().with_timezone(&timezone);
                if now.hour() == 22 && now.minute() < 5 {
                    Some((Priority::Programm, State::CloseWindows(timezone)))
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
                    let tomorrow = now.date_naive().succ_opt().expect("date overflow");
                    (tomorrow.month() == 1 && tomorrow.day() == 1).then(|| if timezone.from_local_datetime(&tomorrow.and_hms_opt(0, 0, 0).expect("tomorrow has no midnight")).single().expect("failed to determine tomorrow at midnight") - now < Duration::hours(1).into() {
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
    CloseWindows(Tz),
    Error(Arc<Error>),
    HexagesimalTime(Tz),
    Logo {
        msg: &'static str,
    },
    NewYear(Tz),
}

async fn load_images_inner(state_tx: EventLoopProxy<UserEvent>) -> Result<(), Error> {
    let dirs = stream::iter(xdg_basedir::get_cache_home().into_iter());
    let files = dirs.filter_map(|cfg_dir| async move { File::open(cfg_dir.join("fidera/gefolge.png")).await.ok() });
    pin_mut!(files);
    if let Some(mut file) = files.next().await {
        let mut buf = Vec::default();
        file.read_to_end(&mut buf).await?;
        tokio::task::block_in_place(|| Ok::<_, Error>(state_tx.send_event(UserEvent::Logo(Pixmap::decode_png(&buf)?))?))?;
    } else {
        let cache_dir = xdg_basedir::get_cache_home()?.join("fidera");
        fs::create_dir_all(&cache_dir).await?;
        let buf = reqwest::get("https://gefolge.org/static/gefolge.png").await?
            .error_for_status()?
            .bytes().await?
            .to_vec();
        fs::write(cache_dir.join("gefolge.png"), &buf).await?;
        tokio::task::block_in_place(|| Ok::<_, Error>(state_tx.send_event(UserEvent::Logo(Pixmap::decode_png(&buf)?))?))?;
    }
    Ok(())
}

pub(crate) async fn load_images(state_tx: EventLoopProxy<UserEvent>) {
    if let Err(e) = load_images_inner(state_tx).await {
        eprintln!("error loading images: {e} (debug: {e:?})"); //TODO send error to event loop?
    }
}

async fn maintain_inner(mut rng: impl Rng + Send, current_event: Option<Event>, states_tx: EventLoopProxy<UserEvent>) -> Result<Never, Error> {
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "loading Gefolge logo" })))?;
    if rng.gen_bool(0.1) {
        tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "reticulating splines" })))?;
        sleep(StdDuration::from_secs_f64(rng.gen_range(0.5..1.5))).await;
    }
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "determining first mode" })))?;
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
            tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(new_state.clone())))?;
        } else {
            tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "no modes available" })))?;
        };
        sleep(StdDuration::from_secs(10)).await;
    }
}

pub(crate) async fn maintain(rng: impl Rng + Send, current_event: Option<Event>, states_tx: EventLoopProxy<UserEvent>) {
    match maintain_inner(rng, current_event, states_tx.clone()).await {
        Ok(never) => match never {},
        Err(e) => { let _ = tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Error(Arc::new(e))))); }
    }
}
