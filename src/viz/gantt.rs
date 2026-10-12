//! Renders a finished build as a Gantt timeline.
//!
//! To interpret: Each target occupies one row, with a bar from start to finish time. 
//! Bars that overlap ran at the same time:
//!
//! ```text
//! base     ███
//! svc_a       ██████
//! svc_b       ████████
//! release             ███
//!          0s    1s    2s
//! ```
//!
//! Most of this code was generated with Claude, and subsequently, has been heavily
//! reviewed for correctness and readability.

use super::svg::{BoxStyle, Point, Rect, Stroke, Svg, TextStyle, palette};
use crate::exec::{JsonReport, JsonTargetResult};

/// Space between the canvas edge and the timeline, on both sides.
const MARGIN_X: f64 = 16.0;
/// Width the whole build is scaled to fit.
const TIMELINE_WIDTH: f64 = 760.0;
/// Total canvas width.
const CANVAS_WIDTH: f64 = MARGIN_X + TIMELINE_WIDTH + MARGIN_X;
/// Factor applied to the build's duration, so the last bar ends short of the
/// right edge.
const TIMELINE_HEADROOM: f64 = 1.05;

/// Space above the first row, where the title and summary go.
const HEADER_HEIGHT: f64 = 56.0;
/// Baseline of the title, measured from the top of the canvas.
const TITLE_BASELINE: f64 = 24.0;
/// Baseline of the summary, measured from the top of the canvas.
const SUMMARY_BASELINE: f64 = 42.0;

/// Height of each target's row.
const ROW_HEIGHT: f64 = 32.0;
/// Space between a bar and the top and bottom of its row.
const BAR_INSET_Y: f64 = 5.0;
/// Corner radius of each bar.
const BAR_CORNER_RADIUS: f64 = 3.0;
/// Narrowest a bar can be, so instant targets (such as skipped ones) stay visible.
const MIN_BAR_WIDTH: f64 = 2.0;

/// Narrowest bar that fits its own label inside it. Narrower bars get their
/// label to their right instead.
const MIN_BAR_WIDTH_FOR_INLINE_LABEL: f64 = 90.0;
/// Space between a label and the bar edge it sits next to.
const LABEL_GAP: f64 = 6.0;
/// Distance from a bar's vertical center down to its label's baseline, which
/// makes the text look vertically centered.
const LABEL_BASELINE_SHIFT: f64 = 4.0;

/// Space below the last row, where the seconds labels go.
const AXIS_HEIGHT: f64 = 28.0;
/// Baseline of the seconds labels, measured from the bottom of the last row.
const AXIS_LABEL_BASELINE: f64 = 16.0;
/// Space below the time axis.
const BOTTOM_MARGIN: f64 = 12.0;

/// Style of the title line.
const TITLE: TextStyle = TextStyle::new(15, palette::TEXT).bold();
/// Style of the built/skipped/failed summary line.
const SUMMARY: TextStyle = TextStyle::new(12, palette::TEXT_MUTED);
/// Style of the seconds labels.
const AXIS_LABEL: TextStyle = TextStyle::new(10, palette::TEXT_FAINT);
/// Style of a bar's label. Its color switches to [`palette::TEXT_ON_FILL`] when
/// the label sits inside the bar.
const BAR_LABEL: TextStyle = TextStyle::new(11, palette::TEXT);
/// Style of the per-second gridlines.
const GRIDLINE: Stroke = Stroke { color: palette::GRIDLINE, width: 1.0 };

/// Returns the bar color for a target's `status`, as written in the JSON report.
fn status_color(status: &str) -> &'static str {
    match status {
        "built" => "#2F9E44",
        "skipped" => "#94A3B8",
        _ => "#E03131", // "failed" or "signaled"
    }
}

/// Renders `report` as an SVG Gantt timeline.
///
/// Targets are drawn one per row, in the order the report lists them, with bars
/// colored by status.
///
/// # Examples
///
/// ```
/// use bagel::exec::{JsonReport, JsonTargetResult};
/// use bagel::viz::report_to_svg;
///
/// let report = JsonReport {
///     total_duration_ms: 1500.0,
///     built: 1,
///     skipped: 0,
///     failed: 0,
///     targets: vec![JsonTargetResult {
///         name: "app".to_string(),
///         status: "built".to_string(),
///         exit_code: None,
///         start_ms: 0.0,
///         duration_ms: 1500.0,
///         deps: vec![],
///     }],
/// };
///
/// let svg = report_to_svg(&report);
/// assert!(svg.contains("app · 1.50s"));
/// ```
pub fn report_to_svg(report: &JsonReport) -> String {
    let timeline = Timeline::new(report);
    let mut svg = Svg::new(CANVAS_WIDTH, timeline.canvas_height());

    draw_header(&mut svg, report);
    draw_time_axis(&mut svg, &timeline);
    for (row, target) in report.targets.iter().enumerate() {
        draw_bar(&mut svg, &timeline, row, target);
    }

    svg.finish()
}

