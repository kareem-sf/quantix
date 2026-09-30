//! Plane geometry for takeoff: where a drawing's objects lie in the space they are drawn in, and their exact
//! lengths and areas. Straight lines, arcs, circles and bulged polylines are measured exactly by opencadkernel;
//! ellipses and splines from a fine chain of points. Points are kept only to draw, pick and compare objects.

use std::collections::HashMap;
use std::f64::consts::TAU;

use opencadcodec::types::{Matrix3, Vector3};
use opencadkernel::geom2d::{Arc, Circle, Curve, NurbsCurve, Polyline, PolylineVertex};

/// How far a chord may leave its arc, as a share of the radius (about 70 chords to a circle).
const SAG: f64 = 0.001;
/// Chords to a full ellipse.
const ELLIPSE_STEPS: f64 = 144.0;

/// A plane transform: x' = a·x + b·y + c, y' = d·x + e·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 0.0,
        e: 1.0,
        f: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine {
            c: x,
            f: y,
            ..Affine::IDENTITY
        }
    }

    /// Rotation by `angle` radians, then scaling, then moving to (x, y).
    pub fn placed(x: f64, y: f64, angle: f64, sx: f64, sy: f64) -> Affine {
        let (sin, cos) = angle.sin_cos();
        Affine {
            a: cos * sx,
            b: -sin * sy,
            c: x,
            d: sin * sx,
            e: cos * sy,
            f: y,
        }
    }

    /// The plane part of an object coordinate system: where a 2D object with this normal lies in the world.
    pub fn ocs(normal: Vector3) -> Affine {
        if normal.x.abs() < 1e-12 && normal.y.abs() < 1e-12 && normal.z > 0.0 {
            return Affine::IDENTITY;
        }
        let m = Matrix3::arbitrary_axis(normal).m;
        Affine {
            a: m[0][0],
            b: m[0][1],
            c: 0.0,
            d: m[1][0],
            e: m[1][1],
            f: 0.0,
        }
    }

    /// `inner` first, then this one.
    pub fn then(&self, inner: &Affine) -> Affine {
        Affine {
            a: self.a * inner.a + self.b * inner.d,
            b: self.a * inner.b + self.b * inner.e,
            c: self.a * inner.c + self.b * inner.f + self.c,
            d: self.d * inner.a + self.e * inner.d,
            e: self.d * inner.b + self.e * inner.e,
            f: self.d * inner.c + self.e * inner.f + self.f,
        }
    }

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.a * p[0] + self.b * p[1] + self.c,
            self.d * p[0] + self.e * p[1] + self.f,
        ]
    }

    pub fn det(&self) -> f64 {
        self.a * self.e - self.b * self.d
    }

    /// The scale, when the transform keeps shapes (no stretch or shear): lengths then scale exactly.
    pub fn uniform(&self) -> Option<f64> {
        let sx = self.a.hypot(self.d);
        let sy = self.b.hypot(self.e);
        let dot = self.a * self.b + self.d * self.e;
        let size = sx.max(sy);
        ((sx - sy).abs() <= 1e-9 * size && dot.abs() <= 1e-9 * size * size).then_some(sx)
    }

    /// The direction the x axis turns to, in radians.
    pub fn angle(&self) -> f64 {
        self.d.atan2(self.a)
    }
}

/// An object's geometry: one or more chains of points, and its length and area in the units it was drawn in.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub parts: Vec<Vec<[f64; 2]>>,
    pub length: f64,
    pub area: f64,
    pub closed: bool,
    pub curved: bool,
    /// The length comes from the chain of points rather than the exact curve.
    pub approximate: bool,
}

impl Shape {
    fn chain(points: Vec<[f64; 2]>, length: f64, area: f64, closed: bool, curved: bool) -> Shape {
        Shape {
            parts: vec![points],
            length,
            area,
            closed,
            curved,
            approximate: false,
        }
    }

    /// The same shape moved into the space around it. Lengths scale exactly under a transform that keeps shapes;
    /// otherwise a curve's length comes from its transformed points. Areas always scale by the determinant.
    pub fn transformed(self, xf: &Affine) -> Shape {
        let parts: Vec<Vec<[f64; 2]>> = self
            .parts
            .iter()
            .map(|part| part.iter().map(|&p| xf.apply(p)).collect())
            .collect();
        let (length, approximate) = match xf.uniform() {
            Some(scale) => (self.length * scale, self.approximate),
            None if !self.curved => (
                parts.iter().map(|p| chain_length(p, self.closed)).sum(),
                self.approximate,
            ),
            None => (
                parts.iter().map(|p| chain_length(p, self.closed)).sum(),
                true,
            ),
        };
        Shape {
            parts,
            length,
            area: self.area * xf.det().abs(),
            approximate,
            ..self
        }
    }

    pub fn bbox(&self) -> Option<[f64; 4]> {
        let mut found = None;
        for p in self.parts.iter().flatten() {
            found = Some(grow(found, *p));
        }
        found
    }
}

pub fn grow(bbox: Option<[f64; 4]>, p: [f64; 2]) -> [f64; 4] {
    match bbox {
        None => [p[0], p[1], p[0], p[1]],
        Some(b) => [
            b[0].min(p[0]),
            b[1].min(p[1]),
            b[2].max(p[0]),
            b[3].max(p[1]),
        ],
    }
}

pub fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

