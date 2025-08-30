use {
    std::{
        collections::HashSet,
        convert::Infallible as Never,
        pin::pin,
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
        future::Either,
        sink::SinkExt as _,
        stream::{
            self,
            StreamExt as _,
        },
    },
    gefolge_web_lib::websocket::{
        ClientMessageV2,
        ServerMessageV2,
    },
    rand::prelude::*,
    semver::Version,
    tiny_skia::Pixmap,
    tokio::{
        io::AsyncReadExt as _,
        select,
        time::{
            MissedTickBehavior,
            interval,
            sleep,
        },
    },
    wheel::{
        fs::{
            self,
            File,
        },
        traits::ReqwestResponseExt as _,
    },
    winit::event_loop::EventLoopProxy,
    crate::{
        Error,
        UserEvent,
        config::Config,
    },
};
#[cfg(unix)] use xdg::BaseDirectories;
#[cfg(windows)] use directories::ProjectDirs;
#[cfg(any(feature = "nixos", unix))] use {
    tokio::process::Command,
    wheel::traits::AsyncCommandOutputExt as _,
};
#[cfg(all(not(feature = "nixos"), unix))] use crate::REIWA_BIN_PATH;

pub(crate) struct Event {
    pub(crate) timezone: Tz,
}

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
                if now.month() == 1 && now.day() == 1 && now.hour() == 0 {
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

async fn load_images_inner(http_client: &reqwest::Client, states_tx: EventLoopProxy<UserEvent>) -> Result<(), Error> {
    if let Some(mut file) = {
        #[cfg(unix)] {
            pin!(
                stream::iter(BaseDirectories::new().find_cache_file("fidera/gefolge.png"))
                    .filter_map(|path| async move { File::open(path).await.ok() })
            ).next().await
        }
        #[cfg(windows)] {
            pin!(
                fs::read_dir(ProjectDirs::from("org", "Gefolge", "sil").ok_or(Error::MissingHomeDir)?.cache_dir())
                    .filter_map(|res| async move { File::open(res.ok()?.path()).await.ok() })
            ).next().await
        }
    } {
        let mut buf = Vec::default();
        file.read_to_end(&mut buf).await?;
        tokio::task::block_in_place(|| Ok::<_, Error>(states_tx.send_event(UserEvent::Logo(Pixmap::decode_png(&buf)?))?))?;
    } else {
        let cache_path = {
            #[cfg(unix)] {
                BaseDirectories::new().place_cache_file("fidera/gefolge.png")?
            }
            #[cfg(windows)] {
                ProjectDirs::from("org", "Gefolge", "sil").ok_or(Error::MissingHomeDir)?.cache_dir().join("gefolge.png")
            }
        };
        fs::create_dir_all(cache_path.parent().expect("attempted to create file at filesystem root")).await?;
        http_client.get("https://gefolge.org/static/gefolge.png")
            .send().await?
            .detailed_error_for_status().await?
            .download(&cache_path).await?;
        tokio::task::block_in_place(|| Ok::<_, Error>(states_tx.send_event(UserEvent::Logo(Pixmap::load_png(cache_path)?))?))?;
    };
    Ok(())
}

async fn update_check(states_tx: EventLoopProxy<UserEvent>, allow_self_update: bool, version: Version) -> Result<(), Error> {
    if version <= env!("CARGO_PKG_VERSION").parse().expect("failed to parse package version") {
        Ok(())
    } else {
        if allow_self_update {
            #[cfg(feature = "nixos")] {
                tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "updating Nix dependencies" })))?;
                Command::new("nix").arg("flake").arg("update").current_dir("/etc/nixos").check("nix flake update").await?;
                tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "switching NixOS config" })))?;
                Command::new("sudo").arg("nixos-rebuild").arg("switch").check("nixos-rebuild").await?;
            }
            #[cfg(not(feature = "nixos"))] {
                #[cfg(unix)] {
                    println!("updating sil from {} to {}", env!("CARGO_PKG_VERSION"), version);
                    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "downloading update" })))?;
                    Command::new("scp").arg("reiwa:/opt/git/github.com/dasgefolge/sil/main/target/release/sil").arg(REIWA_BIN_PATH).check("scp").await?;
                }
            }
        }
        tokio::task::block_in_place(|| states_tx.send_event(UserEvent::UpdateDone))?;
        Ok(())
    }
}

async fn maintain_inner(mut rng: impl Rng + Send, http_client: &reqwest::Client, mock_event: bool, allow_self_update: bool, ws_url: String, states_tx: EventLoopProxy<UserEvent>) -> Result<Never, Error> {
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "loading Gefolge logo" })))?;
    load_images_inner(http_client, states_tx.clone()).await?;
    if rng.gen_bool(0.1) {
        tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "reticulating splines" })))?;
        sleep(StdDuration::from_secs_f64(rng.gen_range(0.5..1.5))).await;
    }
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "getting current event" })))?;
    let (mut stream, mut current_event) = if mock_event {
        (
            Either::Left(stream::pending::<Result<ServerMessageV2, async_proto::ReadError>>()),
            Some(Event {
                timezone: chrono_tz::Europe::Berlin,
            }),
        )
    } else {
        let config = Config::load().await?;
        let (mut sink, mut stream) = async_proto::websocket027(ws_url).await?;
        sink.send(ClientMessageV2::Auth {
            api_key: config.api_key,
        }).await?;
        sink.send(ClientMessageV2::CurrentEvent).await?;
        let current_event = loop {
            break match stream.next().await.ok_or(Error::EndOfStream)?? {
                ServerMessageV2::Ping => continue, //TODO send pong
                ServerMessageV2::Error { debug, display } => return Err(Error::Server { debug, display }),
                ServerMessageV2::NoEvent => None,
                ServerMessageV2::CurrentEvent { id: _, timezone } => Some(Event { timezone }),
                ServerMessageV2::LatestSilVersion(version) => {
                    update_check(states_tx.clone(), allow_self_update, version).await?; //TODO run in background
                    continue
                }
            }
        };
        (Either::Right(stream), current_event)
    };
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "determining first mode" })))?;
    let mut seen_modes = HashSet::new();
    let mut interval = interval(StdDuration::from_secs(10));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        select! {
            res = stream.next() => match res.ok_or(Error::EndOfStream)?? {
                ServerMessageV2::Ping => continue, //TODO send pong
                ServerMessageV2::Error { debug, display } => return Err(Error::Server { debug, display }),
                ServerMessageV2::NoEvent => current_event = None,
                ServerMessageV2::CurrentEvent { id: _, timezone } => current_event = Some(Event { timezone }),
                ServerMessageV2::LatestSilVersion(version) => update_check(states_tx.clone(), allow_self_update, version).await?, //TODO run in background
            },
            _ = interval.tick() => {
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
            }
        }
    }
}

pub(crate) async fn maintain(rng: impl Rng + Send, http_client: reqwest::Client, mock_event: bool, allow_self_update: bool, ws_url: String, states_tx: EventLoopProxy<UserEvent>) {
    match maintain_inner(rng, &http_client, mock_event, allow_self_update, ws_url, states_tx.clone()).await {
        Ok(never) => match never {},
        Err(e) => { let _ = tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Error(Arc::new(e))))); }
    }
}
