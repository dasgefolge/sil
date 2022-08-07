use ggez::{
    Context,
    GameResult,
    graphics::{
        self,
        Align as HorizontalAlign,
        Color,
        DrawParam,
        Drawable as _,
        PxScale,
        Rect,
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

enum Bounds {
    Default,
    //Inner(Rect),
    //Outer(Rect),
}

impl Bounds {
    fn to_inner(&self, ctx: &Context, _ /*size*/: f32) -> Rect {
        match self {
            Self::Default => graphics::screen_coordinates(ctx),
            /*
            Self::Inner(rect) => *rect,
            Self::Outer(Rect { x, y, w, h }) => Rect {
                x: x + size / 2.0,
                y: y + size / 2.0,
                w: w - size,
                h: h - size,
            },
            */
        }
    }
}

pub(crate) struct TextBox {
    text: Text,
    bounds: Bounds,
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

    /*
    #[must_use]
    pub(crate) fn bounds_inner(mut self, bounds: Rect) -> Self {
        self.bounds = Bounds::Inner(bounds);
        self
    }

    #[must_use]
    pub(crate) fn bounds_outer(mut self, bounds: Rect) -> Self {
        self.bounds = Bounds::Outer(bounds);
        self
    }
    */

    #[must_use]
    pub(crate) fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    #[must_use]
    pub(crate) fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /*
    #[must_use]
    pub(crate) fn halign(mut self, halign: HorizontalAlign) -> Self {
        self.halign = halign;
        self
    }
    */

    #[must_use]
    pub(crate) fn valign(mut self, valign: VerticalAlign) -> Self {
        self.valign = valign;
        self
    }

    pub(crate) fn draw(mut self, handler: &crate::Handler, ctx: &mut Context) -> GameResult {
        let Rect { x, y, w, h } = self.bounds.to_inner(ctx, self.size);
        for fragment in self.text.fragments_mut() {
            fragment.color.get_or_insert(self.color.unwrap_or(handler.fg));
        }
        self.text.set_font(handler.dejavu_sans, PxScale::from(self.size));
        self.text.set_bounds([w, h], self.halign);
        let mut param = DrawParam::default().dest([
            x,
            // only handle the y coordinate this way, as the horizontal alignment
            // automatically takes care of the x coordinate
            match self.valign {
                VerticalAlign::Top => y + h / 2.0,
                VerticalAlign::Middle => h / 2.0,
                VerticalAlign::Bottom => y + h,
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
            bounds: Bounds::Default,
            color: None,
            size: 100.0,
            halign: HorizontalAlign::Center,
            valign: VerticalAlign::Middle,
        }
    }
}
