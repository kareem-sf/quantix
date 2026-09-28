//! `qx-dwg read <drawing> <folder>`: every object of a DWG or DXF drawing, as Quantix keeps it.
//!
//! Model space is the first space, then each paper layout that holds anything, in tab order. Block references are
//! placed: every object inside a block is written once per placed copy, in the space around it, with the chain of
//! block references it sits in as part of its key, so a count or a length never takes a block's definition for a
//! drawn object. Each object gets its layer (layer "0" inside a block takes the reference's layer, as CAD shows
//! it), its exact length and area in drawing units, its extent, and a chain of points to draw and pick it.
//!
//! The folder gets `drawing.json` (units, spaces, layers, blocks and what couldn't be read), `objects.json` (keys,
//! names, text, block references, dimensions, tables, hatches and viewports) and three little-endian arrays:
//! `index.bin` (u32 × 8 per object: space, type, layer, block + 1, first point, points, flags, parent + 1),
//! `num.bin` (f64 × 6: extent left, bottom, right, top, length, area) and `coords.bin` (f64 pairs; a NaN pair
//! separates the parts of one object).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use opencadcodec::entities::mtext_format::parse_mtext;
use opencadcodec::entities::{AttachmentPoint, BoundaryEdge, EntityType, Hatch, Insert};
use opencadcodec::objects::ObjectType;
use opencadcodec::types::Vector3;
use opencadcodec::{CadDocument, DwgReadOptions, DwgReader, DxfReader, ReadOutcome};
use serde_json::{json, Value};

use crate::geometry::{self as g, Affine, Shape};
use crate::Failure;

/// Bumped whenever what the folder holds changes, so Quantix reads older folders again.
pub const FORMAT: u32 = 1;
pub const READER: &str = "opencadcodec a35f43e, opencadkernel ee41029";

pub const TYPES: [&str; 20] = [
    "Line",
    "Arc",
    "Circle",
    "Ellipse",
    "Polyline",
    "Spline",
    "Hatch",
    "Solid",
    "Point",
    "Insert",
    "Text",
    "MText",
    "Attribute",
    "Dimension",
    "Leader",
    "MultiLeader",
    "Table",
    "Viewport",
    "MLine",
    "Face",
];
const LINE: usize = 0;
const ARC: usize = 1;
const CIRCLE: usize = 2;
const ELLIPSE: usize = 3;
const POLYLINE: usize = 4;
const SPLINE: usize = 5;
const HATCH: usize = 6;
const SOLID: usize = 7;
const POINT: usize = 8;
const INSERT: usize = 9;
const TEXT: usize = 10;
const MTEXT: usize = 11;
const ATTRIBUTE: usize = 12;
const DIMENSION: usize = 13;
const LEADER: usize = 14;
const MULTILEADER: usize = 15;
const TABLE: usize = 16;
const VIEWPORT: usize = 17;
const MLINE: usize = 18;

pub const CLOSED: u32 = 1;
/// Part of a dimension, leader or table's own drawing: shown, never measured.
pub const ANNOTATION: u32 = 2;
/// Inside a placed block.
pub const IN_BLOCK: u32 = 4;
/// Its length comes from a chain of points rather than the exact curve.
pub const APPROXIMATE: u32 = 8;
/// Drawn from another object (a multiline's lines): shown and used to find rooms, never measured on its own.
pub const DERIVED: u32 = 16;
const MAX_DEPTH: u32 = 16;

#[derive(Clone)]
struct Place {
    space: u32,
    xf: Affine,
    path: String,
    layer: Option<String>,
    parent: Option<usize>,
    flags: u32,
    depth: u32,
}

struct Walker<'a> {
    doc: &'a CadDocument,
    block_names: HashMap<u64, String>,
    layers: Vec<String>,
    layer_index: HashMap<String, u32>,
    blocks: Vec<String>,
    block_index: HashMap<String, u32>,
    keys: Vec<String>,
    index: Vec<[u32; 8]>,
    num: Vec<[f64; 6]>,
    coords: Vec<f64>,
    texts: Vec<Value>,
    inserts: Vec<Value>,
    dims: Vec<Value>,
    tables: Vec<Value>,
    hatches: Vec<Value>,
    viewports: Vec<Value>,
    not_read: BTreeMap<&'static str, u64>,
    xrefs: BTreeMap<String, Value>,
}

