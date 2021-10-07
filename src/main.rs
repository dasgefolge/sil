#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        env,
        fmt,
        path::Path,
    },
    async_proto::Protocol as _,
    derive_more::From,
    futures::stream::StreamExt as _,
    gefolge_websocket::event::Delta as Packet,
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
            DrawParam,
            Drawable as _,
            Font,
            Rect,
            Text,
            TextFragment,
            set_mode,
            supported_resolutions,
        },
        input::mouse,
        timer,
    },
    structopt::StructOpt,
    tokio_tungstenite::tungstenite,
    winit::dpi::PhysicalSize,
    crate::config::Config,
};

mod config;

struct Handler {
    bg: Color,
    dejavu_sans: Font,
    fg: Color,
    init: bool
}

impl Handler {
    fn new(ctx: &mut Context, dark: bool) -> GameResult<Handler> {
        Ok(Handler {
            bg: if dark { Color::BLACK } else { Color::WHITE },
            dejavu_sans: Font::new(ctx, "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")?,
            fg: if dark { Color::WHITE } else { Color::BLACK },
            init: false
        })
    }
}

impl EventHandler<GameError> for Handler {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        if !self.init {
            let (w, h) = graphics::drawable_size(ctx);
            graphics::set_screen_coordinates(ctx, Rect { x: 0.0, y: 0.0, w, h })?;
            mouse::set_cursor_hidden(ctx, true);
            self.init = true;
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        graphics::clear(ctx, self.bg);
        //TODO draw the rest of the fucking owl
        let text = Text::new(TextFragment::new((format!("FPS: {}", timer::fps(ctx)), self.dejavu_sans, 100.0)).color(self.fg));
        text.draw(ctx, DrawParam::default())?;
        graphics::present(ctx)?;
        timer::yield_now();
        Ok(())
    }
}

#[derive(From)]
enum Error {
    Config(config::Error),
    Connection(tungstenite::Error),
    Game(GameError),
    Read(async_proto::ReadError),
    Reqwest(reqwest::Error),
    Server {
        //debug: String,
        display: String,
    },
    Write(async_proto::WriteError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(e) => write!(f, "config error: {}", e),
            Error::Connection(e) => write!(f, "WebSocket connection error: {}", e),
            Error::Game(e) => write!(f, "GUI error: {}", e),
            Error::Read(e) => write!(f, "error reading from WebSocket: {}", e),
            Error::Reqwest(e) => if let Some(url) = e.url() {
                write!(f, "HTTP error at {}: {}", url, e)
            } else {
                write!(f, "HTTP error: {}", e)
            },
            Error::Server { display } => write!(f, "server error: {}", display),
            Error::Write(e) => write!(f, "error writing to WebSocket: {}", e),
        }
    }
}

#[derive(StructOpt)]
struct Args {
    #[structopt(long)]
    conditional: bool,
}

#[wheel::main]
async fn main(args: Args) -> Result<(), Error> {
    if args.conditional && (env::var_os("STY").is_some() || env::var_os("SSH_CLIENT").is_some() || env::var_os("SSH_TTY").is_some()) { return Ok(()) } // only start on device startup, not when SSHing in etc
    let config = Config::new().await?;
    let (mut sink, mut stream) = tokio_tungstenite::connect_async(/*"wss://gefolge.org/api/websocket"*/ "ws://192.168.178.59:24802/websocket").await?.0.split(); //DEBUG
    config.api_key.write_ws(&mut sink).await?;
    1u8.write_ws(&mut sink).await?; // session purpose: current event
    let _ /*current_event*/ = loop {
        let packet = Packet::read_ws(&mut stream).await?;
        break match packet {
            Packet::Ping => continue, //TODO send pong
            Packet::Error { display, .. } => return Err(Error::Server { display }),
            Packet::NoEvent => if args.conditional { return Ok(()) } else { None },
            Packet::CurrentEvent(id) => Some(id),
        }
    };
    let (mut ctx, evt_loop) = ContextBuilder::new("sil", "Fenhl")
        .window_setup(WindowSetup {
            title: format!("Gefolge-Silvester-Beamer"),
            ..WindowSetup::default()
        })
        .modules(ModuleConf {
            gamepad: false,
            audio: false,
        })
        .build()?;
    filesystem::mount(&mut ctx, Path::new("/"), true); // for font support
    if let Some(PhysicalSize { width, height }) = supported_resolutions(&ctx).max_by_key(|PhysicalSize { width, height }| width * height) {
        set_mode(&mut ctx, WindowMode {
            width: width as f32,
            height: height as f32,
            fullscreen_type: FullscreenType::True,
            ..WindowMode::default()
        })?;
    } else {
        eprintln!("could not go fullscreen, no supported resolutions");
    }
    let handler = Handler::new(&mut ctx, true)?; //TODO add option to enable light mode
    //TODO also keep polling websocket in separate task
    ggez::event::run(ctx, evt_loop, handler)
}
