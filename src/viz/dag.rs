//! Renders a build spec's dependency graph as a layered diagram.
//!
//! Each target sits in a layer equal to the length of its longest dependency
//! chain. Targets with no dependencies form the bottom row, and arrows point up
//! from each dependency into the targets that need it:
//!
//! ```text
//!         [A]          layer 2
//!        ↗   ↖
//!      [B]   [C]       layer 1
//!        ↖   ↗
//!         [D]          layer 0
//! ```
//!
//! Most of this code was generated with Claude, and subsequently, has been heavily
//! reviewed for correctness and readability.

use super::svg::{BoxStyle, Point, Rect, Stroke, Svg, TextStyle, palette};
use crate::core::{BuildSpec, BuildSpecError, TargetKind};
use std::collections::{BTreeMap, HashMap};

/// Space between the canvas edge and the outermost nodes.
const MARGIN: f64 = 24.0;
/// Height of every node.
const NODE_HEIGHT: f64 = 52.0;
/// Narrowest a node can be, even when every name is short.
const MIN_NODE_WIDTH: f64 = 84.0;
/// Horizontal space around a node's name, split between both sides.
const NODE_PADDING_X: f64 = 28.0;
/// Approximate width of one monospace character at the name's font size. SVG
/// can't measure text for us, so node widths are estimated from this.
const NAME_CHAR_WIDTH: f64 = 7.6;
/// Horizontal space between neighbouring nodes in the same layer.
const NODE_GAP: f64 = 24.0;
/// Vertical space between layers, where the edges run.
const LAYER_GAP: f64 = 70.0;
/// Baseline of the target name, measured from the top of the node.
const NAME_BASELINE: f64 = 22.0;
/// Baseline of the caption, measured from the top of the node.
const CAPTION_BASELINE: f64 = 38.0;

/// Fill and outline of each node.
const NODE: BoxStyle = BoxStyle::filled("#EEF2FF").stroked("#6366F1", 1.3).rounded(8.0);
/// Style of the target name.
const NAME: TextStyle = TextStyle::new(13, palette::TEXT).bold().centered();
/// Style of the "kind · wave N" caption below the name.
const CAPTION: TextStyle = TextStyle::new(10, palette::TEXT_MUTED).centered();
/// Style of the dependency arrows.
const EDGE: Stroke = Stroke { color: palette::EDGE, width: 1.4 };
/// `id` of the arrowhead marker that every arrow refers to.
const ARROWHEAD_ID: &str = "arrow";

/// Renders `spec`'s dependency graph as an SVG document.
///
/// Each target is drawn as a box labelled with its name, its kind and its wave,
/// with an arrow coming in from each of its dependencies.
///
/// # Errors
///
/// Returns an error if `spec` contains a dependency cycle.
///
/// # Examples
///
/// ```
/// use bagel::core::BuildSpec;
/// use bagel::viz::spec_to_svg;
///
/// let spec = BuildSpec::from_toml(
///     r#"
///     [lib]
///     cmd = "cc -c lib.c"
///     inputs = ["lib.c"]
///     outputs = ["lib.o"]
///
///     [app]
///     cmd = "cc lib.o main.c -o app"
///     inputs = ["main.c"]
///     outputs = ["app"]
///     deps = ["lib"]
///     "#,
/// )
/// .unwrap();
///
/// let svg = spec_to_svg(&spec).unwrap();
/// assert!(svg.contains("binary · wave 2")); // app sits one layer above lib
/// ```
pub fn spec_to_svg(spec: &BuildSpec) -> Result<String, BuildSpecError> {
    let layers = compute_layers(spec)?;
    let layout = Layout::new(&layers);
    let mut svg = Svg::new(layout.width, layout.height);

    svg.arrowhead_marker(ARROWHEAD_ID, palette::EDGE);
    // Edges go first so the nodes are drawn over their ends.
    draw_edges(&mut svg, spec, &layout);
    draw_nodes(&mut svg, spec, &layout, &layers);

    Ok(svg.finish())
}

/// Returns each target's layer: 0 if it has no dependencies, otherwise one more
/// than the highest layer among its dependencies.
///
/// A target's layer is also the earliest wave it can run in when building in
/// parallel, which is why the diagram labels it "wave N" (counting from 1).
/// Returns an error if `spec` contains a dependency cycle.
fn compute_layers(spec: &BuildSpec) -> Result<HashMap<String, usize>, BuildSpecError> {
    let mut layers: HashMap<String, usize> = HashMap::new();

    // Dependencies come before their dependents, so their layers are known by
    // the time we reach the dependent.
    for name in spec.topological_sort()? {
        let target = spec
            .get_target(&name)
            .expect("name came from this spec's own topological sort");
        let layer = target
            .deps
            .iter()
            .map(|d| layers.get(d).copied().unwrap_or(0) + 1)
            .max()
            .unwrap_or(0);
        layers.insert(name, layer);
    }

    Ok(layers)
}

/// Position of every node, plus the canvas size that fits them all.
struct Layout<'a> {
    /// Each target's box, keyed by name. A `BTreeMap` keeps iteration, and so
    /// the SVG output, in a stable order.
    nodes: BTreeMap<&'a str, Rect>,
    /// Canvas width, including margins.
    width: f64,
    /// Canvas height, including margins.
    height: f64,
}