pub fn run(drawing: &str, folder: &str) -> Result<(), Failure> {
    let outcome = open(Path::new(drawing))?;
    let doc = &outcome.document;
    let mut walker = Walker::new(doc);
    let mut spaces = vec![];
    for (number, (name, block, kind)) in spaces_of(doc).into_iter().enumerate() {
        let first = walker.keys.len();
        let place = Place {
            space: number as u32 + 1,
            xf: Affine::IDENTITY,
            path: String::new(),
            layer: None,
            parent: None,
            flags: 0,
            depth: 0,
        };
        walker.block(&block, &place);
        let objects = walker.keys.len() - first;
        if objects == 0 && kind == "paper" {
            continue; // an empty layout tab is not a sheet
        }
        let extent = walker.extent(first);
        spaces.push(json!({"number": number + 1, "name": name, "block": block, "kind": kind, "extents": extent, "objects": objects}));
    }
    // Pages are numbered in order with no gaps: renumber after skipping empty layouts.
    let mut renumber = HashMap::new();
    for (page, space) in spaces.iter_mut().enumerate() {
        renumber.insert(
            space["number"].as_u64().unwrap_or(0) as u32,
            page as u32 + 1,
        );
        space["number"] = json!(page + 1);
    }
    for row in walker.index.iter_mut() {
        row[0] = renumber.get(&row[0]).copied().unwrap_or(0);
    }
    if walker.keys.is_empty()
        && outcome.stats.decoded_source_records == 0
        && outcome.stats.recovered_errors > 0
    {
        // the failsafe reader gave back an empty document: nothing of the file could be read
        return Err(Failure::Unreadable(
            "This drawing can't be opened: nothing in it could be read.".into(),
        ));
    }
    walker.write(Path::new(folder), &outcome, spaces)
}

fn open(path: &Path) -> Result<ReadOutcome, Failure> {
    let is_dxf = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dxf"));
    let result = if is_dxf {
        DxfReader::from_file(path).and_then(|reader| reader.read_with_stats())
    } else {
        DwgReader::from_file_with_options(path, DwgReadOptions::failsafe())
            .and_then(|mut reader| reader.read_with_stats())
    };
    result.map_err(|error| Failure::Unreadable(format!("This drawing can't be opened: {error}")))
}

/// Model space, then each paper layout in tab order: (name, block, kind).
fn spaces_of(doc: &CadDocument) -> Vec<(String, String, &'static str)> {
    let names: HashMap<u64, String> = doc
        .block_records
        .iter()
        .map(|b| (b.handle.value(), b.name.clone()))
        .collect();
    let mut layouts: Vec<(i16, String, String)> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            ObjectType::Layout(layout) if !layout.name.eq_ignore_ascii_case("model") => names
                .get(&layout.block_record.value())
                .map(|block| (layout.tab_order, layout.name.clone(), block.clone())),
            _ => None,
        })
        .collect();
    layouts.sort();
    let mut found = vec![("Model".to_string(), "*Model_Space".to_string(), "model")];
    found.extend(
        layouts
            .into_iter()
            .map(|(_, name, block)| (name, block, "paper")),
    );
    found
}

fn v2(v: Vector3) -> [f64; 2] {
    [v.x, v.y]
}

