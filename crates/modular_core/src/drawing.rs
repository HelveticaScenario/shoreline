//! `Drawing` — a line drawing read from an SVG file, flattened into a table a
//! phase can trace.

use std::borrow::Cow;
use std::result::Result as StdResult;
use std::sync::Arc;

use deserr::{DeserializeError, ErrorKind, IntoValue, Map, ValuePointerRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::patch::Patch;
use crate::types::{CableSource, Connect, LaneExpansion, PreviousIds};

/// Points in a traced drawing, spaced evenly along its length.
pub const DRAWING_TABLE_LEN: usize = 8192;

/// Line segments each curve of the drawing is flattened into.
const CURVE_STEPS: usize = 32;

/// Half the side of the square a drawing is fitted into, in volts.
const DRAWING_HALF_SPAN: f32 = 5.0;

/// A drawing's pen positions, evenly spaced along its length and fitted to
/// ±5 V, with y pointing up.
#[derive(Debug)]
struct DrawingTable {
    points: Box<[[f32; 2]]>,
    /// Whether the pen lifts between point `i` and the next one (wrapping),
    /// where the drawing jumps between strokes instead of drawing a line.
    lifts: Box<[bool]>,
}

impl DrawingTable {
    fn empty() -> Self {
        Self {
            points: Box::default(),
            lifts: Box::default(),
        }
    }

    /// Parses `svg` and traces every visible shape in document order, each
    /// stroke followed by the next with the pen lifted between them.
    fn from_svg(svg: &str) -> StdResult<Self, String> {
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
            .map_err(|e| format!("not a readable SVG drawing: {e}"))?;
        let mut strokes: Vec<Vec<[f32; 2]>> = Vec::new();
        collect_strokes(tree.root(), &mut strokes);
        strokes.retain(|s| s.len() > 1);
        Ok(Self::from_strokes(&strokes))
    }

    fn from_strokes(strokes: &[Vec<[f32; 2]>]) -> Self {
        let all = strokes.iter().flatten();
        let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for p in all {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        let extent = (max[0] - min[0]).max(max[1] - min[1]);
        if !extent.is_finite() || extent <= 0.0 {
            return Self::empty();
        }
        let scale = 2.0 * DRAWING_HALF_SPAN / extent;
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let fit = |p: &[f32; 2]| [(p[0] - center[0]) * scale, (center[1] - p[1]) * scale];

        // Cumulative length at the end of each stroke; jumps between strokes
        // take no length, so the pen spends no time on them.
        let mut lengths = Vec::with_capacity(strokes.len());
        let mut total = 0.0f64;
        for stroke in strokes {
            for w in stroke.windows(2) {
                total += f64::from(distance(&w[0], &w[1]));
            }
            lengths.push(total);
        }
        if total <= 0.0 {
            return Self::empty();
        }

        let mut points = Vec::with_capacity(DRAWING_TABLE_LEN);
        let mut stroke_of = Vec::with_capacity(DRAWING_TABLE_LEN);
        let (mut stroke, mut seg, mut seg_start) = (0usize, 0usize, 0.0f64);
        for k in 0..DRAWING_TABLE_LEN {
            let target = total * k as f64 / DRAWING_TABLE_LEN as f64;
            loop {
                let s = &strokes[stroke];
                let seg_len = f64::from(distance(&s[seg], &s[seg + 1]));
                if target <= seg_start + seg_len
                    || stroke + 1 == strokes.len() && seg + 2 == s.len()
                {
                    let t = if seg_len > 0.0 {
                        ((target - seg_start) / seg_len).clamp(0.0, 1.0) as f32
                    } else {
                        0.0
                    };
                    let (a, b) = (fit(&s[seg]), fit(&s[seg + 1]));
                    points.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                    stroke_of.push(stroke);
                    break;
                }
                seg_start += seg_len;
                seg += 1;
                if seg + 1 == s.len() {
                    seg = 0;
                    stroke += 1;
                }
            }
        }

        let first = fit(&strokes[0][0]);
        let last = fit(strokes[strokes.len() - 1]
            .last()
            .expect("strokes hold 2+ points"));
        let closed = distance(&first, &last) <= 1e-4;
        let lifts = (0..DRAWING_TABLE_LEN)
            .map(|i| match stroke_of.get(i + 1) {
                Some(&next) => next != stroke_of[i],
                None => !closed,
            })
            .collect();
        Self {
            points: points.into_boxed_slice(),
            lifts,
        }
    }

    /// The pen position at `phase`, which wraps into 0–1.
    #[inline]
    fn trace(&self, phase: f32) -> [f32; 2] {
        let len = self.points.len();
        if len == 0 {
            return [0.0, 0.0];
        }
        let wrapped = phase - phase.floor();
        let pos = if wrapped.is_finite() {
            wrapped * len as f32
        } else {
            0.0
        };
        let i = (pos as usize).min(len - 1);
        let a = self.points[i];
        if self.lifts[i] {
            return a;
        }
        let b = self.points[(i + 1) % len];
        let t = pos - i as f32;
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}

fn distance(a: &[f32; 2], b: &[f32; 2]) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn collect_strokes(group: &usvg::Group, strokes: &mut Vec<Vec<[f32; 2]>>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect_strokes(g, strokes),
            usvg::Node::Path(path) => flatten_path(path, strokes),
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

fn flatten_path(path: &usvg::Path, strokes: &mut Vec<Vec<[f32; 2]>>) {
    use usvg::tiny_skia_path::PathSegment;
    let transform = path.abs_transform();
    let map = |p: usvg::tiny_skia_path::Point| {
        let mut q = p;
        transform.map_point(&mut q);
        [q.x, q.y]
    };
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut start = [0.0f32; 2];
    for segment in path.data().segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if current.len() > 1 {
                    strokes.push(std::mem::take(&mut current));
                }
                current.clear();
                start = map(p);
                current.push(start);
            }
            PathSegment::LineTo(p) => current.push(map(p)),
            PathSegment::QuadTo(c, p) => {
                let p0 = *current.last().unwrap_or(&start);
                let (c, p) = (map(c), map(p));
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    current.push(std::array::from_fn(|k| {
                        u * u * p0[k] + 2.0 * u * t * c[k] + t * t * p[k]
                    }));
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let p0 = *current.last().unwrap_or(&start);
                let (c1, c2, p) = (map(c1), map(c2), map(p));
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    current.push(std::array::from_fn(|k| {
                        u * u * u * p0[k]
                            + 3.0 * u * u * t * c1[k]
                            + 3.0 * u * t * t * c2[k]
                            + t * t * t * p[k]
                    }));
                }
            }
            PathSegment::Close => current.push(start),
        }
    }
    if current.len() > 1 {
        strokes.push(current);
    }
}

