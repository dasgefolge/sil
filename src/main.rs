#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        env,
        io,
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
            WindowMode,
            WindowSetup,
        },
        event::EventHandler,
        graphics::{
            Canvas,
            Color,
            DrawMode,
            DrawParam,
            Drawable as _,
            FontData,
            //Image,
            Mesh,
            Rect,
            TextAlign,
        },
        input::mouse,
        timer,
        winit::dpi::PhysicalSize,
    },
    //image::ImageFormat,
    rand::prelude::*,
    tokio::sync::mpsc,
    tokio_tungstenite::tungstenite,
    crate::{
        config::Config,
        state::State,
        text::*,
    },
};
#[cfg(unix)] use {
    std::os::unix::process::CommandExt as _,
    itertools::Itertools as _,
    tokio::{
        fs,
        process::Command,
    },
    wheel::traits::AsyncCommandOutputExt as _,
};

mod config;
mod state;
mod text;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[cfg(unix)] const BIN_PATH: &str = "/home/fenhl/bin/sil";
#[cfg(unix)] const REIWA_BIN_PATH: &str = "/home/fenhl/bin/sil-reiwa";

#[cfg(target_os = "linux")] const DEJAVU_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
#[cfg(target_os = "macos")] const DEJAVU_PATH: &str = "/Users/fenhl/Library/Fonts/DejaVuSans.ttf";
#[cfg(target_os = "windows")] const DEJAVU_PATH: &str = "\\Windows\\Fonts\\DejaVuSans.ttf";

struct Handler {
    bg: Color,
    fg: Color,
    init: bool,
    state_rx: mpsc::Receiver<State>,
    state: State,
}

impl Handler {
    fn new(ctx: &mut Context, dark: bool, windowed: bool, state_rx: mpsc::Receiver<State>) -> GameResult<Handler> {
        ctx.gfx.add_font(DEJAVU, FontData::from_path(ctx, DEJAVU_PATH)?);
        Ok(Handler {
            state_rx,
            bg: if dark { Color::BLACK } else { Color::WHITE },
            fg: if dark { Color::WHITE } else { Color::BLACK },
            init: windowed,
            state: State::Logo {
                msg: "loading the loader",
                img: None,
            },
        })
    }
}

impl EventHandler<GameError> for Handler {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        if !self.init && ctx.time.time_since_start() > StdDuration::from_secs(2) { //HACK wait 2 seconds before going fullscreen to circumvent a potential race condition where `set_mode` can be ignored if called too early
            if let Some(current_monitor) = ctx.gfx.window().current_monitor() {
                let PhysicalSize { width, height } = current_monitor.size();
                let width = width as f32;
                let height = height as f32;
                ctx.gfx.set_mode(WindowMode {
                    width, height,
                    fullscreen_type: FullscreenType::True,
                    ..WindowMode::default()
                })?;
                mouse::set_cursor_hidden(ctx, true);
            } else {
                eprintln!("could not go fullscreen, no current monitor");
            }
            self.init = true;
        }
        if let Ok(state) = self.state_rx.try_recv() {
            self.state = state;
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let (w, h) = ctx.gfx.drawable_size();
        let mut canvas = Canvas::from_frame(&ctx.gfx, self.bg);
        canvas.set_screen_coordinates(Rect { x: 0.0, y: 0.0, w, h });
        match self.state {
            State::BinaryTime(tz) => {
                let now = Utc::now().with_timezone(&tz);
                let mut fract = (now.time() - NaiveTime::from_hms(0, 0, 0)).to_std().expect("nonnegative time of day").as_secs_f32() / 86_400.0;
                let bit_width = w / 4.0;
                let bit_height = h / 4.0;
                for y in 0..4 {
                    for x in 0..4 {
                        fract *= 2.0;
                        Mesh::new_rectangle(
                            &ctx.gfx,
                            DrawMode::fill(),
                            Rect { x: x as f32 * bit_width, y: y as f32 * bit_height, w: bit_width, h: bit_height },
                            if fract >= 1.0 { fract -= 1.0; Color::WHITE } else { Color::BLACK },
                        )?.draw(&mut canvas, DrawParam::default());
                    }
                }
            }
            State::CloseWindows(tz) => {
                TextBox::new(format!("Es ist {} Uhr.\nBitte alle Fenster schließen.", Utc::now().with_timezone(&tz).format("%H:%M:%S"))).draw(self, &mut canvas);
            }
            State::Error(ref e) => {
                canvas = Canvas::from_frame(&ctx.gfx, Color::RED);
                canvas.set_screen_coordinates(Rect { x: 0.0, y: 0.0, w, h });
                TextBox::new(format!("{e}\n\n{e:?}")).color(Color::WHITE).draw(self, &mut canvas);
            }
            State::HexagesimalTime(tz) => {
                TextBox::new(Utc::now().with_timezone(&tz).format("%d.%m.%Y %H:%M:%S").to_string()).draw(self, &mut canvas);
            }
            State::Logo { msg, ref img } => {
                let coords = canvas.screen_coordinates().expect("canvas has screen coordinates");
                assert_eq!(coords.w, w);
                assert_eq!(coords.h, h);
                if let Some(img) = img {
                    img.draw(&mut canvas, DrawParam::default().dest([w / 2.0, h / 2.0]).offset([0.5, 0.5])); //TODO resize Gefolge logo on small resolutions
                }
                TextBox::new(format!("{w}x{h}, {:.2}FPS", ctx.time.fps())).size(24.0).valign(TextAlign::Begin).draw(self, &mut canvas);
                TextBox::new(msg).size(24.0).valign(TextAlign::End).draw(self, &mut canvas);
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
                    }.draw(self, &mut canvas);
                } else {
                    TextBox::new(now.year().to_string()).size(400.0).draw(self, &mut canvas);
                }
            }
        }
        canvas.finish(&mut ctx.gfx)?;
        timer::yield_now();
        Ok(())
    }
}

