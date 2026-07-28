/// BlissGeo Plotter
///
/// Converts BlissGeo geometry nodes (plot, parametric, polar, spiral,
/// regularPolygon, bezier) into SVG path strings using the math evaluator.

use super::math::{eval, eval_with_map};
use crate::compiler::ast::{AttrList, AttrListExt};
use std::collections::HashMap;
use std::f64::consts::PI;

// ─── SVG Point ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self { Self { x, y } }
}

// ─── Plot range parser ────────────────────────────────────────────────────────

/// Parse "0 to 2*PI" or "-5 to 5" into (start, end)
fn parse_range(range: &str) -> (f64, f64) {
    let parts: Vec<&str> = range.split(" to ").collect();
    if parts.len() != 2 { return (0.0, 1.0); }
    let start = eval(parts[0].trim(), &[]).unwrap_or(0.0);
    let end   = eval(parts[1].trim(), &[]).unwrap_or(1.0);
    (start, end)
}

/// Parse "cx cy" into (f64, f64)
fn parse_pair(s: &str) -> (f64, f64) {
    let parts: Vec<f64> = s.split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    (
        parts.first().copied().unwrap_or(0.0),
        parts.get(1).copied().unwrap_or(0.0),
    )
}

/// Convert a Vec<Point> into an SVG path d= string
fn points_to_path(points: &[Point]) -> String {
    if points.is_empty() { return String::new(); }
    let mut path = format!("M {:.3} {:.3}", points[0].x, points[0].y);
    for p in &points[1..] {
        path.push_str(&format!(" L {:.3} {:.3}", p.x, p.y));
    }
    path
}

/// Wrap path points in a full SVG <path> element
fn svg_path(d: &str, attrs: &AttrList) -> String {
    let color  = attrs.get_str("color").unwrap_or("#000");
    let width  = attrs.get_num("width").unwrap_or(2.0);
    let dash   = attrs.get_str("dash").unwrap_or("");
    let dash_attr = if dash.is_empty() { String::new() }
                    else { format!(" stroke-dasharray=\"{}\"", dash) };
    format!(
        r#"  <path d="{}" fill="none" stroke="{}" stroke-width="{}"{} />"#,
        d, color, width, dash_attr
    )
}

// ─── Geo Shape Renderers ──────────────────────────────────────────────────────

/// plot[fn: "sin(x)", x.range: "0 to 2*PI", y.scale: 100, x.scale: 80, origin: "0 200"]
pub fn render_plot(attrs: &AttrList) -> String {
    let fn_expr  = attrs.get_str("fn").unwrap_or("x");
    let x_range  = attrs.get_str("x.range").unwrap_or("0 to 1");
    let y_scale  = attrs.get_num("y.scale").unwrap_or(100.0);
    let x_scale  = attrs.get_num("x.scale").unwrap_or(100.0);
    let steps    = attrs.get_num("steps").unwrap_or(200.0) as usize;
    let origin   = attrs.get_str("origin").unwrap_or("0 0");

    let (ox, oy) = parse_pair(origin);
    let (x_start, x_end) = parse_range(x_range);
    let step_size = (x_end - x_start) / steps as f64;

    let points: Vec<Point> = (0..=steps)
        .filter_map(|i| {
            let x = x_start + i as f64 * step_size;
            let y = eval(fn_expr, &[("x", x), ("t", x)]).ok()?;
            Some(Point::new(
                ox + x * x_scale,
                oy - y * y_scale,
            ))
        })
        .collect();

    svg_path(&points_to_path(&points), attrs)
}

/// parametric[x.fn: "cos(t)*r", y.fn: "sin(t)*r", t.range: "0 to 2*PI", t.steps: 300, origin: "200 200"]
pub fn render_parametric(attrs: &AttrList) -> String {
    let x_fn   = attrs.get_str("x.fn").unwrap_or("cos(t)");
    let y_fn   = attrs.get_str("y.fn").unwrap_or("sin(t)");
    let t_range= attrs.get_str("t.range").unwrap_or("0 to 6.283");
    let steps  = attrs.get_num("t.steps").unwrap_or(300.0) as usize;
    let origin = attrs.get_str("origin").unwrap_or("0 0");

    let (ox, oy) = parse_pair(origin);
    let (t_start, t_end) = parse_range(t_range);
    let step = (t_end - t_start) / steps as f64;

    let mut vars = HashMap::new();
    let points: Vec<Point> = (0..=steps)
        .filter_map(|i| {
            let t = t_start + i as f64 * step;
            vars.insert("t".to_string(), t);
            vars.insert("PI".to_string(), PI);
            let x = eval_with_map(x_fn, &vars).ok()?;
            let y = eval_with_map(y_fn, &vars).ok()?;
            Some(Point::new(ox + x, oy - y))
        })
        .collect();

    svg_path(&points_to_path(&points), attrs)
}