pub fn chain_length(points: &[[f64; 2]], closed: bool) -> f64 {
    let mut total: f64 = points.windows(2).map(|w| dist(w[0], w[1])).sum();
    if closed && points.len() > 2 {
        total += dist(points[points.len() - 1], points[0]);
    }
    total
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The area a ring of points encloses, summed about its own first point so survey coordinates keep their digits.
pub fn ring_area(ring: &[[f64; 2]]) -> f64 {
    signed_area(ring).abs()
}

/// A ring's area, positive when it runs counter-clockwise.
fn signed_area(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let o = ring[0];
    let mut twice = 0.0;
    for i in 1..ring.len() - 1 {
        twice += cross(sub(ring[i], o), sub(ring[i + 1], o));
    }
    twice / 2.0
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Whether a point is inside a ring (ray casting).
pub fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut odd = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

pub fn line(start: [f64; 2], end: [f64; 2]) -> Shape {
    Shape::chain(vec![start, end], dist(start, end), 0.0, false, false)
}

fn arc_points(centre: [f64; 2], radius: f64, start: f64, sweep: f64) -> Vec<[f64; 2]> {
    let step = 2.0 * (1.0 - SAG).acos();
    let n = ((sweep.abs() / step).ceil() as usize).clamp(1, 4096);
    (0..=n)
        .map(|i| {
            let t = start + sweep * i as f64 / n as f64;
            [centre[0] + radius * t.cos(), centre[1] + radius * t.sin()]
        })
        .collect()
}

/// A circular arc running counter-clockwise from `start` to `end` (radians).
pub fn arc(centre: [f64; 2], radius: f64, start: f64, end: f64) -> Shape {
    let curve = Curve::Arc(Arc {
        centre,
        radius,
        start_angle: start,
        end_angle: end,
    });
    let mut sweep = (end - start).rem_euclid(TAU);
    if sweep < 1e-12 {
        sweep = TAU;
    }
    Shape::chain(
        arc_points(centre, radius, start, sweep),
        curve.length(),
        0.0,
        false,
        true,
    )
}

pub fn circle(centre: [f64; 2], radius: f64) -> Shape {
    let curve = Curve::Circle(Circle { centre, radius });
    let mut points = arc_points(centre, radius, 0.0, TAU);
    points.pop(); // the ring closes itself
    Shape::chain(
        points,
        curve.length(),
        curve.enclosed_area().abs(),
        true,
        true,
    )
}

/// An ellipse or part of one: centre, the end of its major axis from the centre, the minor axis's share of the
/// major, and the parameters it runs between. The normal's sign says which way the minor axis points.
pub fn ellipse(
    centre: [f64; 2],
    major: [f64; 2],
    ratio: f64,
    start: f64,
    end: f64,
    clockwise_normal: bool,
) -> Shape {
    let side = if clockwise_normal { -1.0 } else { 1.0 };
    let minor = [-major[1] * ratio * side, major[0] * ratio * side];
    let mut sweep = (end - start).rem_euclid(TAU);
    let full = sweep < 1e-9 || (sweep - TAU).abs() < 1e-9;
    if full {
        sweep = TAU;
    }
    let n = ((sweep / TAU * ELLIPSE_STEPS).ceil() as usize).max(8);
    let mut points: Vec<[f64; 2]> = (0..=n)
        .map(|i| {
            let t = start + sweep * i as f64 / n as f64;
            let (s, c) = t.sin_cos();
            [
                centre[0] + c * major[0] + s * minor[0],
                centre[1] + c * major[1] + s * minor[1],
            ]
        })
        .collect();
    let length = chain_length(&points, false);
    if full {
        points.pop();
        let a = major[0].hypot(major[1]);
        let area = std::f64::consts::PI * a * a * ratio.abs();
        return Shape {
            parts: vec![points],
            length,
            area,
            closed: true,
            curved: true,
            approximate: true,
        };
    }
    Shape {
        parts: vec![points],
        length,
        area: 0.0,
        closed: false,
        curved: true,
        approximate: true,
    }
}

/// A chain of straight and arc segments: each vertex with the bulge of the segment that leaves it.
pub fn polyline(vertices: &[([f64; 2], f64)], closed: bool) -> Shape {
    if vertices.is_empty() {
        return Shape::default();
    }
    let curve = Curve::Polyline(Polyline {
        vertices: vertices
            .iter()
            .map(|&(position, bulge)| PolylineVertex { position, bulge })
            .collect(),
        closed,
    });
    let count = vertices.len();
    let segments = if closed { count } else { count - 1 };
    let mut points = vec![vertices[0].0];
    let mut curved = false;
    for i in 0..segments {
        let (p0, bulge) = vertices[i];
        let p1 = vertices[(i + 1) % count].0;
        if bulge.abs() > 1e-12 && dist(p0, p1) > 0.0 {
            curved = true;
            points.extend(bulge_points(p0, p1, bulge).into_iter().skip(1));
        } else {
            points.push(p1);
        }
    }
    if closed && points.len() > 1 && points[points.len() - 1] == points[0] {
        points.pop();
    }
    let area = if closed {
        curve.enclosed_area().abs()
    } else {
        0.0
    };
    Shape::chain(points, curve.length(), area, closed, curved)
}

fn bulge_points(p0: [f64; 2], p1: [f64; 2], bulge: f64) -> Vec<[f64; 2]> {
    let sweep = 4.0 * bulge.atan();
    let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
    let k = (1.0 - bulge * bulge) / (4.0 * bulge);
    let centre = [
        (p0[0] + p1[0]) / 2.0 - k * dy,
        (p0[1] + p1[1]) / 2.0 + k * dx,
    ];
    let radius = dist(centre, p0);
    let start = (p0[1] - centre[1]).atan2(p0[0] - centre[0]);
    let mut points = arc_points(centre, radius, start, sweep);
    if let Some(last) = points.last_mut() {
        *last = p1;
    }
    points
}

/// A spline from its control points, or through its fit points when it has none.
pub fn spline(
    degree: usize,
    control: &[[f64; 2]],
    knots: &[f64],
    weights: &[f64],
    fit: &[[f64; 2]],
    closed: bool,
) -> Shape {
    let curve = NurbsCurve::new(
        degree,
        control.to_vec(),
        knots.to_vec(),
        (weights.len() == control.len() && !weights.is_empty()).then(|| weights.to_vec()),
    );
    let (points, length) = match curve {
        Some(nurbs) if control.len() > degree => {
            let size = bbox_size(control);
            let curve = Curve::Nurbs(nurbs);
            (curve.tessellate_within(size * 1e-4), curve.length())
        }
        _ => {
            let points = if fit.len() >= 2 {
                fit.to_vec()
            } else {
                control.to_vec()
            };
            let length = chain_length(&points, false);
            (points, length)
        }
    };
    let area = if closed { ring_area(&points) } else { 0.0 };
    Shape {
        parts: vec![points],
        length,
        area,
        closed,
        curved: true,
        approximate: true,
    }
}

fn bbox_size(points: &[[f64; 2]]) -> f64 {
    let mut b = None;
    for p in points {
        b = Some(grow(b, *p));
    }
    b.map(|b| (b[2] - b[0]).hypot(b[3] - b[1]))
        .unwrap_or(1.0)
        .max(1e-9)
}

/// A filled area bounded by rings: the outer rings count, the islands inside them come off, and islands within
/// islands count again.
pub fn rings(parts: Vec<Vec<[f64; 2]>>, curved: bool) -> Shape {
    let parts: Vec<Vec<[f64; 2]>> = parts.into_iter().filter(|p| p.len() >= 3).collect();
    let mut area = 0.0;
    let mut length = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let depth = parts
            .iter()
            .enumerate()
            .filter(|(j, other)| *j != i && inside(other, part[0]))
            .count();
        let a = ring_area(part);
        area += if depth % 2 == 0 { a } else { -a };
        length += chain_length(part, true);
    }
    Shape {
        parts,
        length,
        area: area.max(0.0),
        closed: true,
        curved,
        approximate: curved,
    }
}

/// Straight segments of a hatch boundary edge, running the way the edge runs.
pub fn arc_edge(
    centre: [f64; 2],
    radius: f64,
    start: f64,
    end: f64,
    counter_clockwise: bool,
) -> Vec<[f64; 2]> {
    if counter_clockwise {
        let sweep = (end - start).rem_euclid(TAU);
        return arc_points(
            centre,
            radius,
            start,
            if sweep < 1e-12 { TAU } else { sweep },
        );
    }
    // A clockwise edge keeps its angles as seen from below: mirror them back.
    let (start, end) = (-start, -end);
    let sweep = (start - end).rem_euclid(TAU);
    arc_points(
        centre,
        radius,
        start,
        -(if sweep < 1e-12 { TAU } else { sweep }),
    )
}

pub fn bulge_chain(vertices: &[([f64; 2], f64)], closed: bool) -> Vec<[f64; 2]> {
    polyline(vertices, closed)
        .parts
        .into_iter()
        .next()
        .unwrap_or_default()
}

type Segment = [[f64; 2]; 2];

/// A clip boundary (CAD's XCLIP) in the space a block reference is placed in: of the reference's objects, only
/// what lies inside it shows.
#[derive(Clone, Debug)]
pub struct Clip {
    /// Counter-clockwise.
    ring: Vec<[f64; 2]>,
    bbox: [f64; 4],
    tolerance: f64,
}

impl Clip {
    pub fn new(mut ring: Vec<[f64; 2]>) -> Option<Clip> {
        ring.dedup();
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        let area = signed_area(&ring);
        if ring.len() < 3 || area == 0.0 {
            return None;
        }
        if area < 0.0 {
            ring.reverse();
        }
        let bbox = ring.iter().fold(None, |b, p| Some(grow(b, *p)))?;
        let tolerance = (bbox[2] - bbox[0]).hypot(bbox[3] - bbox[1]) * 1e-9;
        Some(Clip {
            ring,
            bbox,
            tolerance,
        })
    }

    /// Whether a point shows: inside the boundary or on it.
    pub fn shows(&self, p: [f64; 2]) -> bool {
        overlaps([p[0], p[1], p[0], p[1]], self.bbox, self.tolerance)
            && (inside(&self.ring, p) || along(&self.ring, p, self.tolerance).is_some())
    }
}

/// Whether a point shows through every clip.
pub fn shows(clips: &[Clip], p: [f64; 2]) -> bool {
    clips.iter().all(|clip| clip.shows(p))
}

impl Shape {
    /// What of the shape shows through clips: a line is cut where it leaves, and a closed shape keeps the area
    /// inside and the part of its outline inside. `None` when none of it shows.
    pub fn clipped(self, clips: &[Clip]) -> Option<Shape> {
        clips.iter().try_fold(self, |shape, clip| shape.clip(clip))
    }

    fn clip(self, clip: &Clip) -> Option<Shape> {
        let Some(bbox) = self.bbox() else {
            return Some(self);
        };
        if !overlaps(bbox, clip.bbox, clip.tolerance) {
            return None;
        }
        let Some(stretches) = chains_through(&self.parts, self.closed, clip) else {
            return Some(self); // all of it shows
        };
        let length = stretches.iter().map(|s| chain_length(s, false)).sum();
        let approximate = self.approximate || self.curved;
        if !self.closed {
            return (!stretches.is_empty()).then_some(Shape {
                parts: stretches,
                length,
                approximate,
                ..self
            });
        }
        let (loops, area) = region_through(&self.parts, clip);
        if loops.is_empty() && stretches.is_empty() {
            return None;
        }
        Some(Shape {
            parts: if loops.is_empty() { stretches } else { loops },
            length,
            area,
            approximate,
            ..self
        })
    }
}

fn overlaps(a: [f64; 4], b: [f64; 4], tolerance: f64) -> bool {
    a[0] <= b[2] + tolerance
        && b[0] <= a[2] + tolerance
        && a[1] <= b[3] + tolerance
        && b[1] <= a[3] + tolerance
}

fn edges(points: &[[f64; 2]], closed: bool) -> Vec<Segment> {
    let n = points.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count)
        .map(|i| [points[i], points[(i + 1) % n]])
        .filter(|s| s[0] != s[1])
        .collect()
}

