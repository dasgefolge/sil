#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        env,
        io,
        os::unix::process::CommandExt as _,
        path::Path,
        time::Duration as StdDuration,
    },
    async_proto::Protocol as _,
    chrono::{
        Duration,
        prelude::*,
    },
    futures::stream::StreamExt as _,
    gefolge_websocket::event::{
        Event,
        Delta as Packet,
    },
    ggez::{
        Context,
        ContextBuilder,
        GameError,
        GameResult,
        conf::{
            FullscreenType,
            ModuleConf,
            WindowMode,
            WindowSetup,
        },
        event::EventHandler,
        filesystem,
        graphics::{
            self,
            Color,
            DrawMode,
            DrawParam,
            Drawable as _,
            Font,
            Image,
            Mesh,
            Rect,
            set_mode,
        },
        input::mouse,
        timer,
        winit::dpi::PhysicalSize,
    },
    image::ImageFormat,
    itertools::Itertools as _,
    rand::prelude::*,
    tokio::{
        fs,
        process::Command,
        sync::mpsc,
    },
    tokio_tungstenite::tungstenite,
    wheel::traits::AsyncCommandOutputExt as _,
    crate::{
        config::Config,
        state::State,
        text::*,
    },
};

mod config;
mod state;
mod text;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

const BIN_PATH: &str = "/home/fenhl/bin/sil";
const REIWA_BIN_PATH: &str = "/home/fenhl/bin/sil-reiwa";

struct Handler {
    resolution: PhysicalSize<u32>,
    bg: Color,
    fg: Color,
    dejavu_sans: Font,
    init: bool,
    state_rx: mpsc::Receiver<State>,
    state: State,
}

impl Handler {
    fn new(ctx: &mut Context, dark: bool, state_rx: mpsc::Receiver<State>) -> GameResult<Handler> {
        Ok(Handler {
            state_rx,
            resolution: PhysicalSize { width: 813, height: 813 },
            bg: if dark { Color::BLACK } else { Color::WHITE },
            fg: if dark { Color::WHITE } else { Color::BLACK },
            dejavu_sans: Font::new(ctx, "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")?,
            init: false,
            state: State::Logo {
                msg: "loading the loader",
                img: None,
            },
        })
    }
}

impl EventHandler<GameError> for Handler {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        if !self.init && timer::time_since_start(ctx) > StdDuration::from_secs(2) { //HACK wait 2 seconds before going fullscreen to circumvent a potential race condition where `set_mode` can be ignored if called too early
            if let Some(current_monitor) = graphics::window(&ctx).current_monitor() {
                self.resolution = current_monitor.size();
                let PhysicalSize { width, height } = self.resolution;
                let width = width as f32;
                let height = height as f32;
                set_mode(ctx, WindowMode {
                    width, height,
                    fullscreen_type: FullscreenType::True,
                    ..WindowMode::default()
                })?;
                mouse::set_cursor_hidden(ctx, true);
            } else {
                eprintln!("could not go fullscreen, no current monitor");
            }
            let (w, h) = graphics::drawable_size(ctx);
            graphics::set_screen_coordinates(ctx, Rect { x: 0.0, y: 0.0, w, h })?;
            self.init = true;
        }
        if let Ok(state) = self.state_rx.try_recv() {
            self.state = state;
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        graphics::clear(ctx, self.bg);
        match self.state {
            State::BinaryTime(tz) => {
                let now = Utc::now().with_timezone(&tz);
                let mut fract = (now.time() - NaiveTime::from_hms(0, 0, 0)).to_std().expect("nonnegative time of day").as_secs_f32() / 86_400.0;
                let bit_width = self.resolution.width as f32 / 4.0;
                let bit_height = self.resolution.height as f32 / 4.0;
                for y in 0..4 {
                    for x in 0..4 {
                        fract *= 2.0;
                        Mesh::new_rectangle(
                            ctx,
                            DrawMode::fill(),
                            Rect { x: x as f32 * bit_width, y: y as f32 * bit_height, w: bit_width, h: bit_height },
                            if fract >= 1.0 { fract -= 1.0; Color::WHITE } else { Color::BLACK },
                        )?.draw(ctx, DrawParam::default())?;
                    }
                }
            }
            State::Error(ref e) => {
                graphics::clear(ctx, Color::RED);
                TextBox::new(format!("{e}\n\n{e:?}")).color(Color::WHITE).draw(self, ctx)?;
            }
            State::HexagesimalTime(tz) => {
                TextBox::new(Utc::now().with_timezone(&tz).format("%d.%m.%Y %H:%M:%S").to_string()).draw(self, ctx)?;
            }
            State::Logo { msg, ref img } => {
                let Rect { w, h, .. } = graphics::screen_coordinates(ctx);
                TextBox::new(format!("{w}x{h} on {}x{}, {:.2}FPS", self.resolution.width, self.resolution.height, timer::fps(ctx))).size(24.0).valign(VerticalAlign::Top).draw(self, ctx)?;
                if let Some(img) = img {
                    let img = Image::from_bytes_with_format(ctx, &img, ImageFormat::Png)?;
                    img.draw(ctx, DrawParam::default().dest([self.resolution.width as f32 / 2.0, self.resolution.height as f32 / 2.0]).offset([0.5, 0.5]))?; //TODO resize Gefolge logo on small resolutions
                }
                TextBox::new(msg).size(24.0).valign(VerticalAlign::Bottom).draw(self, ctx)?;
            }
            State::NewYear(tz) => {
                let now = Utc::now().with_timezone(&tz);
                if now.month() > 6 {
                    let mut delta = now.timezone().ymd(now.year() + 1, 1, 1).and_hms(0, 0, 0) - now;
                    if delta < Duration::minutes(1) {
                        TextBox::new(delta.num_seconds().to_string()).size(400.0)
                    } else if delta < Duration::hours(1) {
                        let mins = delta.num_minutes();
                        delta = delta - Duration::minutes(mins);
                        TextBox::new(format!("{mins}:{:02}", delta.num_seconds())).size(200.0)
                    } else {
                        let hours = delta.num_hours();
                        delta = delta - Duration::hours(hours);
                        let mins = delta.num_minutes();
                        delta = delta - Duration::minutes(mins);
                        TextBox::new(format!("{hours}:{mins:02}:{:02}", delta.num_seconds())).size(200.0)
                    }.draw(self, ctx)?;
                } else {
                    TextBox::new(now.year().to_string()).size(400.0).draw(self, ctx)?;
                }
            }
        }
        graphics::present(ctx)?;
        timer::yield_now();
        Ok(())
    }
}

