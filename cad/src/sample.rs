//! `qx-dwg sample <spec.json> <drawing.dwg|.dxf>`: writes a synthetic drawing, for Quantix's own tests. No customer
//! drawing is ever committed; tests describe the drawing they need and this writes it with the same library that
//! reads it.
//!
//! The spec: `{"insunits": 4, "layers": ["WALLS", …], "blocks": [{"name", "base": [x, y], "entities": […]}],
//! "entities": […], "layouts": [{"name", "entities": […]}]}`. An entity is one of
//! `{"type": "line", "from", "to"}`, `{"type": "polyline", "points", "bulges"?, "closed"?}`,
//! `{"type": "circle", "centre", "radius"}`, `{"type": "arc", "centre", "radius", "start", "end"}` (degrees),
//! `{"type": "text", "at", "value", "height"?}`, `{"type": "mtext", "at", "value", "height"?}`,
//! `{"type": "insert", "block", "at", "rotation"?, "scale"?, "attributes"?: {tag: value}}`,
//! `{"type": "hatch", "loops": [[[x, y], …], …], "pattern"?}`, `{"type": "dimension", "from", "to", "text"?}` and
//! `{"type": "viewport", "centre", "size": [w, h], "view_centre", "scale"}`, each with an optional `"layer"`.

use std::fs;

use opencadcodec::entities::{
    Arc, AttributeEntity, BoundaryEdge, BoundaryPath, Circle, Dimension, DimensionAligned, EntityType, Hatch, Insert, Line,
    LwPolyline, MText, PolylineEdge, Text, Viewport,
};
use opencadcodec::tables::{BlockRecord, Layer};
use opencadcodec::types::{DxfVersion, Vector2, Vector3};
use opencadcodec::{CadDocument, DwgWriter, DxfWriter};
use serde_json::Value;

use crate::Failure;

pub fn run(spec: &str, out: &str) -> Result<(), Failure> {
    let text = fs::read_to_string(spec).map_err(Failure::io)?;
    let spec: Value = serde_json::from_str(&text).map_err(|e| Failure::Other(format!("Bad spec: {e}")))?;
    let mut doc = CadDocument::with_version(DxfVersion::AC1024);
    doc.header.insertion_units = spec["insunits"].as_i64().unwrap_or(4) as i16;
    for name in spec["layers"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if doc.layers.get(name).is_none() {
            let mut layer = Layer::new(name);
            layer.handle = doc.allocate_handle();
            doc.layers.add(layer).map_err(Failure::Other)?;
        }
    }
    for block in spec["blocks"].as_array().into_iter().flatten() {
        let name = block["name"].as_str().unwrap_or("BLOCK");
        let mut record = BlockRecord::new(name);
        record.handle = doc.allocate_handle();
        record.block_entity_handle = doc.allocate_handle();
        record.block_end_handle = doc.allocate_handle();
        let base = point(&block["base"]);
        record.base_point = Vector3::new(base[0], base[1], 0.0);
        let owner = record.handle;
        doc.block_records.add(record).map_err(Failure::Other)?;
        for item in block["entities"].as_array().into_iter().flatten() {
            let mut entity = build(item)?;
            entity.common_mut().owner_handle = owner;
            let handle = doc.add_entity(entity).map_err(|e| Failure::Other(e.to_string()))?;
            if let Some(record) = doc.block_records.get_mut(name) {
                if !record.entity_handles.contains(&handle) {
                    record.entity_handles.push(handle);
                }
            }
        }
    }
    for item in spec["entities"].as_array().into_iter().flatten() {
        doc.add_entity(build(item)?).map_err(|e| Failure::Other(e.to_string()))?;
    }
    for layout in spec["layouts"].as_array().into_iter().flatten() {
        let name = layout["name"].as_str().unwrap_or("Layout");
        if doc.add_layout(name).is_err() {
            // The document's first layout already exists: use it.
        }
        for item in layout["entities"].as_array().into_iter().flatten() {
            doc.add_entity_to_layout(build(item)?, name).map_err(|e| Failure::Other(e.to_string()))?;
        }
    }
    let written = if out.to_ascii_lowercase().ends_with(".dxf") {
        DxfWriter::new(&doc).write_to_file(out)
    } else {
        DwgWriter::write_to_file(out, &doc)
    };
    written.map_err(|e| Failure::Other(format!("Couldn't write the drawing: {e}")))
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap_or(0.0), value[1].as_f64().unwrap_or(0.0)]
}

fn v3(value: &Value) -> Vector3 {
    let p = point(value);
    Vector3::new(p[0], p[1], 0.0)
}