/// polar[fn: "cos(4*theta)", scale: 120, origin: "300 200"]
pub fn render_polar(attrs: &AttrList) -> String {
    let fn_expr = attrs.get_str("fn").unwrap_or("1");
    let scale   = attrs.get_num("scale").unwrap_or(100.0);
    let origin  = attrs.get_str("origin").unwrap_or("0 0");
    let steps   = attrs.get_num("steps").unwrap_or(360.0) as usize;

    let (ox, oy) = parse_pair(origin);
    let step = 2.0 * PI / steps as f64;

    let points: Vec<Point> = (0..=steps)
        .filter_map(|i| {
            let theta = i as f64 * step;
            let r = eval(fn_expr, &[("theta", theta), ("t", theta), ("PI", PI)]).ok()?;
            let r = r.abs() * scale;
            Some(Point::new(
                ox + r * theta.cos(),
                oy - r * theta.sin(),
            ))
        })
        .collect();

    svg_path(&points_to_path(&points), attrs)
}

/// spiral[type: "archimedean", a: 5, b: 5, turns: 6, origin: "200 200"]
pub fn render_spiral(attrs: &AttrList) -> String {
    let kind  = attrs.get_str("type").unwrap_or("archimedean");
    let a     = attrs.get_num("a").unwrap_or(1.0);
    let b     = attrs.get_num("b").unwrap_or(5.0);
    let turns = attrs.get_num("turns").unwrap_or(3.0);
    let steps = attrs.get_num("steps").unwrap_or(500.0) as usize;
    let origin= attrs.get_str("origin").unwrap_or("0 0");

    let (ox, oy) = parse_pair(origin);
    let total_angle = turns * 2.0 * PI;
    let step = total_angle / steps as f64;

    let points: Vec<Point> = (0..=steps)
        .map(|i| {
            let theta = i as f64 * step;
            let r = match kind {
                "archimedean" => a + b * theta,
                "logarithmic" => a * (b * theta).exp(),
                "fermat"      => a * theta.sqrt(),
                _             => a + b * theta,
            };
            Point::new(
                ox + r * theta.cos(),
                oy - r * theta.sin(),
            )
        })
        .collect();

    svg_path(&points_to_path(&points), attrs)
}

/// bezier[start: "50 350", ctrl1: "150 50", ctrl2: "450 50", end: "550 350"]
pub fn render_bezier(attrs: &AttrList) -> String {
    let start = attrs.get_str("start").unwrap_or("0 0");
    let ctrl1 = attrs.get_str("ctrl1").unwrap_or("100 0");
    let ctrl2 = attrs.get_str("ctrl2").unwrap_or("200 0");
    let end   = attrs.get_str("end").unwrap_or("300 0");

    let (x0, y0) = parse_pair(start);
    let (x1, y1) = parse_pair(ctrl1);
    let (x2, y2) = parse_pair(ctrl2);
    let (x3, y3) = parse_pair(end);

    let color = attrs.get_str("color").unwrap_or("#000");
    let width = attrs.get_num("width").unwrap_or(2.0);

    format!(
        r#"  <path d="M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}" fill="none" stroke="{}" stroke-width="{}" />"#,
        x0, y0, x1, y1, x2, y2, x3, y3, color, width
    )
}

