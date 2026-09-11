use {
    std::{
        collections::{
            HashSet,
            hash_map::{
                self,
                HashMap,
            },
        },
        convert::Infallible as Never,
        io::prelude::*,
        pin::pin,
        sync::{
            Arc,
            LazyLock,
        },
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
    gefolge_web_lib::{
        time::{
            MaybeAwareDateTime,
            MaybeLocalDateTime,
        },
        websocket::{
            ClientMessageV2,
            ServerMessageV2,
        },
    },
    log_lock::*,
    nonempty_collections::NEVec,
    rand::prelude::*,
    semver::Version,
    serde::Deserialize,
    serenity::model::prelude::*,
    tiny_skia::Pixmap,
    tokio::{
        io::AsyncReadExt as _,
        select,
        time::{
            Instant,
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

static NICK_CACHE: LazyLock<Mutex<HashMap<UserId, NickCacheEntry>>> = LazyLock::new(|| Mutex::default());

struct NickCacheEntry {
    nick: String,
    timestamp: Instant,
}

struct Event {
    id: String,
    calendar_events: Vec<CalEvent>,
    rtww_data: Option<RtwwData>,
    timezone: Tz,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalEvent {
    pub(crate) programmpunkt: Option<String>,
    pub(crate) start: MaybeAwareDateTime,
    pub(crate) end: MaybeAwareDateTime,
    pub(crate) text: String,
    pub(crate) ib_subtitle: Option<String>,
}

#[derive(Deserialize)]
struct RtwwData {
    players: Vec<RtwwPlayer>,
}

fn make_true() -> bool { true }

#[derive(Deserialize)]
struct RtwwPlayer {
    #[serde(default = "make_true")]
    alive: bool,
    id: UserId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Sequence)]
enum Mode {
    BinaryTime,
    CloseWindows,
    HexagesimalTime,
    Logo,
    NewYear,
    RtwwPlayerList,
    Schedule,
}

impl Mode {
    async fn state(&self, http_client: &reqwest::Client, api_key: &str, current_event: Option<&Event>) -> Result<Option<(Priority, State)>, Error> {
        let Some(current_event) = current_event else { return Ok(None) };
        Ok(match self {
            Self::BinaryTime => {
                let timezone = current_event.timezone;
                let now = Utc::now().with_timezone(&timezone);
                let tomorrow = now.date_naive().succ_opt().expect("date overflow");
                if tomorrow.month() == 1 && tomorrow.day() == 1 {
                    Some((Priority::Normal, State::BinaryTime(timezone)))
                } else {
                    None
                }
            }
            Self::CloseWindows => {
                let timezone = current_event.timezone;
                let now = Utc::now().with_timezone(&timezone);
                if now.hour() == 22 && now.minute() < 5 {
                    Some((Priority::Programm, State::CloseWindows(timezone)))
                } else {
                    None
                }
            }
            Self::HexagesimalTime => Some((Priority::Normal, State::HexagesimalTime(current_event.timezone))),
            Self::Logo => None,
            Self::NewYear => {
                let timezone = current_event.timezone;
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
            Self::RtwwPlayerList => {
                let Event { calendar_events, rtww_data, timezone, .. } = current_event;
                let Some(rtww_data) = &rtww_data else { return Ok(None) };
                let now = Utc::now().with_timezone(timezone);
                let Some(_) = calendar_events.iter().find(|cal_event|
                    cal_event.programmpunkt.as_ref().is_some_and(|programmpunkt| programmpunkt == "rtww")
                    && cal_event.start.to_maybe_local(Some(*timezone)).is_ok_and(|start| match start {
                        MaybeLocalDateTime::Nonlocal(_) => false,
                        MaybeLocalDateTime::Local(start) => start <= now,
                    })
                    && cal_event.end.to_maybe_local(Some(*timezone)).is_ok_and(|end| match end {
                        MaybeLocalDateTime::Nonlocal(_) => false,
                        MaybeLocalDateTime::Local(end) => end > now,
                    })
                ) else { return Ok(None) };
                let mut rows = Vec::default();
                for player in &rtww_data.players {
                    if player.alive {
                        rows.push(get_nick(http_client, api_key, player.id).await?);
                    }
                }
                Some((Priority::Programm, State::RtwwPlayerList(rows.join("\n"))))
            }
            Self::Schedule => {
                let Event { id, calendar_events, timezone, .. } = current_event;
                let now = Utc::now().with_timezone(timezone);
                let schedule = calendar_events.iter()
                    .filter(|cal_event| cal_event.end.to_maybe_local(Some(*timezone)).is_ok_and(|end| end > now))
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>();
                NEVec::try_from_vec(schedule).map(|schedule| (Priority::Normal, State::Schedule { use_weekdays: !id.starts_with("sil"), tz: *timezone, schedule }))
            }
        })
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
    RtwwPlayerList(String),
    Schedule {
        use_weekdays: bool,
        tz: Tz,
        schedule: NEVec<CalEvent>,
    },
}

#[derive(Deserialize)]
struct Profile {
    nick: Option<String>,
    username: String,
}

async fn get_nick(http_client: &reqwest::Client, api_key: &str, snowflake: UserId) -> Result<String, Error> {
    Ok(lock!(nick_cache = NICK_CACHE; match nick_cache.entry(snowflake) {
        hash_map::Entry::Occupied(mut entry) => if entry.get().timestamp.elapsed() < StdDuration::from_hours(1) {
            entry.get().nick.clone()
        } else {
            let Profile { nick, username } = http_client.get(format!("https://gefolge.org/api/mensch/{snowflake}/profile.json"))
                .basic_auth("api", Some(api_key))
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error().await?;
            entry.insert(NickCacheEntry { nick: nick.unwrap_or(username), timestamp: Instant::now() }).nick.clone()
        },
        hash_map::Entry::Vacant(mut entry) => {
            let Profile { nick, username } = http_client.get(format!("https://gefolge.org/api/mensch/{snowflake}/profile.json"))
                .basic_auth("api", Some(api_key))
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error().await?;
            entry.insert(NickCacheEntry { nick: nick.unwrap_or(username), timestamp: Instant::now() }).nick.clone()
        }
    }))
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

async fn update_check(#[cfg_attr(not(feature = "nixos"), allow(unused))] config: &Config, states_tx: EventLoopProxy<UserEvent>, allow_self_update: bool, version: Version) -> Result<(), Error> {
    if version <= env!("CARGO_PKG_VERSION").parse().expect("failed to parse package version") {
        Ok(())
    } else {
        if allow_self_update {
            #[cfg(feature = "nixos")] {
                tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "switching NixOS config" })))?;
                Command::new("/run/wrappers/bin/sudo")
                    .arg("/run/current-system/sw/bin/nixos-rebuild")
                    .arg("switch")
                    .arg("--recreate-lock-file")
                    .arg("--refresh")
                    .arg("--no-write-lock-file")
                    .arg("--show-trace")
                    .arg("--flake").arg(format!("https://start.fenhl.net/nixos.tar.gz?auth={}", config.nixos_auth))
                    .check("nixos-rebuild").await?;
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
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacyEventData {
        calendar_events: Vec<CalEvent>,
    }

    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "loading Gefolge logo" })))?;
    load_images_inner(http_client, states_tx.clone()).await?;
    if rng.random_bool(0.1) {
        tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "reticulating splines" })))?;
        sleep(StdDuration::from_secs_f64(rng.random_range(0.5..1.5))).await;
    }
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "getting current event" })))?;
    let config = Config::load().await?;
    let (mut stream, mut current_event) = if mock_event {
        (
            Either::Left(stream::pending::<Result<ServerMessageV2, async_proto::ReadError>>()),
            Some(Event {
                id: Utc::now().format("sil%Y").to_string(),
                calendar_events: Vec::default(),
                rtww_data: None,
                timezone: chrono_tz::Europe::Berlin,
            }),
        )
    } else {
        let (mut sink, mut stream) = async_proto::websocket030(ws_url).await?;
        sink.send(ClientMessageV2::Auth {
            api_key: config.api_key.clone(),
        }).await?;
        sink.send(ClientMessageV2::CurrentEvent).await?;
        let current_event = loop {
            break match stream.next().await.ok_or(Error::EndOfStream)?? {
                ServerMessageV2::Ping => continue, //TODO send pong
                ServerMessageV2::Error { debug, display } => return Err(Error::Server { debug, display }),
                ServerMessageV2::NoEvent => None,
                ServerMessageV2::CurrentEvent { id, timezone } => {
                    let LegacyEventData { calendar_events } = http_client.get(format!("https://gefolge.org/api/event/{id}/overview.json"))
                        .basic_auth("api", Some(&config.api_key))
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error().await?;
                    let rtww_setup_path = format!("/usr/local/share/fidera/games/werewolf/rtww/{id}/setup.json");
                    let rtww_data = if Command::new("ssh").arg("gefolge.org").arg("test").arg("-f:").arg(&rtww_setup_path).status().await?.success() {
                        Some(serde_json::from_slice(&Command::new("ssh").arg("gefolge.org").arg("cat").arg(rtww_setup_path).output().await?.stdout)?)
                    } else {
                        None
                    };
                    Some(Event { id, calendar_events, rtww_data, timezone })
                }
                ServerMessageV2::LatestSilVersion(version) => {
                    update_check(&config, states_tx.clone(), allow_self_update, version).await?; //TODO run in background
                    continue
                }
                ServerMessageV2::MarkdownPreview(_) => return Err(Error::UnexpectedMessage),
            }
        };
        (Either::Right(stream), current_event)
    };
    tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Logo { msg: "determining first mode" })))?;
    let mut seen_modes = HashSet::new();
    let mut mode_interval = interval(StdDuration::from_secs(10));
    mode_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut legacy_update_interval = interval(StdDuration::from_mins(5));
    legacy_update_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        select! {
            res = stream.next() => match res.ok_or(Error::EndOfStream)?? {
                ServerMessageV2::Ping => continue, //TODO send pong
                ServerMessageV2::Error { debug, display } => return Err(Error::Server { debug, display }),
                ServerMessageV2::NoEvent => current_event = None,
                ServerMessageV2::CurrentEvent { id, timezone } => {
                    let LegacyEventData { calendar_events } = http_client.get(format!("https://gefolge.org/api/event/{id}/overview.json"))
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error().await?;
                    current_event = Some(Event {
                        rtww_data: None,
                        id, calendar_events, timezone,
                    });
                }
                ServerMessageV2::LatestSilVersion(version) => update_check(&config, states_tx.clone(), allow_self_update, version).await?, //TODO run in background
                ServerMessageV2::MarkdownPreview(_) => return Err(Error::UnexpectedMessage),
            },
            _ = mode_interval.tick() => {
                let mut available_modes = Vec::default();
                for mode in all::<Mode>() {
                    if let Some(state) = mode.state(http_client, &config.api_key, current_event.as_ref()).await? {
                        available_modes.push((mode, state));
                    }
                }
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
            _ = legacy_update_interval.tick() => if let Some(current_event) = &mut current_event {
                let LegacyEventData { calendar_events } = http_client.get(format!("https://gefolge.org/api/event/{}/overview.json", current_event.id))
                    .send().await?
                    .detailed_error_for_status().await?
                    .json_with_text_in_error().await?;
                current_event.calendar_events = calendar_events;
            },
        }
    }
}

pub(crate) async fn maintain(rng: impl Rng + Send, http_client: reqwest::Client, mock_event: bool, allow_self_update: bool, ws_url: String, states_tx: EventLoopProxy<UserEvent>) {
    match maintain_inner(rng, &http_client, mock_event, allow_self_update, ws_url, states_tx.clone()).await {
        Ok(never) => match never {},
        Err(e) => {
            if let Ok(mut log) = std::fs::OpenOptions::new().append(true).create(true).open("sil.log") { let _ = write!(log, "{e}\n\n{e:?}"); }
            println!("{e}\n\n{e:?}");
            let _ = tokio::task::block_in_place(|| states_tx.send_event(UserEvent::State(State::Error(Arc::new(e)))));
        }
    }
}