/// A line drawing from an SVG file. The DSL reads the file and sends its text,
/// which is traced here, on the main thread, as the params deserialize; the
/// audio thread only reads the finished table.
#[derive(Clone)]
pub struct Drawing {
    path: String,
    svg: Arc<str>,
    table: Arc<DrawingTable>,
}

impl Drawing {
    pub fn from_svg(path: String, svg: &str) -> StdResult<Self, String> {
        Ok(Self {
            path,
            svg: Arc::from(svg),
            table: Arc::new(DrawingTable::from_svg(svg)?),
        })
    }

    /// The pen position, in volts, at `phase`, which wraps into 0–1. A drawing
    /// with nothing visible holds the pen at the center.
    #[inline]
    pub fn trace(&self, phase: f32) -> [f32; 2] {
        self.table.trace(phase)
    }
}

impl Connect for Drawing {
    fn apply_default_connections(&mut self) {}
    fn connect(&mut self, _patch: &Patch) {}
    fn collect_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn collect_lane_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn inject_index_ptr(&mut self, _ptr: *const std::cell::Cell<usize>) {}
    fn expand_lanes(&mut self, _expansion: &LaneExpansion) {}
    fn mark_glides(&mut self, _old: Option<&Self>, _generation: u64, _ids: &dyn PreviousIds) {}
}

impl std::fmt::Debug for Drawing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drawing")
            .field("path", &self.path)
            .field("points", &self.table.points.len())
            .finish()
    }
}

/// The file's text takes part, so an edited file is a new drawing.
impl PartialEq for Drawing {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.svg == other.svg
    }
}

