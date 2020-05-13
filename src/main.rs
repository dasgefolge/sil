#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use ggez::{
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
};

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

fn main() -> GameResult {
    let (mut ctx, mut evt_loop) = ContextBuilder::new("sil", "Fenhl")
        .window_mode(WindowMode {
            fullscreen_type: FullscreenType::True,
            ..WindowMode::default()
        })
        .build()?;
    let mut handler = Handler::new(&mut ctx, true)?; //TODO add option to enable light mode
    ggez::event::run(&mut ctx, &mut evt_loop, &mut handler)
}
