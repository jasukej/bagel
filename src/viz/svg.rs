//! A minimal SVG writer with only the elements the charts need.
//!
//! Charts describe shapes in canvas units through [`Svg`], which handles number
//! formatting and escaping:
//!
//! ```ignore
//! let mut svg = Svg::new(200.0, 100.0);
//! let card = Rect { x: 10.0, y: 10.0, width: 80.0, height: 30.0 };
//! svg.rect(card, BoxStyle::filled("#EEF2FF").rounded(4.0));
//! svg.text(card.top_center(), TextStyle::new(12, palette::TEXT).centered(), "hello");
//! let markup: String = svg.finish();
//! ```
//!
//! Most of this code was generated with Claude, and subsequently, has been heavily
//! reviewed for correctness and readability.

/// Font stack for all text. Monospace, so text width can be estimated from its
/// length.
const FONT_FAMILY: &str = "ui-monospace,Menlo,Consolas,monospace";

/// Colors shared by both charts.
pub mod palette {
    /// Canvas background.
    pub const BACKGROUND: &str = "#ffffff";
    /// Primary text.
    pub const TEXT: &str = "#1a1a1a";
    /// Secondary text, such as captions and summaries.
    pub const TEXT_MUTED: &str = "#6b7280";
    /// Least prominent text, such as axis labels.
    pub const TEXT_FAINT: &str = "#9ca3af";
    /// Text drawn on top of a saturated fill.
    pub const TEXT_ON_FILL: &str = "#ffffff";
    /// Timeline gridlines.
    pub const GRIDLINE: &str = "#e5e7eb";
    /// Dependency arrows.
    pub const EDGE: &str = "#9ca3af";
}

/// A position on the canvas, in canvas units. `y` grows downward.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle, positioned by its top-left corner.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Returns the x coordinate of the horizontal center.
    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Returns the y coordinate of the vertical center.
    pub fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }

    /// Returns the x coordinate of the right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Returns the midpoint of the top edge.
    pub fn top_center(&self) -> Point {
        Point { x: self.center_x(), y: self.y }
    }

    /// Returns the midpoint of the bottom edge.
    pub fn bottom_center(&self) -> Point {
        Point { x: self.center_x(), y: self.y + self.height }
    }
}

/// Color and width of a line or outline.
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub color: &'static str,
    pub width: f64,
}

/// Fill, outline and corner radius of a rectangle.
///
/// Built by chaining from [`BoxStyle::filled`], e.g.
/// `BoxStyle::filled("#EEF2FF").stroked("#6366F1", 1.3).rounded(8.0)`.
#[derive(Debug, Clone, Copy)]
pub struct BoxStyle {
    fill: &'static str,
    stroke: Option<Stroke>,
    corner_radius: f64,
}

impl BoxStyle {
    /// Returns a style filled with `fill`, with no outline and square corners.
    pub const fn filled(fill: &'static str) -> Self {
        Self { fill, stroke: None, corner_radius: 0.0 }
    }

    /// Adds an outline in `color`, `width` canvas units wide.
    pub const fn stroked(self, color: &'static str, width: f64) -> Self {
        Self { stroke: Some(Stroke { color, width }), ..self }
    }

    /// Rounds the corners to `corner_radius`.
    pub const fn rounded(self, corner_radius: f64) -> Self {
        Self { corner_radius, ..self }
    }
}

/// Which point of the text its position refers to.
#[derive(Debug, Clone, Copy)]
enum Anchor {
    /// The start (left edge) of the text.
    Start,
    /// The horizontal center of the text.
    Middle,
}

/// Size, color, weight and anchoring of text.
///
/// Built by chaining from [`TextStyle::new`], e.g.
/// `TextStyle::new(13, palette::TEXT).bold().centered()`.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    size: u32,
    fill: &'static str,
    bold: bool,
    anchor: Anchor,
}

impl TextStyle {
    /// Returns a regular-weight style of `size` in `fill`, anchored at its start.
    pub const fn new(size: u32, fill: &'static str) -> Self {
        Self { size, fill, bold: false, anchor: Anchor::Start }
    }

    /// Makes the text bold.
    pub const fn bold(self) -> Self {
        Self { bold: true, ..self }
    }

    /// Anchors the text at its horizontal center instead of its start.
    pub const fn centered(self) -> Self {
        Self { anchor: Anchor::Middle, ..self }
    }

    /// Changes the text color to `fill`.
    pub const fn with_fill(self, fill: &'static str) -> Self {
        Self { fill, ..self }
    }
}

/// An SVG document, built up one element at a time.
///
/// [`Svg::new`] starts the document with a solid background, and [`Svg::finish`]
/// closes it and returns the markup.
pub struct Svg {
    out: String,
}