fn middle(s: Segment) -> [f64; 2] {
    [(s[0][0] + s[1][0]) / 2.0, (s[0][1] + s[1][1]) / 2.0]
}

fn distance_to(s: Segment, p: [f64; 2]) -> f64 {
    let d = sub(s[1], s[0]);
    let t = (dot(sub(p, s[0]), d) / dot(d, d)).clamp(0.0, 1.0);
    dist(p, [s[0][0] + t * d[0], s[0][1] + t * d[1]])
}

/// The edge of a ring a point lies on, if any.
fn along(ring: &[[f64; 2]], p: [f64; 2], tolerance: f64) -> Option<Segment> {
    edges(ring, true)
        .into_iter()
        .find(|&s| distance_to(s, p) <= tolerance)
}

/// Where two sets of segments meet: for each segment of each set, the points along it (with their share of its
/// length) where the other set crosses or joins it. Each meeting is worked out once and both segments get the
/// same point, so the pieces they are cut into meet exactly.
#[allow(clippy::type_complexity)]
fn meetings(
    a: &[Segment],
    b: &[Segment],
    tolerance: f64,
) -> (Vec<Vec<(f64, [f64; 2])>>, Vec<Vec<(f64, [f64; 2])>>) {
    let mut on_a = vec![vec![]; a.len()];
    let mut on_b = vec![vec![]; b.len()];
    let bbox = |s: &Segment| {
        [
            s[0][0].min(s[1][0]),
            s[0][1].min(s[1][1]),
            s[0][0].max(s[1][0]),
            s[0][1].max(s[1][1]),
        ]
    };
    let b_boxes: Vec<[f64; 4]> = b.iter().map(bbox).collect();
    for (i, s) in a.iter().enumerate() {
        let s_box = bbox(s);
        let r = sub(s[1], s[0]);
        let r_len = r[0].hypot(r[1]);
        let eu = tolerance / r_len;
        for (j, t) in b.iter().enumerate() {
            if !overlaps(s_box, b_boxes[j], tolerance) {
                continue;
            }
            let q = sub(t[1], t[0]);
            let q_len = q[0].hypot(q[1]);
            let ev = tolerance / q_len;
            let w = sub(t[0], s[0]);
            let denominator = cross(r, q);
            if denominator.abs() > 1e-12 * r_len * q_len {
                let u = cross(w, q) / denominator;
                let v = cross(w, r) / denominator;
                if u < -eu || u > 1.0 + eu || v < -ev || v > 1.0 + ev {
                    continue;
                }
                // a meeting at an end is that end exactly
                let point = if u.abs() <= eu {
                    s[0]
                } else if (u - 1.0).abs() <= eu {
                    s[1]
                } else if v.abs() <= ev {
                    t[0]
                } else if (v - 1.0).abs() <= ev {
                    t[1]
                } else {
                    [s[0][0] + u * r[0], s[0][1] + u * r[1]]
                };
                if u > eu && u < 1.0 - eu {
                    on_a[i].push((u, point));
                }
                if v > ev && v < 1.0 - ev {
                    on_b[j].push((v, point));
                }
            } else if cross(w, r).abs() / r_len <= tolerance {
                // along one line: each gets the other's ends that lie within it
                for p in t {
                    let u = dot(sub(*p, s[0]), r) / (r_len * r_len);
                    if u > eu && u < 1.0 - eu {
                        on_a[i].push((u, *p));
                    }
                }
                for p in s {
                    let v = dot(sub(*p, t[0]), q) / (q_len * q_len);
                    if v > ev && v < 1.0 - ev {
                        on_b[j].push((v, *p));
                    }
                }
            }
        }
    }
    (on_a, on_b)
}