#[derive(Serialize, Deserialize)]
struct DrawingSerde {
    path: String,
    svg: String,
}

impl Serialize for Drawing {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> StdResult<S::Ok, S::Error> {
        DrawingSerde {
            path: self.path.clone(),
            svg: self.svg.to_string(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Drawing {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> StdResult<Self, D::Error> {
        let data = DrawingSerde::deserialize(deserializer)?;
        Drawing::from_svg(data.path, &data.svg).map_err(serde::de::Error::custom)
    }
}

impl<E: DeserializeError> deserr::Deserr<E> for Drawing {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> StdResult<Self, E> {
        let fail = |msg: String| {
            deserr::take_cf_content(E::error::<V>(None, ErrorKind::Unexpected { msg }, location))
        };
        let deserr::Value::Map(mut map) = value else {
            return Err(fail("Invalid drawing format".to_string()));
        };
        let mut string_field = |name: &'static str| {
            map.remove(name).and_then(|v| match v.into_value() {
                deserr::Value::String(s) => Some(s),
                _ => None,
            })
        };
        let path = string_field("path");
        let svg = string_field("svg");
        let (Some(path), Some(svg)) = (path, svg) else {
            return Err(fail("a drawing needs its path and svg text".to_string()));
        };
        Drawing::from_svg(path.clone(), &svg).map_err(|e| fail(format!("{path}: {e}")))
    }
}

impl JsonSchema for Drawing {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Drawing")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "type": { "type": "string", "const": "drawing_ref" },
                "path": { "type": "string" },
                "svg": { "type": "string" }
            },
            "required": ["type", "path", "svg"]
        }))
        .expect("Drawing schema is valid JSON Schema")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"#)
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        distance(&a, &b) < 0.01
    }

    #[test]
    fn a_square_fits_five_volts_with_y_up() {
        let d = Drawing::from_svg(
            "sq".into(),
            &svg(r#"<path d="M10 10 H90 V90 H10 Z" stroke="black" fill="none"/>"#),
        )
        .unwrap();
        assert!(close(d.trace(0.0), [-5.0, 5.0]));
        assert!(close(d.trace(0.25), [5.0, 5.0]));
        assert!(close(d.trace(0.5), [5.0, -5.0]));
        assert!(close(d.trace(0.75), [-5.0, -5.0]));
        assert!(close(d.trace(1.0), d.trace(0.0)));
        assert!(close(d.trace(-0.75), d.trace(0.25)));
    }

    #[test]
    fn the_pen_jumps_between_strokes_without_drawing() {
        let d = Drawing::from_svg(
            "two".into(),
            &svg(r#"<path d="M0 0 H100 M0 100 H100" stroke="black"/>"#),
        )
        .unwrap();
        // Each stroke takes half the phase, and the jump between them none.
        assert!(close(d.trace(0.25), [0.0, 5.0]));
        assert!(close(d.trace(0.75), [0.0, -5.0]));
        let just_before = d.trace(0.5 - 0.5 / DRAWING_TABLE_LEN as f32);
        assert!(just_before[1] > 4.99, "{just_before:?}");
        // Phase wraps from the end of the second stroke to the start of the first.
        let end = d.trace(1.0 - 0.1 / DRAWING_TABLE_LEN as f32);
        assert!(end[1] < -4.99, "{end:?}");
    }

    #[test]
    fn transforms_and_shapes_are_traced() {
        let d = Drawing::from_svg(
            "circle".into(),
            &svg(r#"<g transform="translate(50 50)"><circle r="40" stroke="black" fill="none"/></g>"#),
        )
        .unwrap();
        for i in 0..16 {
            let p = d.trace(i as f32 / 16.0);
            assert!((p[0].hypot(p[1]) - 5.0).abs() < 0.02, "{p:?}");
        }
    }

    #[test]
    fn an_empty_drawing_holds_the_center() {
        let d = Drawing::from_svg("empty".into(), &svg("")).unwrap();
        assert_eq!(d.trace(0.3), [0.0, 0.0]);
    }

    #[test]
    fn text_that_is_not_svg_is_an_error() {
        assert!(Drawing::from_svg("bad".into(), "hello").is_err());
    }
}