impl<'a> Layout<'a> {
    /// Lays out `layers` as centered rows, with layer 0 at the bottom.
    fn new(layers: &'a HashMap<String, usize>) -> Self {
        let rows = group_by_layer(layers);
        let node_width = node_width(layers.keys());
        let widest_row = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let content_width = span(widest_row, node_width, NODE_GAP);

        let mut nodes = BTreeMap::new();
        for (layer, names) in rows.iter().enumerate() {
            // Each row is centered horizontally. Layer 0 (leaves) is the
            // bottom row and higher layers stack upward.
            let row_x = MARGIN + (content_width - span(names.len(), node_width, NODE_GAP)) / 2.0;
            let row_y = MARGIN + (rows.len() - 1 - layer) as f64 * (NODE_HEIGHT + LAYER_GAP);
            for (i, &name) in names.iter().enumerate() {
                let x = row_x + i as f64 * (node_width + NODE_GAP);
                nodes.insert(name, Rect { x, y: row_y, width: node_width, height: NODE_HEIGHT });
            }
        }

        Self {
            nodes,
            width: content_width + 2.0 * MARGIN,
            height: span(rows.len(), NODE_HEIGHT, LAYER_GAP) + 2.0 * MARGIN,
        }
    }
}

/// Returns target names grouped by layer, sorted alphabetically within each layer.
///
/// Always returns at least one row, so an empty spec still gets a blank canvas.
fn group_by_layer(layers: &HashMap<String, usize>) -> Vec<Vec<&str>> {
    let layer_count = layers.values().max().map_or(1, |&deepest| deepest + 1);
    let mut rows = vec![Vec::new(); layer_count];
    for (name, &layer) in layers {
        rows[layer].push(name.as_str());
    }
    for row in &mut rows {
        row.sort_unstable();
    }
    rows
}

/// Returns the width shared by every node: wide enough for the longest name,
/// and never narrower than [`MIN_NODE_WIDTH`].
fn node_width<'a>(names: impl Iterator<Item = &'a String>) -> f64 {
    let longest = names.map(|n| n.chars().count()).max().unwrap_or(0);
    (longest as f64 * NAME_CHAR_WIDTH + NODE_PADDING_X).max(MIN_NODE_WIDTH)
}

/// Returns the total length of `count` items of length `size`, with `gap`
/// between neighbours. For example, three 10-unit nodes with 5-unit gaps span
/// `3 * 10 + 2 * 5 = 40`.
///
/// `count` must be at least 1.
fn span(count: usize, size: f64, gap: f64) -> f64 {
    count as f64 * (size + gap) - gap
}

/// Draws one arrow per dependency, from the top of the dependency into the
/// bottom of the target that needs it.
fn draw_edges(svg: &mut Svg, spec: &BuildSpec, layout: &Layout) {
    for (&name, node) in &layout.nodes {
        for dep in &spec.targets[name].deps {
            // Unresolved deps are rejected at parse time, so this always matches.
            let Some(dep_node) = layout.nodes.get(dep.as_str()) else {
                continue;
            };
            svg.curved_arrow(dep_node.top_center(), node.bottom_center(), EDGE, ARROWHEAD_ID);
        }
    }
}

/// Draws a box for each target, labelled with its name and a "kind · wave N"
/// caption.
fn draw_nodes(svg: &mut Svg, spec: &BuildSpec, layout: &Layout, layers: &HashMap<String, usize>) {
    for (&name, &node) in &layout.nodes {
        let kind = match spec.targets[name].kind {
            TargetKind::Binary => "binary",
            TargetKind::Lib => "lib",
        };
        let wave = layers[name] + 1; // layers count from 0, waves from 1
        let caption = format!("{kind} · wave {wave}");

        svg.rect(node, NODE);
        svg.text(Point { x: node.center_x(), y: node.y + NAME_BASELINE }, NAME, name);
        svg.text(Point { x: node.center_x(), y: node.y + CAPTION_BASELINE }, CAPTION, &caption);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_to_svg_layers_a_diamond_and_marks_the_shared_leaf_wave_one() {
        // A depends on B and C, both depend on D — the same diamond shape
        // exercised by BuildSpec's own topological_sort tests.
        let toml_content = r#"
            [A]
            cmd = "echo A"
            inputs = ["a.txt"]
            outputs = ["a.out"]
            deps = ["B", "C"]

            [B]
            cmd = "echo B"
            inputs = ["b.txt"]
            outputs = ["b.out"]
            deps = ["D"]

            [C]
            cmd = "echo C"
            inputs = ["c.txt"]
            outputs = ["c.out"]
            deps = ["D"]

            [D]
            cmd = "echo D"
            inputs = ["d.txt"]
            outputs = ["d.out"]
        "#;
        let spec = BuildSpec::from_toml(toml_content).unwrap();

        let layers = compute_layers(&spec).unwrap();
        assert_eq!(layers["D"], 0);
        assert_eq!(layers["B"], 1);
        assert_eq!(layers["C"], 1);
        assert_eq!(layers["A"], 2);

        let svg = spec_to_svg(&spec).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("<rect").count(), 5); // background + 4 nodes
        assert_eq!(svg.matches("marker-end").count(), 4); // A-B, A-C, B-D, C-D
        assert!(svg.contains("wave 1")); // D, the shared leaf
        assert!(svg.contains("wave 3")); // A, at the top
    }
}