/// A segment cut at points along it, in order.
fn pieces(s: Segment, mut cuts: Vec<(f64, [f64; 2])>) -> Vec<Segment> {
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = vec![];
    let mut from = s[0];
    for (_, p) in cuts {
        if p != from {
            out.push([from, p]);
            from = p;
        }
    }
    if from != s[1] {
        out.push([from, s[1]]);
    }
    out
}

/// The stretches of chains of points that show through a clip, or `None` when all of them show.
fn chains_through(
    parts: &[Vec<[f64; 2]>],
    closed: bool,
    clip: &Clip,
) -> Option<Vec<Vec<[f64; 2]>>> {
    let boundary = edges(&clip.ring, true);
    let mut out = vec![];
    let mut all = true;
    for part in parts {
        if part.len() == 1 {
            if clip.shows(part[0]) {
                out.push(part.clone());
            } else {
                all = false;
            }
            continue;
        }
        let segments = edges(part, closed);
        let (cuts, _) = meetings(&segments, &boundary, clip.tolerance);
        let mut stretches: Vec<Vec<[f64; 2]>> = vec![];
        let mut open = false;
        for (s, c) in segments.into_iter().zip(cuts) {
            for piece in pieces(s, c) {
                if clip.shows(middle(piece)) {
                    match stretches.last_mut() {
                        Some(last) if open => last.push(piece[1]),
                        _ => stretches.push(vec![piece[0], piece[1]]),
                    }
                    open = true;
                } else {
                    all = false;
                    open = false;
                }
            }
        }
        // a closed chain cut somewhere joins up again across its first point
        if closed && open && stretches.len() > 1 && stretches[0][0] == part[0] {
            let last = stretches.pop().unwrap_or_default();
            let first = std::mem::take(&mut stretches[0]);
            stretches[0] = last.into_iter().chain(first.into_iter().skip(1)).collect();
        }
        out.extend(stretches);
    }
    (!all).then_some(out)
}

/// What of a filled region shows through a clip: the loops around it and its area. The region is its rings'
/// inside, islands taken off; its edges inside the clip and the clip's edges inside it bound what shows, and
/// the area comes from them directly.
fn region_through(parts: &[Vec<[f64; 2]>], clip: &Clip) -> (Vec<Vec<[f64; 2]>>, f64) {
    let tolerance = clip.tolerance;
    let rings: Vec<&Vec<[f64; 2]>> = parts.iter().filter(|p| p.len() >= 3).collect();
    // outer rings run counter-clockwise and islands clockwise, so the region lies left of every edge
    let mut own: Vec<Segment> = vec![];
    for (i, ring) in rings.iter().enumerate() {
        let depth = rings
            .iter()
            .enumerate()
            .filter(|(j, other)| *j != i && inside(other, ring[0]))
            .count();
        let ring_edges = edges(ring, true);
        if (signed_area(ring) > 0.0) == (depth % 2 == 0) {
            own.extend(ring_edges);
        } else {
            own.extend(ring_edges.into_iter().rev().map(|[a, b]| [b, a]));
        }
    }
    let boundary = edges(&clip.ring, true);
    let (own_cuts, boundary_cuts) = meetings(&own, &boundary, tolerance);
    let mut kept: Vec<Segment> = vec![];
    for (s, c) in own.iter().zip(own_cuts) {
        for piece in pieces(*s, c) {
            let m = middle(piece);
            let keep = match along(&clip.ring, m, tolerance) {
                // along the clip's own edge: kept once, when both run the same way
                Some(edge) => dot(sub(piece[1], piece[0]), sub(edge[1], edge[0])) > 0.0,
                None => inside(&clip.ring, m),
            };
            if keep {
                kept.push(piece);
            }
        }
    }
    for (s, c) in boundary.iter().zip(boundary_cuts) {
        for piece in pieces(*s, c) {
            let m = middle(piece);
            let on_own = own.iter().any(|&e| distance_to(e, m) <= tolerance);
            let filled = rings.iter().filter(|r| inside(r, m)).count() % 2 == 1;
            if !on_own && filled {
                kept.push(piece);
            }
        }
    }
    let o = clip.ring[0];
    let area: f64 = kept
        .iter()
        .map(|s| cross(sub(s[0], o), sub(s[1], o)))
        .sum::<f64>()
        / 2.0;
    (loops(kept), area.max(0.0))
}