fn display_update_error() -> String {
    let current_exe = match env::current_exe() {
        Ok(current_exe) => current_exe,
        Err(e) => return format!("error determining current exe: {e}"),
    };
    let e = std::process::Command::new(if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() { REIWA_BIN_PATH } else { BIN_PATH }).exec();
    format!("error restarting for update: {e}")
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)] BaseDir(#[from] xdg_basedir::Error),
    #[error(transparent)] Config(#[from] config::Error),
    #[error(transparent)] Connection(#[from] tungstenite::Error),
    #[error(transparent)] Game(#[from] GameError),
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Read(#[from] async_proto::ReadError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] SendState(#[from] mpsc::error::SendError<State>),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error(transparent)] Write(#[from] async_proto::WriteError),
    #[error("{display}")]
    Server {
        //debug: String,
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
        println!("updating sil from {:02x} to {:02x}", GIT_COMMIT_HASH.into_iter().take(4).format(""), commit_hash.iter().take(4).format(""));
        Command::new("scp").arg("reiwa:/opt/git/github.com/dasgefolge/sil/master/target/release/sil").arg(REIWA_BIN_PATH).check("scp").await?;
        Err(Error::Update)
    }
}

#[derive(clap::Parser, wheel::IsVerbose)]
struct Args {
    /// Exit on startup if there is no current event
    #[clap(long)]
    conditional: bool,
    /// Use a light theme with mostly white backgrounds and black text
    #[clap(long)]
    light: bool,
    /// Pretend that there's currently an ongoing event for debugging purposes
    #[clap(long)]
    mock_event: bool,
    /// Include debug info in error exits
    #[clap(short, long)]
    verbose: bool,
    /// Connect to the specified WebSocket server instead of gefolge.org
    #[clap(long, default_value = "wss://gefolge.org/api/websocket")]
    ws_url: String,
}

#[wheel::main(verbose_debug)]
async fn main(args: Args) -> Result<(), Error> {
    let current_exe = env::current_exe()?;
    if current_exe == Path::new(REIWA_BIN_PATH) {
        fs::copy(REIWA_BIN_PATH, BIN_PATH).await?;
        return Err(std::process::Command::new(BIN_PATH).exec().into())
    } else if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() {
        fs::remove_file(REIWA_BIN_PATH).await?;
    }
    if args.conditional && (env::var_os("STY").is_some() || env::var_os("SSH_CLIENT").is_some() || env::var_os("SSH_TTY").is_some()) { return Ok(()) } // only start on device startup, not when SSHing in etc
    let current_event = if args.mock_event {
        Some(Event {
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
                Packet::Error { display, .. } => return Err(Error::Server { display }),
                Packet::NoEvent => if args.conditional { return Ok(()) } else { None },
                Packet::CurrentEvent(event) => Some(event),
                Packet::LatestVersion(commit_hash) => {
                    update_check(commit_hash).await?;
                    continue
                }
            }
        }
    };
    let (mut ctx, evt_loop) = ContextBuilder::new("sil", "Fenhl")
        .window_setup(WindowSetup {
            title: format!("Gefolge-Silvester-Beamer"),
            ..WindowSetup::default()
        })
        .window_mode(WindowMode {
            resizable: true,
            ..WindowMode::default()
        })
        .modules(ModuleConf {
            gamepad: false,
            audio: false,
        })
        .build()?;
    filesystem::mount(&mut ctx, Path::new("/"), true); // for font support
    let (state_tx, state_rx) = mpsc::channel(128);
    tokio::task::spawn(state::maintain(SmallRng::from_entropy(), current_event, state_tx));
    let handler = Handler::new(&mut ctx, !args.light, state_rx)?;
    tokio::task::block_in_place(move || ggez::event::run(ctx, evt_loop, handler))
}
