#![deny(rust_2018_idioms, unused, unused_crate_dependencies, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        collections::HashMap,
        env,
        io,
        num::NonZero,
        path::Path,
        process,
        rc::Rc,
        time::{
            Duration,
            Instant,
        },
    },
    async_proto::Protocol as _,
    chrono::{
        TimeDelta,
        prelude::*,
    },
    chrono_tz::Tz,
    fontdue::{
        Font,
        FontSettings,
        layout::{
            GlyphRasterConfig,
            VerticalAlign,
        },
    },
    futures::stream::StreamExt as _,
    gefolge_websocket::event::{
        Event as GefolgeEvent,
        Delta as Packet,
    },
    if_chain::if_chain,
    rand::prelude::*,
    softbuffer::SoftBufferError,
    tiny_skia::*,
    tokio::{
        sync::{
            mpsc,
            oneshot,
        },
        time::sleep,
    },
    tokio_tungstenite::tungstenite,
    wheel::{
        fs,
        traits::LocalResultExt as _,
    },
    winit::{
        dpi::{
            LogicalSize,
            PhysicalSize,
        },
        event::{
            Event,
            StartCause,
            WindowEvent,
        },
        event_loop::{
            ControlFlow,
            EventLoop,
        },
        window::{
            Fullscreen,
            Window,
        },
    },
    crate::{
        config::Config,
        state::State,
    },
};
#[cfg(unix)] use {
    std::os::unix::process::CommandExt as _,
    itertools::Itertools as _,
    tokio::process::Command,
    wheel::traits::{
        AsyncCommandOutputExt as _,
        IoResultExt as _,
    },
};

mod config;
mod state;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[cfg(unix)] const BIN_PATH: &str = "/home/fenhl/bin/sil";
#[cfg(unix)] const REIWA_BIN_PATH: &str = "/home/fenhl/bin/sil-reiwa";

#[cfg(target_os = "linux")] const DEJAVU_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
#[cfg(target_os = "macos")] const DEJAVU_PATH: &str = "/Users/fenhl/Library/Fonts/DejaVuSans.ttf";
#[cfg(target_os = "windows")] const DEJAVU_PATH: &str = "\\Windows\\Fonts\\DejaVuSans.ttf";

const NIXOS_DEJAVU_PATH: &str = "/run/current-system/sw/share/X11/fonts/DejaVuSans.ttf";

trait ControlFlowExt {
    fn redraw_immediately(&mut self);
    fn redraw_at(&mut self, new_time: Instant);
}

impl ControlFlowExt for ControlFlow {
    fn redraw_immediately(&mut self) {
        *self = Self::Poll;
    }