/// Edges given as chains of points, joined end to end into rings where their ends meet (within a billionth of
/// their size).
pub fn rings_of(edges: Vec<Vec<[f64; 2]>>) -> Vec<Vec<[f64; 2]>> {
    let edges: Vec<Vec<[f64; 2]>> = edges.into_iter().filter(|e| e.len() > 1).collect();
    let Some(bbox) = edges.iter().flatten().fold(None, |b, p| Some(grow(b, *p))) else {
        return vec![];
    };
    let tolerance = (bbox[2] - bbox[0]).hypot(bbox[3] - bbox[1]) * 1e-9;
    let meets = |a: [f64; 2], b: [f64; 2]| dist(a, b) <= tolerance;
    let mut used = vec![false; edges.len()];
    let mut rings = vec![];
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut ring = edges[first].clone();
        loop {
            let end = ring[ring.len() - 1];
            if ring.len() > 2 && meets(end, ring[0]) {
                ring.pop();
                break;
            }
            let next = (0..edges.len()).find(|&i| {
                !used[i] && (meets(edges[i][0], end) || meets(edges[i][edges[i].len() - 1], end))
            });
            let Some(i) = next else { break };
            used[i] = true;
            if meets(edges[i][0], end) {
                ring.extend(edges[i].iter().skip(1));
            } else {
                ring.extend(edges[i].iter().rev().skip(1));
            }
        }
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings
}