/// Converts build times (in milliseconds) and row numbers to canvas coordinates.
struct Timeline {
    /// Length of time the timeline covers, including headroom.
    span_ms: f64,
    /// Number of rows, at least 1.
    rows: usize,
}

impl Timeline {
    /// Returns a timeline that fits every target in `report`.
    fn new(report: &JsonReport) -> Self {
        Self {
            span_ms: (report.total_duration_ms * TIMELINE_HEADROOM).max(1.0),
            rows: report.targets.len().max(1),
        }
    }

    /// Returns the x coordinate of the moment `ms` after the build started.
    fn x_at(&self, ms: f64) -> f64 {
        MARGIN_X + self.width_of(ms)
    }

    /// Returns the width of a bar lasting `ms`.
    fn width_of(&self, ms: f64) -> f64 {
        ms / self.span_ms * TIMELINE_WIDTH
    }

    /// Returns the y coordinate of the top of `row`.
    fn row_top(&self, row: usize) -> f64 {
        HEADER_HEIGHT + row as f64 * ROW_HEIGHT
    }

    /// Returns the y coordinate just below the last row.
    fn rows_bottom(&self) -> f64 {
        self.row_top(self.rows)
    }

    /// Returns the canvas height, including the header and time axis.
    fn canvas_height(&self) -> f64 {
        self.rows_bottom() + AXIS_HEIGHT + BOTTOM_MARGIN
    }
}

/// Draws the title (target count and total time) and the built/skipped/failed
/// summary.
fn draw_header(svg: &mut Svg, report: &JsonReport) {
    let title = format!(
        "{} target(s) · {:.2}s total",
        report.targets.len(),
        secs(report.total_duration_ms)
    );
    let summary = format!(
        "built {} · skipped {} · failed {}",
        report.built, report.skipped, report.failed
    );
    svg.text(Point { x: MARGIN_X, y: TITLE_BASELINE }, TITLE, &title);
    svg.text(Point { x: MARGIN_X, y: SUMMARY_BASELINE }, SUMMARY, &summary);
}

/// Draws a labelled gridline at every whole second.
fn draw_time_axis(svg: &mut Svg, timeline: &Timeline) {
    let whole_seconds = secs(timeline.span_ms).floor() as u32;
    let (top, bottom) = (timeline.row_top(0), timeline.rows_bottom());

    for s in 0..=whole_seconds {
        let x = timeline.x_at(s as f64 * 1000.0);
        svg.line(Point { x, y: top }, Point { x, y: bottom }, GRIDLINE);
        svg.text(Point { x, y: bottom + AXIS_LABEL_BASELINE }, AXIS_LABEL, &format!("{s}s"));
    }
}

/// Draws `target`'s bar in `row`, with a "name · duration" label and a hover
/// tooltip.
fn draw_bar(svg: &mut Svg, timeline: &Timeline, row: usize, target: &JsonTargetResult) {
    let bar = Rect {
        x: timeline.x_at(target.start_ms),
        y: timeline.row_top(row) + BAR_INSET_Y,
        width: timeline.width_of(target.duration_ms).max(MIN_BAR_WIDTH),
        height: ROW_HEIGHT - 2.0 * BAR_INSET_Y,
    };
    let style = BoxStyle::filled(status_color(&target.status)).rounded(BAR_CORNER_RADIUS);
    let tooltip = format!(
        "{}: {}, start {:.2}s, duration {:.2}s",
        target.name,
        target.status,
        secs(target.start_ms),
        secs(target.duration_ms),
    );
    svg.rect_with_tooltip(bar, style, &tooltip);

    let label = format!("{} · {:.2}s", target.name, secs(target.duration_ms));
    let baseline = bar.center_y() + LABEL_BASELINE_SHIFT;
    if bar.width > MIN_BAR_WIDTH_FOR_INLINE_LABEL {
        let at = Point { x: bar.x + LABEL_GAP, y: baseline };
        svg.text(at, BAR_LABEL.with_fill(palette::TEXT_ON_FILL), &label);
    } else {
        let at = Point { x: bar.right() + LABEL_GAP, y: baseline };
        svg.text(at, BAR_LABEL, &label);
    }
}

/// Converts milliseconds to seconds.
fn secs(ms: f64) -> f64 {
    ms / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_bar_per_target_and_is_valid_svg_shell() {
        let report = JsonReport {
            total_duration_ms: 5000.0,
            built: 2,
            skipped: 0,
            failed: 0,
            targets: vec![
                JsonTargetResult {
                    name: "base".to_string(),
                    status: "built".to_string(),
                    exit_code: None,
                    start_ms: 0.0,
                    duration_ms: 1000.0,
                    deps: vec![],
                },
                JsonTargetResult {
                    name: "app".to_string(),
                    status: "built".to_string(),
                    exit_code: None,
                    start_ms: 1000.0,
                    duration_ms: 4000.0,
                    deps: vec!["base".to_string()],
                },
            ],
        };

        let svg = report_to_svg(&report);

        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("<rect").count(), 3); // background + 2 bars
        assert!(svg.contains("base"));
        assert!(svg.contains("app"));
    }
}
