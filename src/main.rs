#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        env,
        time::Duration
    },
    chrono::prelude::*,
    ggez::{
        Context,
        ContextBuilder,
        GameResult,
        conf::{
            FullscreenType,
            WindowMode
        },
        event::EventHandler,
        graphics::{
            self,
            BLACK,
            Color,
            //Font,
            Rect,
            WHITE
        },
        input::mouse,
        timer
    },
    crate::{
        config::Config,
        event::Event
    }
};

mod config;
mod event;

struct Handler {
    bg: Color,
    //dejavu_sans: Font,
    //fg: Color,
    init: bool
}

impl Handler {
    fn new(_: &mut Context, dark: bool) -> GameResult<Handler> {
        Ok(Handler {
            bg: if dark { BLACK } else { WHITE },
            //dejavu_sans: Font::new(ctx, "/fonts/dejavu/DejaVuSans.ttf")?,
            //fg: if dark { WHITE } else { BLACK },
            init: false
        })
    }
}

impl EventHandler for Handler {
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
        graphics::present(ctx)?;
        timer::yield_now();
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let config = Config::new().expect("failed to read config");
    let client = reqwest::Client::builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(600))
        .build().expect("failed to build HTTP client");
    if env::args().skip(1).any(|arg| arg == "--conditional") {
        let current_event = match Event::current(&config, &client).await.expect("failed to get current event") {
            Some(event) => event,
            None => return
        };
        if current_event.start().single().map_or(false, |start| Utc::now() < start) || current_event.end().single().map_or(false, |end| end <= Utc::now()) {
            return
        }
    }
    let (mut ctx, mut evt_loop) = ContextBuilder::new("sil", "Fenhl")
        .window_mode(WindowMode {
            fullscreen_type: FullscreenType::True,
            ..WindowMode::default()
        })
        .build().expect("failed to build ggez context");
    let mut handler = Handler::new(&mut ctx, true).expect("failed to build ggez handler"); //TODO add option to enable light mode
    ggez::event::run(&mut ctx, &mut evt_loop, &mut handler).expect("error in main loop");
}