/// Pieces that meet end to start, joined into loops.
fn loops(pieces: Vec<Segment>) -> Vec<Vec<[f64; 2]>> {
    let key = |p: [f64; 2]| (p[0].to_bits(), p[1].to_bits());
    let mut starting: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    for (i, s) in pieces.iter().enumerate() {
        starting.entry(key(s[0])).or_default().push(i);
    }
    let mut used = vec![false; pieces.len()];
    let mut out = vec![];
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut chain = vec![pieces[first][0], pieces[first][1]];
        loop {
            let end = chain[chain.len() - 1];
            if end == chain[0] {
                chain.pop();
                break;
            }
            let next = starting
                .get(&key(end))
                .and_then(|c| c.iter().copied().find(|&i| !used[i]));
            let Some(i) = next else { break };
            used[i] = true;
            chain.push(pieces[i][1]);
        }
        out.push(chain);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * b.abs().max(1.0)
    }

    #[test]
    fn a_bulged_polyline_is_measured_exactly() {
        let square = [
            ([0.0, 0.0], 0.0),
            ([10.0, 0.0], 0.0),
            ([10.0, 10.0], 1.0),
            ([0.0, 10.0], 0.0),
        ];
        let shape = polyline(&square, true);
        assert!(close(shape.area, 100.0 + std::f64::consts::PI * 12.5));
        assert!(close(shape.length, 30.0 + std::f64::consts::PI * 5.0));
    }

    #[test]
    fn a_placed_block_scales_lengths_and_areas() {
        let square = polyline(
            &[
                ([0.0, 0.0], 0.0),
                ([1.0, 0.0], 0.0),
                ([1.0, 1.0], 0.0),
                ([0.0, 1.0], 0.0),
            ],
            true,
        );
        let xf = Affine::placed(100.0, 50.0, std::f64::consts::FRAC_PI_2, 2.0, 2.0);
        let placed = square.transformed(&xf);
        assert!(close(placed.area, 4.0) && close(placed.length, 8.0));
        let first = placed.parts[0][0];
        assert!(close(first[0], 100.0) && close(first[1], 50.0));
    }

    #[test]
    fn an_island_comes_off_the_area() {
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let island = vec![[4.0, 4.0], [6.0, 4.0], [6.0, 6.0], [4.0, 6.0]];
        assert!(close(rings(vec![outer, island], false).area, 96.0));
    }

    fn square(x: f64, y: f64, size: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
    }

    /// An L: the 10 × 10 square at the origin with its top right 6 × 6 cut away.
    fn l_clip() -> Clip {
        Clip::new(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 4.0],
            [4.0, 4.0],
            [4.0, 10.0],
            [0.0, 10.0],
        ])
        .unwrap()
    }

    #[test]
    fn a_clip_cuts_a_line_where_it_leaves() {
        let clip = Clip::new(square(0.0, 0.0, 10.0)).unwrap();
        let cut = line([-5.0, 5.0], [15.0, 5.0])
            .clipped(std::slice::from_ref(&clip))
            .unwrap();
        assert!(close(cut.length, 10.0));
        assert!(line([20.0, 0.0], [30.0, 0.0])
            .clipped(std::slice::from_ref(&clip))
            .is_none());
        let along = line([0.0, 0.0], [10.0, 0.0]).clipped(&[clip]).unwrap();
        assert!(close(along.length, 10.0));
        // across the L's corners
        let across = line([2.0, 6.0], [8.0, 6.0]).clipped(&[l_clip()]).unwrap();
        assert!(close(across.length, 2.0));
        let through = line([8.0, 2.0], [8.0, 12.0]).clipped(&[l_clip()]).unwrap();
        assert!(close(through.length, 2.0));
    }

    #[test]
    fn a_clip_keeps_the_area_inside() {
        // a large square with an island, over the whole L: the L less the island
        let region = rings(vec![square(-5.0, -5.0, 20.0), square(1.0, 1.0, 2.0)], false);
        let shown = region.clipped(&[l_clip()]).unwrap();
        assert!(close(shown.area, 64.0 - 4.0));
        assert!(close(shown.length, 8.0)); // only the island's outline is inside
                                           // a square half over the L
        let vertices: Vec<([f64; 2], f64)> = square(5.0, -5.0, 10.0)
            .into_iter()
            .map(|p| (p, 0.0))
            .collect();
        let shown = polyline(&vertices, true).clipped(&[l_clip()]).unwrap();
        assert!(close(shown.area, 20.0) && close(shown.length, 4.0));
        // sharing the L's edges: its bottom, part of its right side and its left
        let vertices: Vec<([f64; 2], f64)> = [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]]
            .into_iter()
            .map(|p| (p, 0.0))
            .collect();
        let shown = polyline(&vertices, true).clipped(&[l_clip()]).unwrap();
        assert!(close(shown.area, 44.0) && close(shown.length, 23.0));
        assert_eq!(shown.parts.len(), 1);
        // wholly inside, and wholly outside
        let inside = rings(vec![square(1.0, 1.0, 2.0)], false);
        assert!(close(inside.clipped(&[l_clip()]).unwrap().area, 4.0));
        assert!(rings(vec![square(6.0, 6.0, 2.0)], false)
            .clipped(&[l_clip()])
            .is_none());
    }

    #[test]
    fn edges_join_into_rings_either_way_round() {
        // a square's edges in no order, one of them backwards, meeting within a hair
        let edges = vec![
            vec![[10.0, 10.0], [0.0, 10.0]],
            vec![[0.0, 0.0], [10.0, 0.0]],
            vec![[0.0, 0.0], [0.0, 10.0 + 1e-12]],
            vec![[10.0, 0.0], [10.0, 10.0]],
        ];
        let rings = rings_of(edges);
        assert_eq!(rings.len(), 1);
        assert!(close(ring_area(&rings[0]), 100.0));
    }

    #[test]
    fn a_circle_encloses_pi_r_squared() {
        assert!(close(
            circle([5.0e5, 5.0e5], 2.0).area,
            std::f64::consts::PI * 4.0
        ));
    }

    fn straight(points: Vec<[f64; 2]>) -> Vec<([f64; 2], f64)> {
        points.into_iter().map(|p| (p, 0.0)).collect()
    }

    fn ends(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
        (points[0], points[points.len() - 1])
    }

    fn at(p: [f64; 2], x: f64, y: f64) -> bool {
        close(p[0], x) && close(p[1], y)
    }

    #[test]
    fn a_zero_length_line_measures_nothing_but_keeps_its_place() {
        let dot = line([3.0, 4.0], [3.0, 4.0]);
        assert_eq!((dot.length, dot.area), (0.0, 0.0));
        assert_eq!(dot.bbox(), Some([3.0, 4.0, 3.0, 4.0]));
    }

    #[test]
    fn a_polyline_of_one_point_or_none_measures_nothing() {
        let none = polyline(&[], false);
        assert!(none.parts.is_empty() && none.bbox().is_none());
        let one = polyline(&[([2.0, 2.0], 0.0)], true);
        assert_eq!((one.length, one.area), (0.0, 0.0));
        assert_eq!(one.parts, vec![vec![[2.0, 2.0]]]);
    }

    #[test]
    fn a_closed_polyline_of_collinear_points_encloses_nothing() {
        let flat = polyline(&straight(vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]), true);
        assert!(close(flat.area, 0.0));
        assert!(close(flat.length, 20.0)); // out along the line and back
    }

    #[test]
    fn a_closed_polyline_that_repeats_its_first_point_is_measured_once() {
        let mut points = square(0.0, 0.0, 10.0);
        points.push([0.0, 0.0]);
        let shape = polyline(&straight(points), true);
        assert!(close(shape.length, 40.0) && close(shape.area, 100.0));
    }

    #[test]
    fn a_bulge_on_a_zero_length_segment_adds_nothing() {
        let shape = polyline(
            &[([0.0, 0.0], 1.0), ([0.0, 0.0], 0.0), ([10.0, 0.0], 0.0)],
            false,
        );
        assert!(close(shape.length, 10.0) && !shape.curved);
        assert!(shape.parts[0].iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn an_arc_across_zero_degrees_runs_counter_clockwise() {
        let half = arc([0.0, 0.0], 2.0, 1.5 * PI, 0.5 * PI);
        assert!(close(half.length, 2.0 * PI));
        let (first, last) = ends(&half.parts[0]);
        assert!(at(first, 0.0, -2.0) && at(last, 0.0, 2.0));
        assert!(half.parts[0].iter().all(|p| p[0] > -1e-9)); // round by (2, 0), not (-2, 0)
    }

    #[test]
    fn an_arc_that_ends_where_it_starts_is_a_whole_circle() {
        let whole = arc([0.0, 0.0], 2.0, 1.0, 1.0);
        let (first, last) = ends(&whole.parts[0]);
        assert!(at(first, last[0], last[1])); // drawn all the way round
        assert!(close(whole.length, 4.0 * PI));
    }

    #[test]
    fn a_ring_measures_the_same_either_way_round_and_needs_three_points() {
        let anticlockwise = square(0.0, 0.0, 10.0);
        let clockwise: Vec<[f64; 2]> = anticlockwise.iter().rev().copied().collect();
        assert!(close(ring_area(&anticlockwise), 100.0) && close(ring_area(&clockwise), 100.0));
        assert!(signed_area(&anticlockwise) > 0.0 && signed_area(&clockwise) < 0.0);
        assert_eq!(ring_area(&[[0.0, 0.0], [10.0, 10.0]]), 0.0);
    }

    #[test]
    fn survey_coordinates_keep_their_digits() {
        // a 100 mm square five thousand kilometres out, in metres
        let ring = square(5.0e6, 3.0e6, 0.1);
        assert!((ring_area(&ring) - 0.01).abs() < 1e-9);
    }

    #[test]
    fn a_point_is_inside_a_ring_only_within_it() {
        let l = l_clip().ring;
        assert!(inside(&l, [2.0, 2.0]) && inside(&l, [2.0, 8.0]) && inside(&l, [8.0, 2.0]));
        assert!(!inside(&l, [7.0, 7.0])); // the L's notch
        assert!(!inside(&l, [-1.0, 2.0]) && !inside(&l, [11.0, 2.0]));
    }

    #[test]
    fn an_island_within_an_island_counts_again() {
        let shape = rings(
            vec![
                square(0.0, 0.0, 10.0),
                square(2.0, 2.0, 6.0),
                square(4.0, 4.0, 2.0),
            ],
            false,
        );
        assert!(close(shape.area, 100.0 - 36.0 + 4.0));
        assert!(close(shape.length, 40.0 + 24.0 + 8.0));
        assert!(shape.closed && !shape.approximate);
    }

    #[test]
    fn a_ring_of_fewer_than_three_points_is_dropped() {
        let shape = rings(
            vec![vec![[0.0, 0.0], [10.0, 10.0]], square(0.0, 0.0, 1.0)],
            false,
        );
        assert_eq!(shape.parts.len(), 1);
        assert!(close(shape.area, 1.0) && close(shape.length, 4.0));
    }

    #[test]
    fn then_applies_the_inner_transform_first() {
        let shift = Affine::translate(10.0, 0.0);
        let turn = Affine::placed(0.0, 0.0, FRAC_PI_2, 1.0, 1.0);
        assert!(at(turn.then(&shift).apply([1.0, 0.0]), 0.0, 11.0));
        assert!(at(shift.then(&turn).apply([1.0, 0.0]), 10.0, 1.0));
    }

    #[test]
    fn a_mirrored_block_keeps_its_exact_length_and_a_positive_area() {
        let bulged = polyline(
            &[
                ([0.0, 0.0], 0.0),
                ([10.0, 0.0], 0.0),
                ([10.0, 10.0], 1.0),
                ([0.0, 10.0], 0.0),
            ],
            true,
        );
        let (area, length) = (100.0 + PI * 12.5, 30.0 + PI * 5.0);
        let mirror = Affine::placed(0.0, 0.0, 0.0, -1.0, 1.0);
        assert!(mirror.det() < 0.0 && mirror.uniform() == Some(1.0));
        let mirrored = bulged.clone().transformed(&mirror);
        assert!(close(mirrored.area, area) && close(mirrored.length, length));
        assert!(!mirrored.approximate);
        assert!(at(mirrored.parts[0][1], -10.0, 0.0));
        let doubled = bulged.transformed(&Affine::placed(5.0, 5.0, 1.0, -2.0, 2.0));
        assert!(close(doubled.area, 4.0 * area) && close(doubled.length, 2.0 * length));
    }

    #[test]
    fn a_stretched_circle_is_measured_from_its_points() {
        let stretch = Affine::placed(0.0, 0.0, 0.0, 2.0, 1.0);
        assert_eq!(stretch.uniform(), None);
        let stretched = circle([0.0, 0.0], 1.0).transformed(&stretch);
        assert!(close(stretched.area, 2.0 * PI)); // areas scale exactly
                                                  // the 2 × 1 ellipse's perimeter (Ramanujan), which its chords fall just short of
        let perimeter = PI * (9.0 - 35f64.sqrt());
        assert!(stretched.approximate);
        assert!(stretched.length < perimeter && stretched.length > perimeter * 0.999);
    }

    #[test]
    fn a_stretched_or_sheared_straight_shape_stays_exact() {
        let unit = || polyline(&straight(square(0.0, 0.0, 1.0)), true);
        let stretched = unit().transformed(&Affine::placed(0.0, 0.0, 0.0, 2.0, 3.0));
        assert!(close(stretched.length, 10.0) && close(stretched.area, 6.0));
        assert!(!stretched.approximate);
        let shear = Affine {
            b: 1.0,
            ..Affine::IDENTITY
        };
        assert_eq!(shear.uniform(), None);
        let sheared = unit().transformed(&shear);
        assert!(close(sheared.length, 2.0 + 2.0 * 2f64.sqrt()) && close(sheared.area, 1.0));
        assert!(!sheared.approximate);
    }

    #[test]
    fn an_object_facing_down_is_mirrored_into_the_world() {
        assert_eq!(Affine::ocs(Vector3::new(0.0, 0.0, 1.0)), Affine::IDENTITY);
        let down = Affine::ocs(Vector3::new(0.0, 0.0, -1.0));
        assert!(at(down.apply([5.0, 2.0]), -5.0, 2.0));
    }

    #[test]
    fn a_clip_needs_an_area() {
        assert!(Clip::new(vec![]).is_none());
        assert!(Clip::new(vec![[0.0, 0.0], [10.0, 0.0]]).is_none());
        assert!(Clip::new(vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]).is_none());
        // the same two points over and over
        assert!(Clip::new(vec![
            [0.0, 0.0],
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 0.0],
            [0.0, 0.0]
        ])
        .is_none());
    }

    #[test]
    fn a_clip_drawn_clockwise_or_closed_cuts_the_same() {
        let clockwise: Vec<[f64; 2]> = square(0.0, 0.0, 10.0).into_iter().rev().collect();
        let mut closed = square(0.0, 0.0, 10.0);
        closed.push([0.0, 0.0]);
        for ring in [clockwise, closed] {
            let clip = [Clip::new(ring).unwrap()];
            let cut = line([-5.0, 5.0], [15.0, 5.0]).clipped(&clip).unwrap();
            assert!(close(cut.length, 10.0));
            let corner = rings(vec![square(5.0, 5.0, 10.0)], false)
                .clipped(&clip)
                .unwrap();
            assert!(close(corner.area, 25.0) && close(corner.length, 10.0));
        }
    }

    #[test]
    fn a_point_shows_only_inside_or_on_a_clip() {
        let point = |p: [f64; 2]| Shape {
            parts: vec![vec![p]],
            ..Shape::default()
        };
        let clip = [l_clip()];
        assert!(point([2.0, 2.0]).clipped(&clip).is_some());
        assert!(point([10.0, 2.0]).clipped(&clip).is_some()); // on its edge
        assert!(point([7.0, 7.0]).clipped(&clip).is_none()); // in the notch
        assert!(point([15.0, 2.0]).clipped(&clip).is_none());
        assert!(point([15.0, 2.0]).clipped(&[]).is_some()); // nothing clips it
    }

    #[test]
    fn only_what_shows_through_every_clip_is_kept() {
        let clips = [
            Clip::new(square(0.0, 0.0, 10.0)).unwrap(),
            Clip::new(square(5.0, 0.0, 10.0)).unwrap(),
        ];
        let cut = line([-5.0, 5.0], [20.0, 5.0]).clipped(&clips).unwrap();
        assert!(close(cut.length, 5.0));
        assert!(shows(&clips, [7.0, 5.0]));
        assert!(!shows(&clips, [2.0, 5.0]) && !shows(&clips, [12.0, 5.0]));
    }

    #[test]
    fn a_line_along_or_through_a_clips_corners_is_cut_there() {
        let clip = [Clip::new(square(0.0, 0.0, 10.0)).unwrap()];
        // along the bottom edge and on past both corners
        let along = line([-5.0, 0.0], [15.0, 0.0]).clipped(&clip).unwrap();
        assert!(close(along.length, 10.0));
        // corner to corner and beyond
        let diagonal = line([-5.0, -5.0], [15.0, 15.0]).clipped(&clip).unwrap();
        assert!(close(diagonal.length, 200f64.sqrt()));
        assert!(at(diagonal.parts[0][0], 0.0, 0.0) && at(diagonal.parts[0][1], 10.0, 10.0));
        // only touching a corner from outside shows nothing
        assert!(line([10.0, 10.0], [20.0, 20.0]).clipped(&clip).is_none());
    }

    #[test]
    fn a_clipped_circle_keeps_the_half_inside() {
        let clip =
            [Clip::new(vec![[0.0, -10.0], [10.0, -10.0], [10.0, 10.0], [0.0, 10.0]]).unwrap()];
        let half = circle([0.0, 0.0], 5.0).clipped(&clip).unwrap();
        // from its chords, which fall a little inside the curve
        assert!((half.area / (PI * 12.5) - 1.0).abs() < 0.002);
        assert!((half.length / (PI * 5.0) - 1.0).abs() < 0.002);
        assert!(half.closed && half.approximate);
        assert!(half.parts.iter().flatten().all(|p| p[0] >= -1e-9));
    }

    #[test]
    fn pieces_meeting_end_to_start_join_into_loops() {
        let pieces = vec![
            [[10.0, 10.0], [0.0, 10.0]],
            [[0.0, 0.0], [10.0, 0.0]],
            [[20.0, 0.0], [21.0, 0.0]],
            [[0.0, 10.0], [0.0, 0.0]],
            [[21.0, 0.0], [20.0, 1.0]],
            [[10.0, 0.0], [10.0, 10.0]],
            [[20.0, 1.0], [20.0, 0.0]],
        ];
        let found = loops(pieces);
        assert_eq!(found.iter().map(Vec::len).collect::<Vec<_>>(), [4, 3]);
        assert!(close(ring_area(&found[0]), 100.0) && close(ring_area(&found[1]), 0.5));
    }

    #[test]
    fn separate_edges_join_into_separate_rings() {
        let edges = vec![
            vec![[20.0, 0.0], [22.0, 0.0]],
            arc_points([0.0, 0.0], 5.0, 0.0, PI), // (5, 0) round to (-5, 0)
            vec![[22.0, 2.0], [20.0, 2.0], [20.0, 0.0]],
            vec![[5.0, 0.0], [-5.0, 0.0]], // the diameter, the other way round
            vec![[22.0, 0.0], [22.0, 2.0]],
            vec![[1.0, 1.0]],
        ];
        let mut areas: Vec<f64> = rings_of(edges).iter().map(|r| ring_area(r)).collect();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas.len(), 2);
        assert!(close(areas[0], 4.0));
        assert!((areas[1] / (PI * 12.5) - 1.0).abs() < 0.002);
        assert!(rings_of(vec![vec![[1.0, 1.0]]]).is_empty() && rings_of(vec![]).is_empty());
    }

    #[test]
    fn a_full_ellipse_encloses_pi_a_b() {
        let whole = ellipse([0.0, 0.0], [4.0, 0.0], 0.5, 0.0, TAU, false);
        assert!(whole.closed && whole.approximate && close(whole.area, 8.0 * PI));
        let perimeter = PI * (18.0 - 140f64.sqrt()); // Ramanujan's, for 4 × 2
        assert!((whole.length / perimeter - 1.0).abs() < 0.001);
    }

    #[test]
    fn an_elliptical_arc_turns_the_way_its_normal_faces() {
        let quarter = |down| ellipse([0.0, 0.0], [2.0, 0.0], 0.5, 0.0, FRAC_PI_2, down);
        let up = quarter(false);
        assert!(!up.closed && up.area == 0.0);
        assert!(at(ends(&up.parts[0]).1, 0.0, 1.0));
        assert!(at(ends(&quarter(true).parts[0]).1, 0.0, -1.0));
    }

    #[test]
    fn a_straight_spline_measures_its_control_polygon() {
        let open = spline(
            1,
            &[[0.0, 0.0], [3.0, 4.0], [6.0, 0.0]],
            &[0.0, 0.0, 1.0, 2.0, 2.0],
            &[],
            &[],
            false,
        );
        assert!(close(open.length, 10.0) && open.area == 0.0 && open.approximate);
        let mut control = square(0.0, 0.0, 10.0);
        control.push([0.0, 0.0]);
        let closed = spline(
            1,
            &control,
            &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0],
            &[],
            &[],
            true,
        );
        assert!(close(closed.length, 40.0) && close(closed.area, 100.0));
    }

    #[test]
    fn a_spline_without_control_points_runs_through_its_fit_points() {
        let fit = [[0.0, 0.0], [3.0, 4.0], [6.0, 0.0]];
        let shape = spline(3, &[], &[], &[], &fit, false);
        assert_eq!(shape.parts, vec![fit.to_vec()]);
        assert!(close(shape.length, 10.0) && shape.approximate);
    }

    #[test]
    fn a_hatch_edge_runs_the_way_it_turns() {
        let anticlockwise = arc_edge([0.0, 0.0], 1.0, 0.0, FRAC_PI_2, true);
        let (first, last) = ends(&anticlockwise);
        assert!(at(first, 1.0, 0.0) && at(last, 0.0, 1.0));
        // a clockwise edge keeps its angles as seen from below
        let clockwise = arc_edge([0.0, 0.0], 1.0, 0.0, FRAC_PI_2, false);
        let (first, last) = ends(&clockwise);
        assert!(at(first, 1.0, 0.0) && at(last, 0.0, -1.0));
        assert!(anticlockwise
            .iter()
            .chain(&clockwise)
            .all(|p| close(p[0].hypot(p[1]), 1.0)));
    }
}
