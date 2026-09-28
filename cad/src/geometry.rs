//! Plane geometry for takeoff: where a drawing's objects lie in the space they are drawn in, and their exact
//! lengths and areas. Straight lines, arcs, circles and bulged polylines are measured exactly by opencadkernel;
//! ellipses and splines from a fine chain of points. Points are kept only to draw, pick and compare objects.

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
    if ring.len() < 3 {
        return 0.0;
    }
    let o = ring[0];
    let mut twice = 0.0;
    for i in 1..ring.len() - 1 {
        let (p, q) = (ring[i], ring[i + 1]);
        twice += (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1]);
    }
    (twice / 2.0).abs()
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_circle_encloses_pi_r_squared() {
        assert!(close(
            circle([5.0e5, 5.0e5], 2.0).area,
            std::f64::consts::PI * 4.0
        ));
    }
}
