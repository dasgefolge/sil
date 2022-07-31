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
        Transform,
        mint,
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
        self.text.set_font(handler.dejavu_sans, PxScale::from(self.size));
        self.text.set_bounds([handler.resolution.width as f32 - self.size, handler.resolution.height as f32 - self.size], self.halign);
        let mut param = DrawParam::default().dest([
            self.size / 2.0,
            // only handle the y coordinate this way, as the horizontal alignment
            // automatically takes care of the x coordinate
            match self.valign {
                VerticalAlign::Top => self.size / 2.0,
                VerticalAlign::Middle => handler.resolution.height as f32 / 2.0,
                VerticalAlign::Bottom => handler.resolution.height as f32 - self.size / 2.0,
            },
        ]).offset([
            0.0,
            // same with the offset
            match self.valign {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Middle => 0.5,
                VerticalAlign::Bottom => 1.0,
            },
        ]);
        if let Transform::Values { offset, .. } = param.trans {
            let dim = self.text.dimensions(ctx);
            let new_offset = mint::Vector2 {
                x: offset.x * dim.w + dim.x,
                y: offset.y * dim.h + dim.y,
            };
            param = param.offset(new_offset);
        }
        self.text.draw(ctx, param)
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
