//! Static SVG renderings of a build, for docs and READMEs.
//!
//! - [`spec_to_svg`] draws a [`BuildSpec`](crate::core::BuildSpec)'s dependency
//!   graph as a layered diagram.
//! - [`report_to_svg`] draws a [`JsonReport`](crate::exec::JsonReport), as written
//!   by `bagel build --json <path>`, as a Gantt timeline.
//!
//! Both return SVG markup as a string. To extract a PNG, run it through a converter
//! such as `rsvg-convert chart.svg -o chart.png`.
//!
//! Most of this code was generated with Claude, and subsequently, has been heavily
//! reviewed for correctness and readability.

mod dag;
mod gantt;
mod svg;

pub use dag::spec_to_svg;
pub use gantt::report_to_svg;
