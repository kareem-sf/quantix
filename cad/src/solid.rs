//! 3D solids and regions: an object's ACIS body lifted into opencadkernel's B-rep, meshed and measured. A solid's
//! volume is exact for a cylinder or a sphere and otherwise comes from its closed mesh; a region's area from its
//! faces. Their edges seen from above are what the drawing shows of them.

use opencadcodec::entities::acis::{SabReader, SatParser};
use opencadcodec::entities::AcisData;
use opencadkernel::acis::lift;
use opencadkernel::brep::bounds::body_bounds;
use opencadkernel::brep::mass::analytic_mass_properties;
use opencadkernel::brep::mesh::{tessellate, TessellationTolerance};

/// Largest turn between two chords of a curved face, in radians (about 60 chords to a circle).
const TURN: f64 = 0.1;

pub struct Measured {
    /// Edges seen from above, each a chain of points.
    pub edges: Vec<Vec<[f64; 2]>>,
    /// Enclosed volume, cubic drawing units.
    pub volume: f64,
    /// Area of the faces, square drawing units.
    pub area: f64,
}

/// A body's edges and measures, or `None` when its data can't be read or its faces can't all be meshed.
pub fn measure(data: &AcisData) -> Option<Measured> {
    let document = if data.is_binary {
        SabReader::read(&data.sab_data).ok()?
    } else {
        SatParser::parse(&data.sat_data)
            .or_else(|_| SatParser::parse(&AcisData::decode_sat(&data.sat_data)))
            .ok()?
    };
    let (bodies, _) = lift(&document);
    if bodies.is_empty() {
        return None;
    }
    let mut measured = Measured {
        edges: vec![],
        volume: 0.0,
        area: 0.0,
    };
    for body in &bodies {
        let bounds = body_bounds(body)?;
        let size = (0..3)
            .map(|k| (bounds.max[k] - bounds.min[k]).powi(2))
            .sum::<f64>()
            .sqrt();
        let meshed = tessellate(
            body,
            TessellationTolerance::new(TURN, (size * 1e-4).max(1e-9)),
        );
        if !meshed.missing_faces.is_empty() {
            return None;
        }
        let mesh = &meshed.mesh;
        measured.volume += analytic_mass_properties(body)
            .map(|m| m.volume)
            .or_else(|| mesh.mass_properties().map(|(volume, _)| volume))
            .unwrap_or(0.0);
        for t in &mesh.triangles {
            let [a, b, c] = t.map(|i| mesh.positions[i]);
            let (u, v) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            measured.area += (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt() / 2.0;
        }
        measured.edges.extend(
            meshed
                .drawing_edges
                .iter()
                .map(|e| e.positions.iter().map(|p| [p[0], p[1]]).collect::<Vec<_>>())
                .filter(|e: &Vec<[f64; 2]>| e.len() > 1),
        );
    }
    Some(measured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencadcodec::entities::acis::{primitives, SatDocument, SatWriter};
    use std::f64::consts::PI;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * b.abs().max(1.0)
    }

    fn measured(sat: SatDocument) -> Measured {
        measure(&AcisData::from_sat(&SatWriter::write(&sat))).expect("a solid Quantix can measure")
    }

    /// The extent of the edges seen from above.
    fn plan(m: &Measured) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in m.edges.iter().flatten() {
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ];
        }
        b
    }

    #[test]
    fn a_cube_holds_its_side_cubed() {
        let cube = measured(primitives::build_box([0.0, 0.0, 0.0], 2.0, 2.0, 2.0));
        assert!(close(cube.volume, 8.0) && close(cube.area, 24.0));
        let [left, bottom, right, top] = plan(&cube);
        assert!(close(left, -1.0) && close(bottom, -1.0) && close(right, 1.0) && close(top, 1.0));
    }

    #[test]
    fn a_triangular_prism_holds_half_its_box() {
        // legs 3 and 4 (so a hypotenuse of 5), 5 high
        let wedge = measured(primitives::build_wedge([0.0, 0.0, 0.0], 3.0, 4.0, 5.0));
        assert!(close(wedge.volume, 0.5 * 3.0 * 4.0 * 5.0));
        assert!(close(wedge.area, 2.0 * 6.0 + (3.0 + 4.0 + 5.0) * 5.0));
    }

    #[test]
    fn a_pyramid_holds_a_third_of_its_box() {
        // a 6 × 6 base 4 high: each sloping face 5 high
        let pyramid = measured(primitives::build_pyramid([0.0, 0.0, 0.0], 6.0, 4.0));
        assert!(close(pyramid.volume, 36.0 * 4.0 / 3.0));
        assert!(close(pyramid.area, 36.0 + 4.0 * 0.5 * 6.0 * 5.0));
    }

    #[test]
    fn a_cylinder_and_a_sphere_are_measured_exactly() {
        let bar = measured(primitives::build_cylinder([0.0, 0.0, 0.0], 10.0, 500.0));
        assert!(close(bar.volume, PI * 100.0 * 500.0));
        // its faces' area comes from the mesh, a little inside the curve
        let skin = 2.0 * PI * 10.0 * 500.0 + 2.0 * PI * 100.0;
        assert!(bar.area < skin && bar.area > skin * 0.99);
        let ball = measured(primitives::build_sphere([0.0, 0.0, 0.0], 3.0));
        assert!(close(ball.volume, 4.0 / 3.0 * PI * 27.0));
    }

    #[test]
    fn data_that_is_not_a_solid_is_not_measured() {
        assert!(measure(&AcisData::from_sat("not a solid at all")).is_none());
        assert!(measure(&AcisData::from_sat("")).is_none());
        assert!(measure(&AcisData::from_sab(b"ACIS BinaryFile garbage".to_vec())).is_none());
        assert!(measure(&AcisData::from_sab(vec![])).is_none());
    }
}
