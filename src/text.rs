use ggez::graphics::{
    Canvas,
    Color,
    DrawParam,
    Drawable as _,
    PxScale,
    Rect,
    Text,
    TextAlign,
    TextFragment,
    TextLayout,
};

pub(crate) const DEJAVU: &str = "DejaVu Sans";

enum Bounds {
    Default,
    //Inner(Rect),
    //Outer(Rect),
}

impl Bounds {
    fn to_inner(&self, canvas: &Canvas, size: f32) -> Rect {
        match self {
            Self::Default => {
                let Rect { x, y, w, h } = canvas.screen_coordinates().expect("canvas has screen coordinates");
                Rect {
                    x: x + size / 2.0,
                    y: y + size / 2.0,
                    w: w - size,
                    h: h - size,
                }
            }
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
    halign: TextAlign,
    valign: TextAlign,
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
    pub(crate) fn halign(mut self, halign: TextAlign) -> Self {
        self.halign = halign;
        self
    }
    */

    #[must_use]
    pub(crate) fn valign(mut self, valign: TextAlign) -> Self {
        self.valign = valign;
        self
    }

    pub(crate) fn draw(mut self, handler: &crate::Handler, canvas: &mut Canvas) {
        let Rect { x, y, w, h } = self.bounds.to_inner(canvas, self.size);
        for fragment in self.text.fragments_mut() {
            fragment.color.get_or_insert(self.color.unwrap_or(handler.fg));
        }
        self.text
            .set_font("DejaVu Sans")
            .set_scale(PxScale::from(self.size))
            .set_bounds([w, h], TextLayout::Wrap { h_align: self.halign, v_align: self.valign })
            .draw(canvas, DrawParam::default().dest([x, y]));
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self {
            text: Text::default(),
            bounds: Bounds::Default,
            color: None,
            size: 100.0,
            halign: TextAlign::Middle,
            valign: TextAlign::Middle,
        }
    }
}
