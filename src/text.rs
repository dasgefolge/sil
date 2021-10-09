use ggez::{
    Context,
    GameResult,
    graphics::{
        Align as HorizontalAlign,
        Color,
        DrawParam,
        Drawable as _,
        PxScale,
        Text,
        TextFragment,
    },
};

pub(crate) enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

pub(crate) struct TextBox {
    text: Text,
    color: Option<Color>,
    size: f32,
    halign: HorizontalAlign,
    valign: VerticalAlign,
}

impl TextBox {
    pub(crate) fn new(text: impl Into<TextFragment>) -> Self {
        Self {
            text: Text::new(text),
            ..Self::default()
        }
    }

    pub(crate) fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub(crate) fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /*
    pub(crate) fn halign(mut self, halign: HorizontalAlign) -> Self {
        self.halign = halign;
        self
    }
    */

    pub(crate) fn valign(mut self, valign: VerticalAlign) -> Self {
        self.valign = valign;
        self
    }

    pub(crate) fn draw(mut self, handler: &crate::Handler, ctx: &mut Context) -> GameResult {
        for fragment in self.text.fragments_mut() {
            fragment.color.get_or_insert(self.color.unwrap_or(handler.fg));
        }
        self.text
            .set_font(handler.dejavu_sans, PxScale::from(self.size))
            .set_bounds([handler.resolution.width as f32 - self.size, handler.resolution.height as f32 - self.size], self.halign)
            .draw(ctx, DrawParam::default().dest([
                match self.halign {
                    HorizontalAlign::Left => self.size / 2.0,
                    HorizontalAlign::Center => handler.resolution.width as f32 / 2.0,
                    HorizontalAlign::Right => handler.resolution.width as f32 - self.size / 2.0,
                },
                match self.valign {
                    VerticalAlign::Top => self.size / 2.0,
                    VerticalAlign::Middle => handler.resolution.height as f32 / 2.0,
                    VerticalAlign::Bottom => handler.resolution.height as f32 - self.size / 2.0,
                },
            ]).offset([
                match self.halign {
                    HorizontalAlign::Left => 0.0,
                    HorizontalAlign::Center => 0.5,
                    HorizontalAlign::Right => 1.0,
                },
                match self.valign {
                    VerticalAlign::Top => 0.0,
                    VerticalAlign::Middle => 0.5,
                    VerticalAlign::Bottom => 1.0,
                },
            ]))
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self {
            text: Text::default(),
            color: None,
            size: 100.0,
            halign: HorizontalAlign::Center,
            valign: VerticalAlign::Middle,
        }
    }
}