#[cfg(unix)]
fn display_update_error() -> String {
    let current_exe = match env::current_exe() {
        Ok(current_exe) => current_exe,
        Err(e) => return format!("error determining current exe: {e}"),
    };
    let e = std::process::Command::new(if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() { REIWA_BIN_PATH } else { BIN_PATH }).exec();
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
        #[cfg(unix)] {
            println!("updating sil from {:02x} to {:02x}", GIT_COMMIT_HASH.into_iter().take(4).format(""), commit_hash.iter().take(4).format(""));
            Command::new("scp").arg("reiwa:/opt/git/github.com/dasgefolge/sil/master/target/release/sil").arg(REIWA_BIN_PATH).check("scp").await?;
        }
        Err(Error::Update)
    }
}

#[derive(clap::Parser, wheel::IsVerbose)]
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
    /// Include debug info in error exits
    #[clap(short, long)]
    verbose: bool,
    #[clap(short, long)]
    windowed: bool,
    /// Connect to the specified WebSocket server instead of gefolge.org
    #[clap(long, default_value = "wss://gefolge.org/api/websocket")]
    ws_url: String,
}

#[wheel::main(verbose_debug)]
async fn main(args: Args) -> Result<(), Error> {
    #[cfg(unix)] {
        let current_exe = env::current_exe()?;
        if current_exe == Path::new(REIWA_BIN_PATH) {
            fs::copy(REIWA_BIN_PATH, BIN_PATH).await?;
            return Err(std::process::Command::new(BIN_PATH).exec().into())
        } else if current_exe == Path::new(BIN_PATH) && Path::new(REIWA_BIN_PATH).exists() {
            fs::remove_file(REIWA_BIN_PATH).await?;
        }
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
    #[cfg(windows)] let root_path = Path::new("C:\\");
    #[cfg(not(windows))] let root_path = Path::new("/");
    let (mut ctx, evt_loop) = ContextBuilder::new("sil", "Fenhl")
        .window_setup(WindowSetup {
            title: format!("Gefolge-Silvester-Beamer"),
            ..WindowSetup::default()
        })
        .window_mode(WindowMode {
            resizable: true,
            ..WindowMode::default()
        })
        .add_resource_path(root_path)
        .build()?;
    let (state_tx, state_rx) = mpsc::channel(128);
    tokio::task::spawn(state::maintain(SmallRng::from_entropy(), current_event, state_tx));
    let handler = Handler::new(&mut ctx, !args.light, args.windowed, state_rx)?;
    tokio::task::block_in_place(move || ggez::event::run(ctx, evt_loop, handler))
}