impl Svg {
    /// Starts a `width` × `height` document filled with [`palette::BACKGROUND`].
    /// Both sizes are rounded up to whole canvas units.
    pub fn new(width: f64, height: f64) -> Self {
        // Whole units keep the viewBox tidy; rounding up never clips content.
        let (width, height) = (width.ceil(), height.ceil());
        let mut svg = Self {
            out: format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.0} {height:.0}\" \
                 font-family=\"{FONT_FAMILY}\">\n"
            ),
        };
        let canvas = Rect { x: 0.0, y: 0.0, width, height };
        svg.rect(canvas, BoxStyle::filled(palette::BACKGROUND));
        svg
    }

    /// Closes the document and returns its markup.
    pub fn finish(mut self) -> String {
        self.out.push_str("</svg>\n");
        self.out
    }

    /// Draws `rect` in `style`.
    pub fn rect(&mut self, rect: Rect, style: BoxStyle) {
        let attrs = rect_attrs(rect, style);
        self.element(&format!("<rect {attrs}/>"));
    }

    /// Draws `rect` in `style`, showing `tooltip` on hover.
    pub fn rect_with_tooltip(&mut self, rect: Rect, style: BoxStyle, tooltip: &str) {
        let attrs = rect_attrs(rect, style);
        let tooltip = escape(tooltip);
        self.element(&format!("<rect {attrs}><title>{tooltip}</title></rect>"));
    }

    /// Draws `content` in `style`, with its baseline at `at`.
    ///
    /// `at` marks the start of the text, or its center if the style is
    /// [`TextStyle::centered`].
    pub fn text(&mut self, at: Point, style: TextStyle, content: &str) {
        let Point { x, y } = at;
        let TextStyle { size, fill, bold, anchor } = style;
        let weight = if bold { " font-weight=\"700\"" } else { "" };
        let anchor = match anchor {
            Anchor::Start => "",
            Anchor::Middle => " text-anchor=\"middle\"",
        };
        let content = escape(content);
        self.element(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"{size}\"{weight} fill=\"{fill}\"{anchor}>{content}</text>"
        ));
    }

    /// Draws a straight line from `from` to `to`.
    pub fn line(&mut self, from: Point, to: Point, stroke: Stroke) {
        let (Point { x: x1, y: y1 }, Point { x: x2, y: y2 }) = (from, to);
        let Stroke { color, width } = stroke;
        self.element(&format!(
            "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{color}\" stroke-width=\"{width}\"/>"
        ));
    }

    /// Defines an arrowhead marker that [`Svg::curved_arrow`] can refer to by `id`.
    ///
    /// The arrowhead is a triangle in a 10×10 box with its tip at (10, 5), drawn
    /// [`ARROWHEAD_SIZE`] canvas units wide at the end of each arrow.
    pub fn arrowhead_marker(&mut self, id: &str, color: &str) {
        self.element(&format!(
            "<defs><marker id=\"{id}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" \
             markerWidth=\"{ARROWHEAD_SIZE}\" markerHeight=\"{ARROWHEAD_SIZE}\" orient=\"auto-start-reverse\">\
             <path d=\"M0,0 L10,5 L0,10 z\" fill=\"{color}\"/></marker></defs>"
        ));
    }

    /// Draws an S-shaped arrow from `from` to `to`, ending in the marker
    /// `marker_id`. The curve leaves `from` and enters `to` vertically.
    pub fn curved_arrow(&mut self, from: Point, to: Point, stroke: Stroke, marker_id: &str) {
        let (Point { x: x1, y: y1 }, Point { x: x2, y: y2 }) = (from, to);
        let mid_y = (y1 + y2) / 2.0;
        let Stroke { color, width } = stroke;
        self.element(&format!(
            "<path d=\"M{x1:.1},{y1:.1} C{x1:.1},{mid_y:.1} {x2:.1},{mid_y:.1} {x2:.1},{y2:.1}\" \
             stroke=\"{color}\" stroke-width=\"{width}\" fill=\"none\" marker-end=\"url(#{marker_id})\"/>"
        ));
    }

    /// Appends `markup` as one indented line.
    fn element(&mut self, markup: &str) {
        self.out.push_str("  ");
        self.out.push_str(markup);
        self.out.push('\n');
    }
}

/// Size of an arrowhead, in canvas units.
const ARROWHEAD_SIZE: f64 = 6.0;

/// Returns the attributes of a `<rect>`: position, size, corner radius, fill and
/// outline.
fn rect_attrs(rect: Rect, style: BoxStyle) -> String {
    let Rect { x, y, width, height } = rect;
    let BoxStyle { fill, stroke, corner_radius } = style;
    let mut attrs =
        format!("x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width:.1}\" height=\"{height:.1}\"");
    if corner_radius > 0.0 {
        attrs.push_str(&format!(" rx=\"{corner_radius}\""));
    }
    attrs.push_str(&format!(" fill=\"{fill}\""));
    if let Some(Stroke { color, width }) = stroke {
        attrs.push_str(&format!(" stroke=\"{color}\" stroke-width=\"{width}\""));
    }
    attrs
}

/// Escapes the characters that are special in SVG text and attribute values.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