    fn redraw_at(&mut self, new_time: Instant) {
        match self {
            ControlFlow::Wait => *self = Self::WaitUntil(new_time),
            ControlFlow::WaitUntil(prev_time) => *prev_time = (*prev_time).min(new_time),
            ControlFlow::Poll => {}
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum DrawError {
    #[error(transparent)] Text(#[from] text::Error),
    #[error(transparent)] TimeFromLocal(#[from] wheel::traits::TimeFromLocalError<DateTime<Tz>>),
}

struct DrawCache {
    dark: bool,
    state: State,
    canvas: Pixmap,
    redraw_at: ControlFlow,
    logo: Option<Pixmap>,
    text_layout: fontdue::layout::Layout,
    dejavu_sans: Font,
    glyph_cache: HashMap<(GlyphRasterConfig, [u8; 4]), Pixmap>, // ColorU8 does not implement Eq or Hash
}

impl DrawCache {
    fn draw(&mut self) -> Result<(), DrawError> {
        self.redraw_at = ControlFlow::Wait;
        let width = self.canvas.width() as f32;
        let height = self.canvas.height() as f32;
        let now_monotonic = Instant::now();
        let now_utc = Utc::now();
        #[cfg(debug_assertions)] {
            println!("{} redrawing for {:?}", now_utc.format("%Y-%m-%d %H:%M:%S"), self.state);
        }
        self.canvas.fill(if self.dark { Color::BLACK } else { Color::WHITE });
        match self.state {
            State::BinaryTime(tz) => {
                self.redraw_at.redraw_immediately();
                let width = self.canvas.width();
                let height = self.canvas.height();
                let now = now_utc.with_timezone(&tz);
                let bit_pattern = (now.time() - NaiveTime::from_hms_opt(0, 0, 0).expect("invalid hardcoded daytime")).to_std().expect("nonnegative time of day").as_secs_f32() * (65536.0 / 86_400.0);
                let bit_pattern = bit_pattern.floor() as u16;
                for (i, p) in self.canvas.pixels_mut().iter_mut().enumerate() {
                    let x = i as u32 % width;
                    let y = i as u32 / width;
                    let row = x * 4 / width;
                    let col = y * 4 / height;
                    *p = if bit_pattern & (1 << (4 * row + col)) != 0 {
                        PremultipliedColorU8::from_rgba(255, 255, 255, 255).expect("failed to construct premultiplied white")
                    } else {
                        PremultipliedColorU8::from_rgba(0, 0, 0, 255).expect("failed to construct premultiplied black")
                    };
                }
            }
            State::CloseWindows(tz) => {
                text::Builder::new(&self.dejavu_sans, &format!("Es ist {} Uhr.\nBitte alle Fenster schließen.", now_utc.with_timezone(&tz).format("%H:%M:%S")))
                    .color(if self.dark { Color::WHITE } else { Color::BLACK })
                    .size(100.0)
                    .build(&mut self.text_layout, [width, height])?
                    .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
            }
            State::Error(ref e) => {
                self.canvas.fill(Color::from_rgba8(0xff, 0x00, 0x00, 0xff));
                text::Builder::new(&self.dejavu_sans, &format!("{e}\n\n{e:?}"))
                    .color(Color::WHITE)
                    .size(100.0)
                    .build(&mut self.text_layout, [width, height])?
                    .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
            }
            State::HexagesimalTime(tz) => {
                let nanos_until_next_second = 1_000_000_000 - now_utc.timestamp_subsec_nanos() % 1_000_000_000;
                self.redraw_at.redraw_at(now_monotonic + Duration::from_nanos(nanos_until_next_second.into()));
                text::Builder::new(&self.dejavu_sans, &now_utc.with_timezone(&tz).format("%d.%m.%Y %H:%M:%S").to_string())
                    .color(if self.dark { Color::WHITE } else { Color::BLACK })
                    .size(100.0)
                    .build(&mut self.text_layout, [width, height])?
                    .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
            }
            State::Logo { msg } => {
                if let Some(logo) = &self.logo {
                    //TODO resize Gefolge logo on small resolutions
                    self.canvas.draw_pixmap(0, 0, logo.as_ref(), &PixmapPaint::default(), Transform::from_translate((width - logo.width() as f32) / 2.0, (height - logo.height() as f32) / 2.0), None);
                }
                text::Builder::new(&self.dejavu_sans, &format!("{width}x{height}"))
                    .color(if self.dark { Color::WHITE } else { Color::BLACK })
                    .size(24.0)
                    .valign(VerticalAlign::Top)
                    .build(&mut self.text_layout, [width, height])?
                    .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
                text::Builder::new(&self.dejavu_sans, msg)
                    .color(if self.dark { Color::WHITE } else { Color::BLACK })
                    .size(24.0)
                    .valign(VerticalAlign::Bottom)
                    .build(&mut self.text_layout, [width, height])?
                    .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
            }
            State::NewYear(tz) => {
                let now = now_utc.with_timezone(&tz);
                if now.month() > 6 {
                    let nanos_until_next_second = 1_000_000_000 - now_utc.timestamp_subsec_nanos() % 1_000_000_000;
                    self.redraw_at.redraw_at(now_monotonic + Duration::from_nanos(nanos_until_next_second.into()));
                    let mut delta = now.timezone().with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0).single_ok()? - now;
                    if delta < TimeDelta::minutes(1) {
                        text::Builder::new(&self.dejavu_sans, &delta.num_seconds().to_string())
                            .color(if self.dark { Color::WHITE } else { Color::BLACK })
                            .size(400.0)
                            .build(&mut self.text_layout, [width, height])?
                            .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
                    } else if delta < TimeDelta::hours(1) {
                        let mins = delta.num_minutes();
                        delta = delta - TimeDelta::minutes(mins);
                        text::Builder::new(&self.dejavu_sans, &format!("{mins}:{:02}", delta.num_seconds()))
                            .color(if self.dark { Color::WHITE } else { Color::BLACK })
                            .size(200.0)
                            .build(&mut self.text_layout, [width, height])?
                            .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
                    } else {
                        let hours = delta.num_hours();
                        delta = delta - TimeDelta::hours(hours);
                        let mins = delta.num_minutes();
                        delta = delta - TimeDelta::minutes(mins);
                        text::Builder::new(&self.dejavu_sans, &format!("{hours}:{mins:02}:{:02}", delta.num_seconds()))
                            .color(if self.dark { Color::WHITE } else { Color::BLACK })
                            .size(200.0)
                            .build(&mut self.text_layout, [width, height])?
                            .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
                    }
                } else {
                    text::Builder::new(&self.dejavu_sans, &now.year().to_string())
                        .color(if self.dark { Color::WHITE } else { Color::BLACK })
                        .size(400.0)
                        .build(&mut self.text_layout, [width, height])?
                        .draw(self.canvas.as_mut(), &mut self.glyph_cache)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn display_update_error() -> String {
    let current_exe = match env::current_exe() {
        Ok(current_exe) => current_exe,
        Err(e) => return format!("error determining current exe: {e}"),
    };
    let e = process::Command::new(if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() { REIWA_BIN_PATH } else { BIN_PATH }).exec();
    format!("error restarting for update: {e}")
}

#[cfg(windows)]
fn display_update_error() -> String {
    format!("please update sil")
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)] BaseDir(#[from] xdg_basedir::Error),
    #[error(transparent)] Config(#[from] config::Error),
    #[error(transparent)] Connection(#[from] tungstenite::Error),
    #[error(transparent)] EventLoop(#[from] winit::error::EventLoopError),
    #[error(transparent)] EventLoopClosed(#[from] winit::event_loop::EventLoopClosed<UserEvent>),
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Png(#[from] png::DecodingError),
    #[error(transparent)] Read(#[from] async_proto::ReadError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] SendState(#[from] mpsc::error::SendError<State>),
    #[error(transparent)] Task(#[from] tokio::task::JoinError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error(transparent)] Write(#[from] async_proto::WriteError),
    #[error("{0}")]
    Font(&'static str),
    #[error("failed to create canvas")]
    Pixmap,
    #[error("{display}")]
    Server {
        debug: String,
        display: String,
    },
    //HACK use the `Display` impl instead of directly calling `exec` to restart the program to make sure destructors run normally
    #[error("{}", display_update_error())]
    Update,
}

async fn update_check(commit_hash: [u8; 20]) -> Result<(), Error> {
    if commit_hash == GIT_COMMIT_HASH {
        Ok(())
    } else {
        #[cfg(unix)] {
            println!("updating sil from {:02x} to {:02x}", GIT_COMMIT_HASH.into_iter().take(4).format(""), commit_hash.iter().take(4).format(""));
            Command::new("scp").arg("reiwa:/opt/git/github.com/dasgefolge/sil/master/target/release/sil").arg(REIWA_BIN_PATH).check("scp").await?;
        }
        Err(Error::Update)
    }
}

fn pixmap_to_softbuf<D: raw_window_handle::HasDisplayHandle, W: raw_window_handle::HasWindowHandle>(pixmap: PixmapRef<'_>, mut buffer: softbuffer::Buffer<'_, D, W>) -> Result<(), SoftBufferError> {
    for (src, target) in pixmap.pixels().iter().zip_eq(&mut *buffer) {
        *target = (u32::from(src.red()) << 16) | (u32::from(src.green()) << 8) | u32::from(src.blue());
    }
    buffer.present() //TODO calculate changed rects and use present_with_damage? (need to keep track of previous coords and sizes of all items)
}

#[derive(Debug)]
enum UserEvent {
    State(State),
    Logo(Pixmap),
}

#[derive(clap::Parser)]
#[clap(version)]
struct Args {
    /// Exit on startup if there is no current event
    #[clap(long)]
    conditional: bool,
    /// Use a light theme with mostly white backgrounds and black text
    #[clap(short, long)]
    light: bool,
    /// Pretend that there's currently an ongoing event for debugging purposes
    #[clap(short, long)]
    mock_event: bool,
    #[clap(long)]
    no_self_update: bool,
    #[clap(short, long)]
    windowed: bool,
    /// Connect to the specified WebSocket server instead of gefolge.org
    #[clap(long, default_value = "wss://gefolge.org/api/websocket")]
    ws_url: String,
}

#[wheel::main]
async fn main(args: Args) -> Result<i32, Error> {
    #[cfg(unix)] let current_exe = env::current_exe().at_unknown()?; // determine at the start of the program, before anything can delete it
    #[cfg(unix)] {
        if current_exe == Path::new(REIWA_BIN_PATH) {
            fs::copy(REIWA_BIN_PATH, BIN_PATH).await?;
            return Err(process::Command::new(BIN_PATH).exec().into())
        } else if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() {
            fs::remove_file(REIWA_BIN_PATH).await?;
        }
    }
    if args.conditional && (env::var_os("STY").is_some() || env::var_os("SSH_CLIENT").is_some() || env::var_os("SSH_TTY").is_some()) { return Ok(0) } // only start on device startup, not when SSHing in etc
    let current_event = if args.mock_event {
        Some(GefolgeEvent {
            id: format!("mock"),
            timezone: chrono_tz::Europe::Berlin,
        })
    } else {
        let config = Config::new().await?;
        let (mut sink, mut stream) = tokio_tungstenite::connect_async(args.ws_url).await?.0.split();
        config.api_key.write_ws(&mut sink).await?;
        1u8.write_ws(&mut sink).await?; // session purpose: current event
        loop {
            let packet = Packet::read_ws(&mut stream).await?;
            break match packet {
                Packet::Ping => continue, //TODO send pong
                Packet::Error { debug, display } => return Err(Error::Server { debug, display }),
                Packet::NoEvent => if args.conditional { return Ok(0) } else { None },
                Packet::CurrentEvent(event) => Some(event),
                Packet::LatestVersion(commit_hash) => {
                    update_check(commit_hash).await?;
                    continue
                }
            }
        }
    };
    let mut cache = DrawCache {
        dark: !args.light,
        state: State::Logo {
            msg: "loading the loader",
        },
        canvas: Pixmap::new(100, 100).ok_or(Error::Pixmap)?,
        redraw_at: ControlFlow::Poll,
        logo: None,
        text_layout: fontdue::layout::Layout::new(fontdue::layout::CoordinateSystem::PositiveYDown),
        dejavu_sans: Font::from_bytes(if fs::exists(NIXOS_DEJAVU_PATH).await? {
            fs::read(NIXOS_DEJAVU_PATH).await?
        } else {
            fs::read(DEJAVU_PATH).await?
        }, FontSettings {
            scale: 100.0,
            ..FontSettings::default()
        }).map_err(Error::Font)?,
        glyph_cache: HashMap::default(),
    };
    let event_loop = EventLoop::with_user_event().build()?;
    let mut main_window = None::<(Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>)>;
    tokio::spawn(state::load_images(event_loop.create_proxy()));
    tokio::spawn(state::maintain(SmallRng::from_entropy(), current_event, event_loop.create_proxy()));
    let (exit_code_tx, mut exit_code_rx) = oneshot::channel();
    let mut exit_code_tx = Some(exit_code_tx);
    #[allow(deprecated)] /*TODO event_loop.run_app*/ let event_loop_result = tokio::task::block_in_place(move || event_loop.run(move |event, target| {
        macro_rules! winit_try {
            ($res:expr, $msg:literal) => {{
                match $res {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("{}: {e} ({e:?})", $msg);
                        if let Some(exit_code_tx) = exit_code_tx.take() {
                            let _ = exit_code_tx.send(1);
                        }
                        target.exit();
                        return
                    }
                }
            }};
        }

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. } | StartCause::Poll) => if let Some((ref window, _)) = main_window {
                window.request_redraw();
            }
            Event::WindowEvent { event, window_id } => if let Some((ref window, ref mut surface)) = main_window {
                if window_id == window.id() {
                    match event {
                        WindowEvent::CloseRequested => {
                            target.exit();
                            return
                        }
                        WindowEvent::RedrawRequested => {
                            let PhysicalSize { width, height } = window.inner_size();
                            if width != cache.canvas.width() || height != cache.canvas.height() {
                                match Pixmap::new(width, height) {
                                    Some(new_canvas) => cache.canvas = new_canvas,
                                    None => {
                                        eprintln!("failed to create a new canvas");
                                        if let Some(exit_code_tx) = exit_code_tx.take() {
                                            let _ = exit_code_tx.send(1);
                                        }
                                        target.exit();
                                        return
                                    }
                                }
                                if let (Some(width), Some(height)) = (NonZero::new(width), NonZero::new(height)) {
                                    winit_try!(surface.resize(width, height), "failed to resize the screen buffer");
                                }
                            }
                            winit_try!(cache.draw(), "failed to draw to the canvas");
                            let buffer = winit_try!(surface.buffer_mut(), "failed to get the screen buffer");
                            winit_try!(pixmap_to_softbuf(cache.canvas.as_ref(), buffer), "failed to present the screen buffer");
                        }
                        _ => {} //TODO handle more events (which?)
                    }
                }
            },
            Event::UserEvent(event) => {
                match event {
                    UserEvent::State(state) => cache.state = state,
                    UserEvent::Logo(img) => cache.logo = Some(img),
                }
                if let Some((ref window, _)) = main_window {
                    window.request_redraw();
                }
            }
            Event::Resumed => if main_window.is_none() {
                let mut window_attributes = Window::default_attributes();
                window_attributes.min_inner_size = Some(winit::dpi::Size::Logical(LogicalSize { width: 100.0, height: 100.0 }));
                window_attributes.title = format!("Gefolge-Silvester-Beamer");
                //TODO window_attributes.window_icon
                let size = if_chain! {
                    if let Some(monitor) = target.primary_monitor().or_else(|| target.available_monitors().max_by_key(|monitor| (monitor.size().width * monitor.size().height, monitor.refresh_rate_millihertz())));
                    if let Some(video_mode) = monitor.video_modes().min_by_key(|video_mode| (video_mode.size().width.abs_diff(monitor.size().width) + video_mode.size().height.abs_diff(monitor.size().height), -(video_mode.refresh_rate_millihertz() as i32)));
                    then {
                        cache.canvas = winit_try!(Pixmap::new(video_mode.size().width, video_mode.size().height).ok_or(Error::Pixmap), "failed to create new canvas");
                        let size = (NonZero::new(video_mode.size().width), NonZero::new(video_mode.size().height));
                        window_attributes.fullscreen = {
                            #[cfg(target_os = "macos")] { Some(Fullscreen::Borderless(None)) }
                            #[cfg(not(target_os = "macos"))] { Some(Fullscreen::Exclusive(video_mode)) }
                        };
                        size
                    } else {
                        (None, None)
                    }
                };
                let window = Rc::new(winit_try!(target.create_window(window_attributes), "failed to create window"));
                let context = winit_try!(softbuffer::Context::new(window.clone()), "failed to create window context");
                let mut surface = winit_try!(softbuffer::Surface::new(&context, window.clone()), "failed to create surface");
                if let (Some(width), Some(height)) = size {
                    winit_try!(surface.resize(width, height), "failed to resize surface");
                }
                if env::var_os("SWAYSOCK").is_some() {
                    tokio::spawn(async move {
                        sleep(Duration::from_secs(1)).await;
                        eprintln!("enabling fullscreen using swaymsg");
                        match Command::new("swaymsg").arg(format!("[pid={}] focus", process::id())).check("swaymsg").await {
                            Ok(_) => if let Err(e) = Command::new("swaymsg").arg("fullscreen").arg("enable").check("swaymsg").await {
                                eprintln!("failed to enable fullscreen using swaymsg: {e} ({e:?})");
                            },
                            Err(e) => eprintln!("failed to focus sil using swaymsg: {e} ({e:?})"),
                        }
                    });
                }
                window.set_cursor_visible(false);
                main_window = Some((window, surface));
            },
            _ => {} //TODO handle more events (which?)
        }
        target.set_control_flow(cache.redraw_at);
    }));
    /*
    match exit_code {
        4813 => {
            #[cfg(unix)] {
                let mut cmd = std::process::Command::new(if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() { REIWA_BIN_PATH } else { BIN_PATH });
                if args.no_self_update {
                    cmd.arg("--no-self-update");
                }
                Err(cmd.exec().into())
            }
            #[cfg(not(unix))] compile_error!("relaunch not yet implemented on non-Unix platforms");
        }
        code => Ok(code)
    }
    */
    match event_loop_result {
        Ok(()) => Ok(exit_code_rx.try_recv().unwrap_or_default()),
        Err(winit::error::EventLoopError::ExitFailure(code)) => Ok(code),
        Err(e) => Err(e.into()),
    }
}