fn round(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

/// A measurement as a drawing would print it when nothing overrides it.
fn printed(value: f64) -> String {
    if (value - value.round()).abs() < 1e-6 {
        format!("{}", value.round())
    } else {
        let text = format!("{value:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The words of a TEXT, with its %% codes as the characters they print.
fn plain_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && chars.peek() == Some(&'%') {
            chars.next();
            match chars.next() {
                Some('c' | 'C') => out.push('Ø'),
                Some('d' | 'D') => out.push('°'),
                Some('p' | 'P') => out.push('±'),
                Some('%') => out.push('%'),
                Some('u' | 'U' | 'o' | 'O') => {}
                Some(other) => {
                    out.push_str("%%");
                    out.push(other);
                }
                None => out.push_str("%%"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn mtext_plain(raw: &str) -> String {
    parse_mtext(raw, false).to_plain_text()
}

/// Where a text lies: its anchor, height, rotation, and how much of the box sits left of and below the anchor.
fn text_box(
    anchor: [f64; 2],
    text: &str,
    height: f64,
    width: f64,
    rotation: f64,
    shift: (f64, f64),
) -> [f64; 4] {
    let lines = text.lines().count().max(1) as f64;
    let chars = text
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    let w = if width > 0.0 {
        width
    } else {
        height * 0.8 * chars
    }; // no font metrics: an estimate
    let h = height * if lines > 1.0 { lines * 1.67 } else { 1.0 };
    let (sin, cos) = rotation.sin_cos();
    let mut bbox = None;
    for (x, y) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
        let (x, y) = (x - shift.0 * w, y - shift.1 * h);
        bbox = Some(g::grow(
            bbox,
            [anchor[0] + x * cos - y * sin, anchor[1] + x * sin + y * cos],
        ));
    }
    bbox.unwrap_or([anchor[0], anchor[1], anchor[0], anchor[1]])
}

impl<'a> Walker<'a> {
    fn new(doc: &'a CadDocument) -> Walker<'a> {
        let mut walker = Walker {
            doc,
            block_names: doc
                .block_records
                .iter()
                .map(|b| (b.handle.value(), b.name.clone()))
                .collect(),
            layers: vec![],
            layer_index: HashMap::new(),
            blocks: vec![],
            block_index: HashMap::new(),
            keys: vec![],
            index: vec![],
            num: vec![],
            coords: vec![],
            texts: vec![],
            inserts: vec![],
            dims: vec![],
            tables: vec![],
            hatches: vec![],
            viewports: vec![],
            not_read: BTreeMap::new(),
            xrefs: BTreeMap::new(),
        };
        for layer in doc.layers.iter() {
            walker.layer(&layer.name);
        }
        walker
    }

    fn layer(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.layer_index.get(name) {
            return i;
        }
        let i = self.layers.len() as u32;
        self.layers.push(name.to_string());
        self.layer_index.insert(name.to_string(), i);
        i
    }

    fn block_id(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.block_index.get(name) {
            return i;
        }
        let i = self.blocks.len() as u32;
        self.blocks.push(name.to_string());
        self.block_index.insert(name.to_string(), i);
        i
    }

    fn skipped(&mut self, what: &'static str) {
        *self.not_read.entry(what).or_insert(0) += 1;
    }

    /// Every object of a block (or a space), placed.
    fn block(&mut self, name: &str, place: &Place) {
        let doc = self.doc;
        for entity in doc.entities_in_block(name) {
            self.entity(entity, place);
        }
    }

    fn extent(&self, first: usize) -> Option<[f64; 4]> {
        let mut found = None;
        for (row, num) in self.index[first..].iter().zip(&self.num[first..]) {
            if row[6] & ANNOTATION == 0 && num[0].is_finite() {
                found = Some(g::union(found, [num[0], num[1], num[2], num[3]]));
            }
        }
        found.or_else(|| {
            let mut all = None;
            for num in &self.num[first..] {
                if num[0].is_finite() {
                    all = Some(g::union(all, [num[0], num[1], num[2], num[3]]));
                }
            }
            all
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        place: &Place,
        handle: u64,
        kind: usize,
        layer: &str,
        block: Option<&str>,
        shape: Option<&Shape>,
        bbox: Option<[f64; 4]>,
        flags: u32,
    ) -> usize {
        let i = self.keys.len();
        self.keys.push(format!("{}{:X}", place.path, handle));
        let start = (self.coords.len() / 2) as u32;
        let mut count = 0u32;
        let mut flags = place.flags | flags;
        let (mut length, mut area, mut extent) = (0.0, 0.0, bbox);
        if let Some(shape) = shape {
            for (k, part) in shape.parts.iter().enumerate() {
                if k > 0 {
                    self.coords.extend([f64::NAN, f64::NAN]);
                    count += 1;
                }
                for p in part {
                    self.coords.extend(p);
                    count += 1;
                }
            }
            length = shape.length;
            area = shape.area;
            if shape.closed {
                flags |= CLOSED;
            }
            if shape.approximate {
                flags |= APPROXIMATE;
            }
            extent = shape.bbox().or(extent);
        }
        let layer = self.layer(layer);
        let block = block.map(|b| self.block_id(b) + 1).unwrap_or(0);
        let parent = place.parent.map(|p| p as u32 + 1).unwrap_or(0);
        self.index.push([
            place.space,
            kind as u32,
            layer,
            block,
            start,
            count,
            flags,
            parent,
        ]);
        let e = extent.unwrap_or([f64::NAN; 4]);
        self.num
            .push([e[0], e[1], e[2], e[3], round(length), round(area)]);
        i
    }

    fn shape(
        &mut self,
        place: &Place,
        handle: u64,
        kind: usize,
        layer: &str,
        shape: Shape,
    ) -> usize {
        let placed = shape.transformed(&place.xf);
        self.push(place, handle, kind, layer, None, Some(&placed), None, 0)
    }

    fn text(
        &mut self,
        place: &Place,
        handle: u64,
        kind: usize,
        layer: &str,
        found: TextFound,
    ) -> usize {
        let anchor = place.xf.apply(found.anchor);
        let scale = place.xf.det().abs().sqrt();
        let height = found.height * scale;
        let rotation = found.rotation + place.xf.angle();
        let bbox = text_box(
            anchor,
            &found.plain,
            height,
            found.width * scale,
            rotation,
            found.shift,
        );
        let i = self.push(place, handle, kind, layer, None, None, Some(bbox), 0);
        self.texts.push(json!([
            i,
            found.plain,
            (found.raw != found.plain).then_some(found.raw),
            round(anchor[0]),
            round(anchor[1]),
            round(height),
            round(rotation.to_degrees()),
            found.tag
        ]));
        i
    }

    fn entity(&mut self, entity: &EntityType, place: &Place) {
        let common = entity.common();
        if common.invisible {
            return;
        }
        let handle = common.handle.value();
        let layer = match &place.layer {
            Some(inherited) if common.layer == "0" => inherited.clone(),
            _ => common.layer.clone(),
        };
        let layer = layer.as_str();
        match entity {
            EntityType::Line(e) => {
                let shape = g::line(v2(e.start), v2(e.end));
                self.shape(place, handle, LINE, layer, shape);
            }
            EntityType::Arc(e) => {
                let shape = g::arc(v2(e.center), e.radius, e.start_angle, e.end_angle)
                    .transformed(&Affine::ocs(e.normal));
                self.shape(place, handle, ARC, layer, shape);
            }
            EntityType::Circle(e) => {
                let shape = g::circle(v2(e.center), e.radius).transformed(&Affine::ocs(e.normal));
                self.shape(place, handle, CIRCLE, layer, shape);
            }
            EntityType::Ellipse(e) => {
                let shape = g::ellipse(
                    v2(e.center),
                    v2(e.major_axis),
                    e.minor_axis_ratio,
                    e.start_parameter,
                    e.end_parameter,
                    e.normal.z < 0.0,
                );
                self.shape(place, handle, ELLIPSE, layer, shape);
            }
            EntityType::LwPolyline(e) => {
                let vertices: Vec<([f64; 2], f64)> = e
                    .vertices
                    .iter()
                    .map(|v| ([v.location.x, v.location.y], v.bulge))
                    .collect();
                let shape = g::polyline(&vertices, e.is_closed).transformed(&Affine::ocs(e.normal));
                self.shape(place, handle, POLYLINE, layer, shape);
            }
            EntityType::Polyline2D(e) => {
                let vertices: Vec<([f64; 2], f64)> = e
                    .vertices
                    .iter()
                    .filter(|v| v.flags.bits() & 16 == 0)
                    .map(|v| ([v.location.x, v.location.y], v.bulge))
                    .collect();
                let closed = e.flags.is_closed();
                let shape = g::polyline(&vertices, closed).transformed(&Affine::ocs(e.normal));
                self.shape(place, handle, POLYLINE, layer, shape);
            }
            EntityType::Polyline(e) => {
                let vertices: Vec<([f64; 2], f64)> = e
                    .vertices
                    .iter()
                    .map(|v| ([v.location.x, v.location.y], 0.0))
                    .collect();
                let closed = e.flags.is_closed();
                self.shape(
                    place,
                    handle,
                    POLYLINE,
                    layer,
                    g::polyline(&vertices, closed),
                );
            }
            EntityType::Polyline3D(e) => {
                let vertices: Vec<([f64; 2], f64)> = e
                    .vertices
                    .iter()
                    .map(|v| ([v.position.x, v.position.y], 0.0))
                    .collect();
                self.shape(
                    place,
                    handle,
                    POLYLINE,
                    layer,
                    g::polyline(&vertices, e.is_closed()),
                );
            }
            EntityType::Spline(e) => {
                let control: Vec<[f64; 2]> = e.control_points.iter().map(|p| v2(*p)).collect();
                let fit: Vec<[f64; 2]> = e.fit_points.iter().map(|p| v2(*p)).collect();
                let shape = g::spline(
                    e.degree.max(1) as usize,
                    &control,
                    &e.knots,
                    &e.weights,
                    &fit,
                    e.flags.closed,
                );
                self.shape(place, handle, SPLINE, layer, shape);
            }
            EntityType::Hatch(e) => self.hatch(e, place, handle, layer),
            EntityType::Solid(e) => {
                let corners = [
                    e.first_corner,
                    e.second_corner,
                    e.fourth_corner,
                    e.third_corner,
                ];
                let mut ring: Vec<[f64; 2]> = corners.iter().map(|c| v2(*c)).collect();
                ring.dedup();
                let shape = g::rings(vec![ring], false).transformed(&Affine::ocs(e.normal));
                self.shape(place, handle, SOLID, layer, shape);
            }
            EntityType::Point(e) => {
                let p = v2(e.location);
                let shape = Shape {
                    parts: vec![vec![p]],
                    ..Shape::default()
                };
                self.shape(place, handle, POINT, layer, shape);
            }
            EntityType::Insert(e) => self.insert(e, place, handle, layer),
            EntityType::Text(e) => {
                let aligned = e.alignment_point.filter(|_| {
                    !matches!(format!("{:?}", e.horizontal_alignment).as_str(), "Left")
                        || !matches!(format!("{:?}", e.vertical_alignment).as_str(), "Baseline")
                });
                let anchor = Affine::ocs(e.normal).apply(v2(aligned.unwrap_or(e.insertion_point)));
                let plain = plain_text(&e.value);
                let found = TextFound {
                    anchor,
                    height: e.height,
                    width: 0.0,
                    rotation: e.rotation,
                    shift: (0.0, 0.0),
                    raw: e.value.clone(),
                    plain,
                    tag: None,
                };
                self.text(place, handle, TEXT, layer, found);
            }
            EntityType::MText(e) => {
                let found = TextFound {
                    anchor: v2(e.insertion_point),
                    height: e.height,
                    width: e.rectangle_width,
                    rotation: e.rotation,
                    shift: attachment(e.attachment_point),
                    raw: e.value.clone(),
                    plain: mtext_plain(&e.value),
                    tag: None,
                };
                self.text(place, handle, MTEXT, layer, found);
            }
            EntityType::Dimension(e) => self.dimension(e, place, handle, layer),
            EntityType::Leader(e) => {
                let vertices: Vec<([f64; 2], f64)> =
                    e.vertices.iter().map(|v| (v2(*v), 0.0)).collect();
                let shape = g::polyline(&vertices, false).transformed(&place.xf);
                self.push(
                    place,
                    handle,
                    LEADER,
                    layer,
                    None,
                    Some(&shape),
                    None,
                    ANNOTATION,
                );
            }
            EntityType::MultiLeader(e) => {
                let i = self.push(
                    place,
                    handle,
                    MULTILEADER,
                    layer,
                    None,
                    None,
                    None,
                    ANNOTATION,
                );
                let inner = Place {
                    path: format!("{}{:X}/", place.path, handle),
                    parent: Some(i),
                    flags: place.flags | ANNOTATION,
                    ..place.clone()
                };
                for part in entity.explode() {
                    self.entity(
                        &part,
                        &Place {
                            layer: Some(layer.to_string()),
                            ..inner.clone()
                        },
                    );
                }
                self.close_extent(i);
                let _ = e;
            }
            EntityType::Table(e) => {
                let mut rows = vec![];
                for r in 0..e.row_count() {
                    let row: Vec<String> = (0..e.column_count())
                        .map(|c| {
                            e.cell(r, c)
                                .map(|cell| mtext_plain(cell.text_value()))
                                .unwrap_or_default()
                        })
                        .collect();
                    rows.push(row);
                }
                let at = place.xf.apply(v2(e.insertion_point));
                let i = self.push(
                    place,
                    handle,
                    TABLE,
                    layer,
                    None,
                    None,
                    Some([at[0], at[1], at[0], at[1]]),
                    0,
                );
                self.tables.push(json!([i, rows]));
                if !e.block_name.is_empty() && self.doc.block_records.get(&e.block_name).is_some() {
                    let inner = Place {
                        path: format!("{}{:X}/", place.path, handle),
                        parent: Some(i),
                        layer: Some(layer.to_string()),
                        flags: place.flags | ANNOTATION,
                        depth: place.depth + 1,
                        ..place.clone()
                    };
                    self.block(&e.block_name.clone(), &inner);
                    self.close_extent(i);
                }
            }
            EntityType::Viewport(e) => {
                if place.depth > 0 {
                    return;
                }
                let (cx, cy, w, h) = (e.center.x, e.center.y, e.width, e.height);
                let ring = vec![
                    [cx - w / 2.0, cy - h / 2.0],
                    [cx + w / 2.0, cy - h / 2.0],
                    [cx + w / 2.0, cy + h / 2.0],
                    [cx - w / 2.0, cy + h / 2.0],
                ];
                let shape = Shape {
                    parts: vec![ring],
                    closed: true,
                    ..Shape::default()
                };
                let i = self.push(
                    place,
                    handle,
                    VIEWPORT,
                    layer,
                    None,
                    Some(&shape),
                    None,
                    ANNOTATION,
                );
                let scale = if e.view_height.abs() > 1e-12 {
                    e.height / e.view_height
                } else {
                    0.0
                };
                self.viewports.push(json!([
                    i,
                    round(cx),
                    round(cy),
                    round(w),
                    round(h),
                    round(e.view_center.x),
                    round(e.view_center.y),
                    round(e.view_height),
                    scale,
                    e.status.is_on,
                    e.id
                ]));
            }
            EntityType::MLine(e) => {
                let vertices: Vec<([f64; 2], f64)> =
                    e.vertices.iter().map(|v| (v2(v.position), 0.0)).collect();
                let closed = format!("{:?}", e.flags).contains("CLOSED")
                    || format!("{:?}", e.flags).contains("closed: true");
                let i = self.shape(place, handle, MLINE, layer, g::polyline(&vertices, closed));
                let inner = Place {
                    path: format!("{}{:X}/", place.path, handle),
                    parent: Some(i),
                    flags: place.flags | DERIVED,
                    ..place.clone()
                };
                for part in entity.explode() {
                    self.entity(
                        &part,
                        &Place {
                            layer: Some(layer.to_string()),
                            ..inner.clone()
                        },
                    );
                }
            }
            EntityType::Face3D(_) => self.skipped("3D faces"),
            EntityType::Solid3D(_)
            | EntityType::Region(_)
            | EntityType::Body(_)
            | EntityType::Surface(_) => self.skipped("3D solids and surfaces"),
            EntityType::Mesh(_) | EntityType::PolyfaceMesh(_) | EntityType::PolygonMesh(_) => {
                self.skipped("3D meshes")
            }
            EntityType::RasterImage(_) | EntityType::Underlay(_) | EntityType::Ole2Frame(_) => {
                self.skipped("images, underlays and embedded objects")
            }
            EntityType::Unknown(_) | EntityType::Extended(_) => {
                self.skipped("objects of kinds Quantix can't read")
            }
            EntityType::Ray(_) | EntityType::XLine(_) => self.skipped("construction lines"),
            _ => {} // block markers, attribute definitions, wipeouts, shapes and other things with nothing to measure
        }
    }

    /// The extent of an object that holds others: all of theirs together.
    fn close_extent(&mut self, i: usize) {
        let mut found = None;
        for num in &self.num[i + 1..] {
            if num[0].is_finite() {
                found = Some(g::union(found, [num[0], num[1], num[2], num[3]]));
            }
        }
        if let Some(e) = found {
            let own = self.num[i];
            let e = if own[0].is_finite() {
                g::union(Some([own[0], own[1], own[2], own[3]]), e)
            } else {
                e
            };
            self.num[i][..4].copy_from_slice(&e);
        }
    }

    fn hatch(&mut self, e: &Hatch, place: &Place, handle: u64, layer: &str) {
        let mut parts = vec![];
        let mut curved = false;
        for path in &e.paths {
            let mut ring: Vec<[f64; 2]> = vec![];
            for edge in &path.edges {
                let points: Vec<[f64; 2]> = match edge {
                    BoundaryEdge::Line(l) => vec![[l.start.x, l.start.y], [l.end.x, l.end.y]],
                    BoundaryEdge::CircularArc(a) => {
                        curved = true;
                        g::arc_edge(
                            [a.center.x, a.center.y],
                            a.radius,
                            a.start_angle,
                            a.end_angle,
                            a.counter_clockwise,
                        )
                    }
                    BoundaryEdge::EllipticArc(a) => {
                        curved = true;
                        let (start, end) = if a.counter_clockwise {
                            (a.start_angle, a.end_angle)
                        } else {
                            (-a.end_angle, -a.start_angle)
                        };
                        let s = g::ellipse(
                            [a.center.x, a.center.y],
                            [a.major_axis_endpoint.x, a.major_axis_endpoint.y],
                            a.minor_axis_ratio,
                            start,
                            end,
                            false,
                        );
                        let mut points = s.parts.into_iter().next().unwrap_or_default();
                        if !a.counter_clockwise {
                            points.reverse();
                        }
                        points
                    }
                    BoundaryEdge::Spline(s) => {
                        curved = true;
                        let control: Vec<[f64; 2]> =
                            s.control_points.iter().map(|p| [p.x, p.y]).collect();
                        let weights: Vec<f64> = if s.rational {
                            s.control_points.iter().map(|p| p.z).collect()
                        } else {
                            vec![]
                        };
                        let fit: Vec<[f64; 2]> = s.fit_points.iter().map(|p| [p.x, p.y]).collect();
                        g::spline(
                            s.degree.max(1) as usize,
                            &control,
                            &s.knots,
                            &weights,
                            &fit,
                            false,
                        )
                        .parts
                        .into_iter()
                        .next()
                        .unwrap_or_default()
                    }
                    BoundaryEdge::Polyline(p) => {
                        let vertices: Vec<([f64; 2], f64)> =
                            p.vertices.iter().map(|v| ([v.x, v.y], v.z)).collect();
                        curved |= vertices.iter().any(|v| v.1 != 0.0);
                        g::bulge_chain(&vertices, p.is_closed)
                    }
                };
                for p in points {
                    if ring.last() != Some(&p) {
                        ring.push(p);
                    }
                }
            }
            if ring.len() > 2 && ring.first() == ring.last() {
                ring.pop();
            }
            if ring.len() >= 3 {
                parts.push(ring);
            }
        }
        if parts.is_empty() {
            return;
        }
        let shape = g::rings(parts, curved).transformed(&Affine::ocs(e.normal));
        let i = self.shape(place, handle, HATCH, layer, shape);
        self.hatches.push(json!([i, e.pattern.name, e.is_solid]));
    }

    fn dimension(
        &mut self,
        e: &opencadcodec::entities::Dimension,
        place: &Place,
        handle: u64,
        layer: &str,
    ) {
        let base = e.base();
        let measured = e.measurement();
        let override_text = base
            .text_override()
            .filter(|t| !t.is_empty() && *t != "<>")
            .map(|t| t.to_string());
        let shown = match &override_text {
            None => printed(measured),
            Some(t) if t.contains("<>") => mtext_plain(&t.replace("<>", &printed(measured))),
            Some(t) => mtext_plain(t),
        };
        let at = place.xf.apply(v2(base.text_middle_point));
        let i = self.push(
            place,
            handle,
            DIMENSION,
            layer,
            None,
            None,
            Some([at[0], at[1], at[0], at[1]]),
            0,
        );
        let kind = format!("{:?}", base.dimension_type);
        self.dims.push(json!([
            i,
            round(measured),
            shown,
            override_text,
            round(base.actual_measurement),
            round(at[0]),
            round(at[1]),
            kind
        ]));
        if !base.block_name.is_empty()
            && place.depth < MAX_DEPTH
            && self.doc.block_records.get(&base.block_name).is_some()
        {
            let inner = Place {
                path: format!("{}{:X}/", place.path, handle),
                parent: Some(i),
                layer: Some(layer.to_string()),
                flags: place.flags | ANNOTATION,
                depth: place.depth + 1,
                ..place.clone()
            };
            self.block(&base.block_name.clone(), &inner);
            self.close_extent(i);
        }
    }

    fn insert(&mut self, e: &Insert, place: &Place, handle: u64, layer: &str) {
        let doc = self.doc;
        let Some(record) = doc.block_records.get(&e.block_name) else {
            self.skipped("references to blocks the drawing doesn't define");
            return;
        };
        // An anonymous copy of a dynamic block is counted under the block it came from.
        let name = doc
            .dynamic_definition_for_insert(e.common.handle)
            .and_then(|h| self.block_names.get(&h.value()).cloned())
            .filter(|n| !n.starts_with('*'))
            .unwrap_or_else(|| e.block_name.clone());
        let is_xref =
            record.flags.is_xref || record.flags.is_xref_overlay || !record.xref_path.is_empty();
        if is_xref {
            let loaded = !record.entity_handles.is_empty();
            self.xrefs.insert(
                name.clone(),
                json!({"name": name, "path": record.xref_path, "loaded": loaded}),
            );
        }
        let instances = if e.is_minsert() {
            e.instance_count().max(1)
        } else {
            1
        };
        let ocs = Affine::ocs(e.normal);
        let base = Affine::translate(-record.base_point.x, -record.base_point.y);
        let at = ocs.apply(v2(e.insert_point));
        let own = place
            .xf
            .then(&ocs)
            .then(&Affine::placed(
                e.insert_point.x,
                e.insert_point.y,
                e.rotation,
                e.x_scale(),
                e.y_scale(),
            ))
            .then(&base);
        let world = place.xf.apply(at);
        let i = self.push(
            place,
            handle,
            INSERT,
            layer,
            Some(&name),
            None,
            Some([world[0], world[1], world[0], world[1]]),
            0,
        );
        let mut attributes = serde_json::Map::new();
        for attribute in &e.attributes {
            if attribute.common.invisible {
                continue;
            }
            let value = mtext_plain(&attribute.value);
            attributes.insert(attribute.tag.clone(), json!(value));
            let found = TextFound {
                anchor: v2(attribute.insertion_point),
                height: attribute.height,
                width: 0.0,
                rotation: attribute.rotation,
                shift: (0.0, 0.0),
                raw: attribute.value.clone(),
                plain: value,
                tag: Some(attribute.tag.clone()),
            };
            let attribute_layer = if attribute.common.layer == "0" {
                layer.to_string()
            } else {
                attribute.common.layer.clone()
            };
            let inner = Place {
                path: format!("{}{:X}/", place.path, handle),
                parent: Some(i),
                flags: place.flags | IN_BLOCK,
                ..place.clone()
            };
            self.text(
                &inner,
                attribute.common.handle.value(),
                ATTRIBUTE,
                &attribute_layer,
                found,
            );
        }
        self.inserts.push(json!([
            i,
            name,
            attributes,
            round(world[0]),
            round(world[1]),
            round((e.rotation + place.xf.angle()).to_degrees()),
            round(e.x_scale()),
            round(e.y_scale()),
            instances,
            e.block_name
        ]));
        if place.depth >= MAX_DEPTH {
            self.skipped("blocks nested too deep to place");
            return;
        }
        let (rows, columns) = if instances > 1 {
            (e.row_count.max(1), e.column_count.max(1))
        } else {
            (1, 1)
        };
        let block_name = e.block_name.clone();
        for row in 0..rows {
            for column in 0..columns {
                let cell = if instances > 1 {
                    let (sin, cos) = e.rotation.sin_cos();
                    let (dx, dy) = (column as f64 * e.column_spacing, row as f64 * e.row_spacing);
                    let shift = Affine::translate(dx * cos - dy * sin, dx * sin + dy * cos);
                    place
                        .xf
                        .then(&ocs)
                        .then(&shift)
                        .then(&Affine::placed(
                            e.insert_point.x,
                            e.insert_point.y,
                            e.rotation,
                            e.x_scale(),
                            e.y_scale(),
                        ))
                        .then(&base)
                } else {
                    own
                };
                let path = if instances > 1 {
                    format!(
                        "{}{:X}#{}/",
                        place.path,
                        handle,
                        row as usize * columns as usize + column as usize
                    )
                } else {
                    format!("{}{:X}/", place.path, handle)
                };
                let inner = Place {
                    space: place.space,
                    xf: cell,
                    path,
                    layer: Some(layer.to_string()),
                    parent: Some(i),
                    flags: place.flags | IN_BLOCK,
                    depth: place.depth + 1,
                };
                self.block(&block_name, &inner);
            }
        }
        self.close_extent(i);
    }

    fn write(
        self,
        folder: &Path,
        outcome: &ReadOutcome,
        spaces: Vec<Value>,
    ) -> Result<(), Failure> {
        let doc = &outcome.document;
        let staging = folder.with_extension("reading");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(Failure::io)?;
        let layers: Vec<Value> = self
            .layers
            .iter()
            .map(|name| match doc.layers.get(name) {
                Some(l) => json!({"name": name, "off": l.flags.off, "frozen": l.flags.frozen, "locked": l.flags.locked, "linetype": l.line_type}),
                None => json!({"name": name, "off": false, "frozen": false, "locked": false, "linetype": ""}),
            })
            .collect();
        let blocks: Vec<Value> = self
            .blocks
            .iter()
            .map(|name| match doc.block_records.get(name) {
                Some(b) => json!({"name": name, "anonymous": b.flags.anonymous, "xref": b.flags.is_xref || !b.xref_path.is_empty(), "description": b.description, "units": b.units}),
                None => json!({"name": name, "anonymous": name.starts_with('*'), "xref": false, "description": "", "units": 0}),
            })
            .collect();
        // Each clip hangs off its block reference through the reference's own dictionaries: find the reference.
        let mut clipped_inserts: Vec<String> = vec![];
        let mut clipped = 0;
        for object in doc.objects.values() {
            let ObjectType::SpatialFilter(filter) = object else {
                continue;
            };
            clipped += 1;
            let mut owner = Some(filter.owner);
            for _ in 0..4 {
                let Some(h) = owner else { break };
                if let Some(EntityType::Insert(_)) = doc.get_entity(h) {
                    clipped_inserts.push(format!("{:X}", h.value()));
                    break;
                }
                owner = doc.object_owner(h);
            }
        }
        let stats = &outcome.stats;
        let diagnostics: Vec<String> = stats
            .diagnostics
            .iter()
            .take(20)
            .map(|d| d.message.chars().take(300).collect())
            .collect();
        let drawing = json!({
            "format": FORMAT,
            "reader": READER,
            "version": format!("{:?}", doc.version),
            "units": {"insunits": doc.header.insertion_units, "measurement": doc.header.measurement},
            "dimlfac": doc.header.dim_linear_scale,
            "dimscale": doc.header.dim_scale,
            "spaces": spaces,
            "layers": layers,
            "blocks": blocks,
            "xrefs": self.xrefs.values().collect::<Vec<_>>(),
            "read": {
                "records": stats.source_records,
                "decoded": stats.decoded_source_records,
                "skipped": stats.skipped_source_records,
                "recovered": stats.recovered_errors,
                "diagnostics": diagnostics,
                "not_read": self.not_read,
                "clipped": clipped,
                "clipped_inserts": clipped_inserts,
            },
        });
        let objects = json!({
            "types": TYPES,
            "layers": self.layers,
            "blocks": self.blocks,
            "keys": self.keys,
            "texts": self.texts,
            "inserts": self.inserts,
            "dims": self.dims,
            "tables": self.tables,
            "hatches": self.hatches,
            "viewports": self.viewports,
        });
        write_json(&staging.join("drawing.json"), &drawing)?;
        write_json(&staging.join("objects.json"), &objects)?;
        let mut index =
            BufWriter::new(fs::File::create(staging.join("index.bin")).map_err(Failure::io)?);
        for row in &self.index {
            for v in row {
                index.write_all(&v.to_le_bytes()).map_err(Failure::io)?;
            }
        }
        index.flush().map_err(Failure::io)?;
        let mut num =
            BufWriter::new(fs::File::create(staging.join("num.bin")).map_err(Failure::io)?);
        for row in &self.num {
            for v in row {
                num.write_all(&v.to_le_bytes()).map_err(Failure::io)?;
            }
        }
        num.flush().map_err(Failure::io)?;
        let mut coords =
            BufWriter::new(fs::File::create(staging.join("coords.bin")).map_err(Failure::io)?);
        for v in &self.coords {
            coords.write_all(&v.to_le_bytes()).map_err(Failure::io)?;
        }
        coords.flush().map_err(Failure::io)?;
        drop((index, num, coords));
        let _ = fs::remove_dir_all(folder);
        fs::rename(&staging, folder).map_err(Failure::io)?;
        Ok(())
    }
}

struct TextFound {
    anchor: [f64; 2],
    height: f64,
    width: f64,
    rotation: f64,
    /// How much of the text's box lies left of and below its anchor, as shares of its width and height.
    shift: (f64, f64),
    raw: String,
    plain: String,
    tag: Option<String>,
}

fn attachment(point: AttachmentPoint) -> (f64, f64) {
    let name = format!("{point:?}");
    let x = if name.ends_with("Center") {
        0.5
    } else if name.ends_with("Right") {
        1.0
    } else {
        0.0
    };
    let y = if name.starts_with("Top") {
        1.0
    } else if name.starts_with("Middle") {
        0.5
    } else {
        0.0
    };
    (x, y)
}

fn write_json(path: &Path, value: &Value) -> Result<(), Failure> {
    let file = fs::File::create(path).map_err(Failure::io)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, value).map_err(|e| Failure::Other(e.to_string()))?;
    writer.flush().map_err(Failure::io)
}
