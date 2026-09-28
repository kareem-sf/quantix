//! `qx-dwg faces`: every closed region a pile of straight segments encloses, for finding rooms from walls.
//!
//! Reads `{"segments": [[x1, y1, x2, y2], …], "tolerance": t}` on standard input and writes
//! `{"faces": [[[x, y], …], …]}`: each region as a ring of points, counter-clockwise. The segments need not be
//! joined or ordered; `tolerance` is the distance, in drawing units, below which two points are one.

use std::io::Read;

use opencadkernel::geom2d::{bounded_faces, Line, Tolerance};
use serde::Deserialize;
use serde_json::json;

use crate::Failure;

#[derive(Deserialize)]
struct Request {
    segments: Vec<[f64; 4]>,
    tolerance: f64,
}

pub fn run() -> Result<(), Failure> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).map_err(Failure::io)?;
    let request: Request = serde_json::from_str(&input).map_err(|e| Failure::Other(format!("Bad request: {e}")))?;
    let lines: Vec<Line> = request
        .segments
        .iter()
        .filter(|s| s.iter().all(|v| v.is_finite()) && (s[0] != s[2] || s[1] != s[3]))
        .map(|s| Line { start: [s[0], s[1]], end: [s[2], s[3]] })
        .collect();
    let faces = bounded_faces(&lines, Tolerance::new(request.tolerance.max(1e-9)));
    println!("{}", json!({ "faces": faces }));
    Ok(())
}