fn number(value: &Value, default: f64) -> f64 {
    value.as_f64().unwrap_or(default)
}

fn build(item: &Value) -> Result<EntityType, Failure> {
    let layer = item["layer"].as_str().unwrap_or("0").to_string();
    let mut entity = match item["type"].as_str().unwrap_or("") {
        "line" => {
            let (a, b) = (point(&item["from"]), point(&item["to"]));
            EntityType::Line(Line::from_coords(a[0], a[1], 0.0, b[0], b[1], 0.0))
        }
        "polyline" => {
            let mut polyline = LwPolyline::new();
            let bulges = item["bulges"].as_array();
            for (i, p) in item["points"].as_array().into_iter().flatten().enumerate() {
                let bulge = bulges.and_then(|b| b.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
                let p = point(p);
                polyline.add_point_with_bulge(Vector2::new(p[0], p[1]), bulge);
            }
            polyline.is_closed = item["closed"].as_bool().unwrap_or(false);
            EntityType::LwPolyline(polyline)
        }
        "circle" => EntityType::Circle(Circle::from_center_radius(v3(&item["centre"]), number(&item["radius"], 1.0))),
        "arc" => {
            let mut arc = Arc::new();
            arc.center = v3(&item["centre"]);
            arc.radius = number(&item["radius"], 1.0);
            arc.start_angle = number(&item["start"], 0.0).to_radians();
            arc.end_angle = number(&item["end"], 90.0).to_radians();
            EntityType::Arc(arc)
        }
        "text" => EntityType::Text(
            Text::with_value(item["value"].as_str().unwrap_or(""), v3(&item["at"])).with_height(number(&item["height"], 250.0)),
        ),
        "mtext" => EntityType::MText(
            MText::with_value(item["value"].as_str().unwrap_or(""), v3(&item["at"])).with_height(number(&item["height"], 250.0)),
        ),
        "insert" => {
            let scale = &item["scale"];
            let (sx, sy) = if scale.is_array() { (number(&scale[0], 1.0), number(&scale[1], 1.0)) } else { (number(scale, 1.0), number(scale, 1.0)) };
            let mut insert = Insert::new(item["block"].as_str().unwrap_or("BLOCK"), v3(&item["at"]))
                .with_scale(sx, sy, 1.0)
                .with_rotation(number(&item["rotation"], 0.0).to_radians());
            if let Some(attributes) = item["attributes"].as_object() {
                for (tag, value) in attributes {
                    let mut attribute = AttributeEntity::new(tag.clone(), value.as_str().unwrap_or("").to_string());
                    attribute.set_position(v3(&item["at"]));
                    attribute.set_height(200.0);
                    insert.attributes.push(attribute);
                }
            }
            EntityType::Insert(insert)
        }
        "hatch" => {
            let mut hatch = Hatch::default();
            for ring in item["loops"].as_array().into_iter().flatten() {
                let points: Vec<Vector2> = ring.as_array().into_iter().flatten().map(|p| {
                    let p = point(p);
                    Vector2::new(p[0], p[1])
                }).collect();
                let mut path = BoundaryPath::new();
                path.flags.set_polyline(true);
                path.edges.push(BoundaryEdge::Polyline(PolylineEdge::new(points, true)));
                hatch.paths.push(path);
            }
            let pattern = item["pattern"].as_str().unwrap_or("SOLID");
            hatch.pattern.name = pattern.to_string();
            hatch.is_solid = pattern.eq_ignore_ascii_case("SOLID");
            EntityType::Hatch(hatch)
        }
        "dimension" => {
            let mut dimension = DimensionAligned::new(v3(&item["from"]), v3(&item["to"]));
            if let Some(text) = item["text"].as_str() {
                dimension.base.set_text_override(Some(text.to_string()));
            }
            let (a, b) = (point(&item["from"]), point(&item["to"]));
            dimension.base.text_middle_point = Vector3::new((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0 + 300.0, 0.0);
            EntityType::Dimension(Dimension::Aligned(dimension))
        }
        "viewport" => {
            let size = &item["size"];
            let mut viewport = Viewport::with_size(v3(&item["centre"]), number(&size[0], 400.0), number(&size[1], 280.0));
            viewport.view_center = v3(&item["view_centre"]);
            let scale = number(&item["scale"], 0.01);
            viewport.view_height = viewport.height / scale;
            viewport.id = 2;
            EntityType::Viewport(viewport)
        }
        other => return Err(Failure::Other(format!("Unknown sample entity {other:?}"))),
    };
    entity.common_mut().layer = layer;
    Ok(entity)
}