/// regularPolygon[center: "200 200", radius: 100, sides: 6, fill: "none", border.color: "#333"]
pub fn render_regular_polygon(attrs: &AttrList) -> String {
    let center = attrs.get_str("center").unwrap_or("0 0");
    let radius = attrs.get_num("radius").unwrap_or(50.0);
    let sides  = attrs.get_num("sides").unwrap_or(6.0) as usize;
    let fill   = attrs.get_str("fill").unwrap_or("none");
    let stroke = attrs.get_str("border.color").unwrap_or("#000");
    let sw     = attrs.get_num("border.width").unwrap_or(1.0);

    let (cx, cy) = parse_pair(center);
    let angle_offset = -PI / 2.0; // start from top

    let points: Vec<String> = (0..sides)
        .map(|i| {
            let angle = angle_offset + 2.0 * PI * i as f64 / sides as f64;
            format!("{:.3},{:.3}", cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect();

    format!(
        r#"  <polygon points="{}" fill="{}" stroke="{}" stroke-width="{}" />"#,
        points.join(" "), fill, stroke, sw
    )
}

/// vertex_point(cx, cy, r, i, n) — returns (x, y) of nth vertex
#[allow(dead_code)]
pub fn vertex_point(cx: f64, cy: f64, r: f64, i: usize, n: usize) -> (f64, f64) {
    let angle = -PI / 2.0 + 2.0 * PI * i as f64 / n as f64;
    (cx + r * angle.cos(), cy + r * angle.sin())
}

/// Evaluate a geo expression that may reference canvas variables.
/// Used for repeat[] body coordinates and var declarations.
#[allow(dead_code)]
pub fn eval_geo_expr(expr: &str, vars: &HashMap<String, f64>) -> f64 {
    eval_with_map(expr, vars).unwrap_or(0.0)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ast::Attr;

    fn attrs_from(pairs: &[(&str, &str)]) -> AttrList {
        pairs.iter().map(|(k, v)| Attr {
            key:   k.split('.').map(str::to_string).collect(),
            value: crate::compiler::ast::AttrValue::Str(v.to_string()),
        }).collect()
    }

    fn attrs_num(pairs: &[(&str, f64)]) -> AttrList {
        pairs.iter().map(|(k, v)| Attr {
            key:   k.split('.').map(str::to_string).collect(),
            value: crate::compiler::ast::AttrValue::Number(*v),
        }).collect()
    }

    #[test]
    fn test_plot_sine_returns_svg_path() {
        let mut a = attrs_from(&[
            ("fn", "sin(x)"),
            ("x.range", "0 to 6.283"),
            ("origin", "0 200"),
            ("color", "#E94560"),
        ]);
        a.push(Attr { key: vec!["y.scale".to_string()], value: crate::compiler::ast::AttrValue::Number(100.0) });
        a.push(Attr { key: vec!["x.scale".to_string()], value: crate::compiler::ast::AttrValue::Number(80.0) });
        a.push(Attr { key: vec!["steps".to_string()],   value: crate::compiler::ast::AttrValue::Number(10.0) });
        let svg = render_plot(&a);
        assert!(svg.contains("<path"), "Expected <path in: {}", svg);
        assert!(svg.contains("M "), "Expected SVG M command");
        assert!(svg.contains("fill=\"none\""));
    }

    #[test]
    fn test_regular_polygon_hexagon() {
        let mut a = attrs_from(&[("center", "100 100"), ("fill", "none"), ("border.color", "#333")]);
        a.push(Attr { key: vec!["radius".to_string()], value: crate::compiler::ast::AttrValue::Number(50.0) });
        a.push(Attr { key: vec!["sides".to_string()],  value: crate::compiler::ast::AttrValue::Number(6.0) });
        let svg = render_regular_polygon(&a);
        assert!(svg.contains("<polygon"), "Expected <polygon");
        assert!(svg.contains("points="), "Expected points=");
        // A hexagon has 6 points — count commas in points string
        let points_start = svg.find("points=\"").unwrap() + 8;
        let points_end   = svg[points_start..].find('"').unwrap() + points_start;
        let points_str   = &svg[points_start..points_end];
        let point_count  = points_str.split(' ').count();
        assert_eq!(point_count, 6, "Hexagon should have 6 points, got {}", point_count);
    }

    #[test]
    fn test_bezier_output() {
        let a = attrs_from(&[
            ("start", "50 350"), ("ctrl1", "150 50"),
            ("ctrl2", "450 50"), ("end", "550 350"), ("color", "#000"),
        ]);
        let svg = render_bezier(&a);
        assert!(svg.contains("C "), "Expected cubic bezier C command");
    }

    #[test]
    fn test_vertex_point() {
        let (x, y) = vertex_point(200.0, 200.0, 100.0, 0, 4);
        // Top vertex of a square should be at (200, 100)
        assert!((x - 200.0).abs() < 0.01, "x should be ~200, got {}", x);
        assert!((y - 100.0).abs() < 0.01, "y should be ~100, got {}", y);
    }

    #[test]
    fn test_parametric_circle() {
        let mut a = attrs_from(&[
            ("x.fn", "100*cos(t)"), ("y.fn", "100*sin(t)"),
            ("t.range", "0 to 6.283"), ("origin", "200 200"),
            ("color", "gold"),
        ]);
        a.push(Attr { key: vec!["t.steps".to_string()], value: crate::compiler::ast::AttrValue::Number(8.0) });
        let svg = render_parametric(&a);
        assert!(svg.contains("<path"), "Expected <path");
        assert!(svg.contains("M "),    "Expected move command");
    }

    #[test]
    fn test_spiral_generates_path() {
        let mut a = attrs_from(&[("type", "archimedean"), ("origin", "200 200"), ("color", "#0F3460")]);
        a.push(Attr { key: vec!["a".to_string()],     value: crate::compiler::ast::AttrValue::Number(5.0) });
        a.push(Attr { key: vec!["b".to_string()],     value: crate::compiler::ast::AttrValue::Number(5.0) });
        a.push(Attr { key: vec!["turns".to_string()], value: crate::compiler::ast::AttrValue::Number(2.0) });
        a.push(Attr { key: vec!["steps".to_string()], value: crate::compiler::ast::AttrValue::Number(20.0) });
        let svg = render_spiral(&a);
        assert!(svg.contains("<path"), "Expected <path");
    }
}
