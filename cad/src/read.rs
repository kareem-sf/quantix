//! `qx-dwg read <drawing> <folder>`: every object of a DWG or DXF drawing, as Quantix keeps it.
//!
//! Model space is the first space, then each paper layout that holds anything, in tab order. Block references are
//! placed: every object inside a block is written once per placed copy, in the space around it, with the chain of
//! block references it sits in as part of its key, so a count or a length never takes a block's definition for a
//! drawn object. Each object gets its layer (layer "0" inside a block takes the reference's layer, as CAD shows
//! it), its exact length and area in drawing units, its extent, and a chain of points to draw and pick it. A
//! clipped reference keeps only what shows inside its clip boundary. A drawing it refers to (an xref) that Quantix
//! gives it is placed like a block, its layers and blocks named after it ("BASE|WALLS").
//!
//! The folder gets `drawing.json` (units, spaces, layers, blocks and what couldn't be read), `objects.json` (keys,
//! names, text, block references, dimensions, tables, hatches and viewports) and four little-endian arrays:
//! `index.bin` (u32 × 8 per object: space, type, layer, block + 1, first point, points, flags, parent + 1),
//! `num.bin` (f64 × 7: extent left, bottom, right, top, length, area, volume), `coords.bin` (f64 pairs; a NaN pair
//! separates the parts of one object) and `colours.bin` (u32 per object: the colour it shows in, 0xRRGGBB).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::rc::Rc;

use opencadcodec::entities::mtext_format::parse_mtext;
use opencadcodec::entities::{AcisData, AttachmentPoint, BoundaryEdge, EntityType, Hatch, Insert};
use opencadcodec::objects::ObjectType;
use opencadcodec::types::{Color, Handle, Matrix4, Vector3};
use opencadcodec::{CadDocument, DwgReadOptions, DwgReader, DxfReader, ReadOutcome};
use serde_json::{json, Value};

use crate::geometry::{self as g, Affine, Clip, Shape};
use crate::solid;
use crate::Failure;

/// Bumped whenever what the folder holds changes, so Quantix reads older folders again.
pub const FORMAT: u32 = 4;
pub const READER: &str = "opencadcodec a35f43e, opencadkernel ee41029";

pub const TYPES: [&str; 22] = [
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
    "Region",
    "Solid3D",
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
const REGION: usize = 20;
const SOLID3D: usize = 21;

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
    /// The clip boundaries of the block references it sits in: only what shows through all of them is kept.
    clips: Rc<Vec<Clip>>,
    /// The drawing its objects come from: the one being read, or a drawing it refers to.
    source: usize,
    /// The drawings being placed around it, one bit each, so a drawing that refers back to one of them stops.
    above: u64,
    /// The colour of the block reference (or dimension, leader or multiline) it sits in: what "by block" shows in.
    colour: u32,
}

/// White, as CAD shows colour 7 on paper: in ink.
const WHITE: u32 = 0xFF_FFFF;

/// A drawing whose objects the walker places: the one being read (the first), or a drawing it refers to (an
/// xref) that Quantix was given.
struct Source<'a> {
    doc: &'a CadDocument,
    /// What its layers and blocks are called in the drawing being read: "" for that drawing, "NAME|" for an xref.
    prefix: String,
    block_names: HashMap<u64, String>,
    /// Each clipped block reference's clip boundary, in its block's own coordinates.
    clips: BTreeMap<u64, Vec<[f64; 2]>>,
}

impl<'a> Source<'a> {
    fn new(doc: &'a CadDocument, prefix: String) -> Source<'a> {
        Source {
            doc,
            prefix,
            block_names: doc
                .block_records
                .iter()
                .map(|b| (b.handle.value(), b.name.clone()))
                .collect(),
            clips: clip_boundaries(doc),
        }
    }
}

fn packed((r, g, b): (u8, u8, u8)) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// How much the walker had written, to take back a clipped block reference none of whose objects show.
struct Mark([usize; 8]);

struct Walker<'a> {
    sources: Vec<Source<'a>>,
    /// The drawings given for xrefs, by the xref's name in capitals.
    xref_sources: HashMap<String, usize>,
    layers: Vec<String>,
    layer_index: HashMap<String, u32>,
    blocks: Vec<String>,
    block_index: HashMap<String, u32>,
    keys: Vec<String>,
    index: Vec<[u32; 8]>,
    num: Vec<[f64; 7]>,
    coords: Vec<f64>,
    texts: Vec<Value>,
    inserts: Vec<Value>,
    dims: Vec<Value>,
    tables: Vec<Value>,
    hatches: Vec<Value>,
    viewports: Vec<Value>,
    /// Each object's colour, and the colour of the object being read.
    colours: Vec<u32>,
    colour: u32,
    not_read: BTreeMap<&'static str, u64>,
    xrefs: BTreeMap<String, Value>,
}

/// Reads a drawing into a folder. `xrefs` gives drawings it refers to as `NAME=path`: each is placed wherever the
/// drawing refers to it by that name, as CAD shows a loaded xref. One that can't be opened stays unloaded.
pub fn run(drawing: &str, folder: &str, xrefs: &[String]) -> Result<(), Failure> {
    let mut given = vec![];
    for xref in xrefs.iter().take(62) {
        let (name, path) = xref.split_once('=').ok_or(Failure::Usage)?;
        if let Ok(outcome) = open(Path::new(path)) {
            given.push((name.to_string(), outcome));
        }
    }
    let outcome = open(Path::new(drawing))?;
    let doc = &outcome.document;
    let mut walker = Walker::new(doc, &given);
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
            clips: Rc::default(),
            source: 0,
            above: 1,
            colour: WHITE,
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

/// Each clipped block reference's clip boundary (CAD's XCLIP), in its block's own coordinates. A clip hangs off
/// its reference through the reference's own dictionaries; a two-point boundary is a rectangle; a switched-off
/// clip shows everything.
fn clip_boundaries(doc: &CadDocument) -> BTreeMap<u64, Vec<[f64; 2]>> {
    let mut found = BTreeMap::new();
    for object in doc.objects.values() {
        let ObjectType::SpatialFilter(filter) = object else {
            continue;
        };
        let points: Vec<[f64; 2]> = filter.boundary_points.iter().map(|p| [p.x, p.y]).collect();
        let points = match points.as_slice() {
            [a, b] => vec![*a, [b[0], a[1]], *b, [a[0], b[1]]],
            _ => points,
        };
        if !filter.display_enabled || points.len() < 3 {
            continue;
        }
        let Some(insert) = clipped_insert(doc, filter.owner) else {
            continue;
        };
        // boundary → the world when the clip was made → the block's own coordinates
        let ring = points
            .iter()
            .map(|&p| {
                apply(
                    &filter.inverse_block_transform,
                    apply(&filter.clip_bound_transform, p),
                )
            })
            .collect();
        found.insert(insert.value(), ring);
    }
    found
}

fn clipped_insert(doc: &CadDocument, owner: Handle) -> Option<Handle> {
    let mut owner = Some(owner);
    for _ in 0..4 {
        let h = owner?;
        if let Some(EntityType::Insert(_)) = doc.get_entity(h) {
            return Some(h);
        }
        owner = doc.object_owner(h);
    }
    None
}

fn apply(m: &Matrix4, p: [f64; 2]) -> [f64; 2] {
    let m = m.m;
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][3],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][3],
    ]
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
    fn new(doc: &'a CadDocument, xrefs: &'a [(String, ReadOutcome)]) -> Walker<'a> {
        let mut sources = vec![Source::new(doc, String::new())];
        let mut xref_sources = HashMap::new();
        for (name, outcome) in xrefs {
            xref_sources.insert(name.to_uppercase(), sources.len());
            sources.push(Source::new(&outcome.document, format!("{name}|")));
        }
        let mut walker = Walker {
            sources,
            xref_sources,
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
            colours: vec![],
            colour: WHITE,
            not_read: BTreeMap::new(),
            xrefs: BTreeMap::new(),
        };
        for layer in doc.layers.iter() {
            walker.layer(&layer.name);
        }
        walker
    }

    fn mark(&self) -> Mark {
        Mark([
            self.keys.len(),
            self.coords.len(),
            self.texts.len(),
            self.inserts.len(),
            self.dims.len(),
            self.tables.len(),
            self.hatches.len(),
            self.viewports.len(),
        ])
    }

    fn rollback(&mut self, mark: &Mark) {
        let [objects, coords, texts, inserts, dims, tables, hatches, viewports] = mark.0;
        self.keys.truncate(objects);
        self.index.truncate(objects);
        self.num.truncate(objects);
        self.colours.truncate(objects);
        self.coords.truncate(coords);
        self.texts.truncate(texts);
        self.inserts.truncate(inserts);
        self.dims.truncate(dims);
        self.tables.truncate(tables);
        self.hatches.truncate(hatches);
        self.viewports.truncate(viewports);
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

    /// The colour an object shows in: its own, its layer's, or the block reference's it sits in.
    fn colour_of(&self, colour: &Color, layer: &str, place: &Place) -> u32 {
        match colour {
            Color::ByBlock => place.colour,
            Color::ByLayer => self
                .defined(layer, |d, n| d.layers.get(n))
                .and_then(|l| l.color.rgb())
                .map(packed)
                .unwrap_or(WHITE),
            other => other.rgb().map(packed).unwrap_or(WHITE),
        }
    }

    fn skipped(&mut self, what: &'static str) {
        *self.not_read.entry(what).or_insert(0) += 1;
    }

    /// Every object of a block (or a space), placed.
    fn block(&mut self, name: &str, place: &Place) {
        let doc = self.sources[place.source].doc;
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
            .push([e[0], e[1], e[2], e[3], round(length), round(area), 0.0]);
        self.colours.push(self.colour);
        i
    }

    /// An object placed, and cut to what shows through its clips; `None` when none of it shows.
    fn shape(
        &mut self,
        place: &Place,
        handle: u64,
        kind: usize,
        layer: &str,
        shape: Shape,
    ) -> Option<usize> {
        let placed = shape.transformed(&place.xf).clipped(&place.clips)?;
        Some(self.push(place, handle, kind, layer, None, Some(&placed), None, 0))
    }

    fn text(
        &mut self,
        place: &Place,
        handle: u64,
        kind: usize,
        layer: &str,
        found: TextFound,
    ) -> Option<usize> {
        let anchor = place.xf.apply(found.anchor);
        if !g::shows(&place.clips, anchor) {
            return None;
        }
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
        Some(i)
    }

    fn entity(&mut self, entity: &EntityType, place: &Place) {
        let common = entity.common();
        if common.invisible {
            return;
        }
        let handle = common.handle.value();
        let layer = match &place.layer {
            Some(inherited) if common.layer == "0" => inherited.clone(),
            _ => format!("{}{}", self.sources[place.source].prefix, common.layer),
        };
        let layer = layer.as_str();
        let own = self.colour_of(&common.color, layer, place);
        self.colour = own;
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
                let Some(shape) = g::polyline(&vertices, false)
                    .transformed(&place.xf)
                    .clipped(&place.clips)
                else {
                    return;
                };
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
                let mark = self.mark();
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
                    colour: own,
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
                if !place.clips.is_empty() && self.keys.len() == i + 1 {
                    self.rollback(&mark); // none of it shows through the clip
                    return;
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
                if !g::shows(&place.clips, at) {
                    return;
                }
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
                if !e.block_name.is_empty()
                    && self.sources[place.source]
                        .doc
                        .block_records
                        .get(&e.block_name)
                        .is_some()
                {
                    let inner = Place {
                        path: format!("{}{:X}/", place.path, handle),
                        parent: Some(i),
                        layer: Some(layer.to_string()),
                        flags: place.flags | ANNOTATION,
                        depth: place.depth + 1,
                        colour: own,
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
                let Some(i) =
                    self.shape(place, handle, MLINE, layer, g::polyline(&vertices, closed))
                else {
                    return;
                };
                let inner = Place {
                    path: format!("{}{:X}/", place.path, handle),
                    parent: Some(i),
                    flags: place.flags | DERIVED,
                    colour: own,
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
            EntityType::Solid3D(e) => self.solid(place, handle, layer, &e.acis_data),
            EntityType::Region(e) => self.region(place, handle, layer, &e.acis_data),
            EntityType::Body(_) | EntityType::Surface(_) => self.skipped("3D bodies and surfaces"),
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

    /// A 3D solid: its edges seen from above, and its volume, unless a clip cuts it.
    fn solid(&mut self, place: &Place, handle: u64, layer: &str, data: &AcisData) {
        let Some(measured) = solid::measure(data) else {
            self.skipped("3D solids whose shape can't be read");
            return;
        };
        let shape = Shape {
            parts: measured.edges,
            curved: true,
            approximate: true,
            ..Shape::default()
        }
        .transformed(&place.xf);
        let whole = shape
            .parts
            .iter()
            .flatten()
            .all(|&p| g::shows(&place.clips, p));
        let Some(i) = shape
            .clipped(&place.clips)
            .map(|shape| self.push(place, handle, SOLID3D, layer, None, Some(&shape), None, 0))
        else {
            return;
        };
        if whole {
            // a transform that keeps shapes scales a volume by its scale cubed
            self.num[i][6] = round(measured.volume * place.xf.det().abs().powf(1.5));
        } else {
            self.skipped("volumes of 3D solids a clip cuts");
        }
    }

    /// A region: a flat area bounded by its edges.
    fn region(&mut self, place: &Place, handle: u64, layer: &str, data: &AcisData) {
        let Some(measured) = solid::measure(data) else {
            self.skipped("regions whose shape can't be read");
            return;
        };
        let length = measured
            .edges
            .iter()
            .map(|e| g::chain_length(e, false))
            .sum();
        let shape = Shape {
            parts: g::rings_of(measured.edges),
            length,
            area: measured.area,
            closed: true,
            curved: true,
            approximate: true,
        };
        self.shape(place, handle, REGION, layer, shape);
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
        if let Some(i) = self.shape(place, handle, HATCH, layer, shape) {
            self.hatches.push(json!([i, e.pattern.name, e.is_solid]));
        }
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
        if !g::shows(&place.clips, at) {
            return;
        }
        let own = self.colour;
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
            && self.sources[place.source]
                .doc
                .block_records
                .get(&base.block_name)
                .is_some()
        {
            let inner = Place {
                path: format!("{}{:X}/", place.path, handle),
                parent: Some(i),
                layer: Some(layer.to_string()),
                flags: place.flags | ANNOTATION,
                depth: place.depth + 1,
                colour: own,
                ..place.clone()
            };
            self.block(&base.block_name.clone(), &inner);
            self.close_extent(i);
        }
    }

    fn insert(&mut self, e: &Insert, place: &Place, handle: u64, layer: &str) {
        let by_block = self.colour; // what its "by block" objects show in
        let source = &self.sources[place.source];
        let (doc, prefix) = (source.doc, source.prefix.clone());
        let Some(record) = doc.block_records.get(&e.block_name) else {
            self.skipped("references to blocks the drawing doesn't define");
            return;
        };
        // An anonymous copy of a dynamic block is counted under the block it came from.
        let name = doc
            .dynamic_definition_for_insert(e.common.handle)
            .and_then(|h| source.block_names.get(&h.value()).cloned())
            .filter(|n| !n.starts_with('*'))
            .unwrap_or_else(|| e.block_name.clone());
        let name = format!("{prefix}{name}");
        let clip_ring = source.clips.get(&handle).cloned();
        let is_xref =
            record.flags.is_xref || record.flags.is_xref_overlay || !record.xref_path.is_empty();
        // A drawing it refers to that Quantix was given is placed like a block: its model space, its layers named
        // after it, as CAD shows a loaded xref. One that refers back to a drawing around it stops there.
        let given = (is_xref && record.entity_handles.is_empty())
            .then(|| self.xref_sources.get(&e.block_name.to_uppercase()).copied())
            .flatten()
            .filter(|&s| place.above & (1 << s) == 0);
        if is_xref {
            let loaded = !record.entity_handles.is_empty() || given.is_some();
            self.xrefs.insert(
                name.clone(),
                json!({"name": name, "path": record.xref_path, "loaded": loaded}),
            );
        }
        let (block_source, block_name) = match given {
            Some(s) => (s, "*Model_Space".to_string()),
            None => (place.source, e.block_name.clone()),
        };
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
        // A clipped reference shows only what lies inside its clip boundary, and inside those around it.
        let clip =
            clip_ring.and_then(|ring| Clip::new(ring.iter().map(|&p| own.apply(p)).collect()));
        let clips = match clip {
            Some(clip) => {
                let mut all = (*place.clips).clone();
                all.push(clip);
                Rc::new(all)
            }
            None => place.clips.clone(),
        };
        let mark = self.mark();
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
                format!("{prefix}{}", attribute.common.layer)
            };
            let inner = Place {
                path: format!("{}{:X}/", place.path, handle),
                parent: Some(i),
                flags: place.flags | IN_BLOCK,
                clips: clips.clone(),
                colour: by_block,
                ..place.clone()
            };
            self.colour = self.colour_of(&attribute.common.color, &attribute_layer, &inner);
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
                    clips: clips.clone(),
                    source: block_source,
                    above: place.above | (1 << block_source),
                    colour: by_block,
                };
                self.block(&block_name, &inner);
            }
        }
        if !clips.is_empty() && self.keys.len() == i + 1 {
            // none of the reference's objects show through its clips
            let empty = self.sources[block_source]
                .doc
                .entities_in_block(&block_name)
                .next()
                .is_none();
            if !(empty && g::shows(&place.clips, world)) {
                self.rollback(&mark);
                return;
            }
        }
        self.close_extent(i);
    }

    /// A layer or block as the drawing being read defines it, or else the drawing it comes from ("NAME|LAYER").
    fn defined<T: 'a>(
        &self,
        name: &str,
        get: impl Fn(&'a CadDocument, &str) -> Option<&'a T>,
    ) -> Option<&'a T> {
        get(self.sources[0].doc, name).or_else(|| {
            let (xref, own) = name.rsplit_once('|')?;
            let xref = xref.rsplit('|').next()?;
            let source = *self.xref_sources.get(&xref.to_uppercase())?;
            get(self.sources[source].doc, own)
        })
    }

    fn write(
        self,
        folder: &Path,
        outcome: &ReadOutcome,
        spaces: Vec<Value>,
    ) -> Result<(), Failure> {
        let doc = self.sources[0].doc;
        let staging = folder.with_extension("reading");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(Failure::io)?;
        let layers: Vec<Value> = self
            .layers
            .iter()
            .map(|name| match self.defined(name, |d, n| d.layers.get(n)) {
                Some(l) => json!({"name": name, "off": l.flags.off, "frozen": l.flags.frozen, "locked": l.flags.locked, "linetype": l.line_type}),
                None => json!({"name": name, "off": false, "frozen": false, "locked": false, "linetype": ""}),
            })
            .collect();
        let blocks: Vec<Value> = self
            .blocks
            .iter()
            .map(|name| match self.defined(name, |d, n| d.block_records.get(n)) {
                Some(b) => json!({"name": name, "anonymous": b.flags.anonymous, "xref": b.flags.is_xref || !b.xref_path.is_empty(), "description": b.description, "units": b.units}),
                None => json!({"name": name, "anonymous": name.starts_with('*'), "xref": false, "description": "", "units": 0}),
            })
            .collect();
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
                "clipped": self.sources.iter().map(|s| s.clips.len()).sum::<usize>(),
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
        let mut colours =
            BufWriter::new(fs::File::create(staging.join("colours.bin")).map_err(Failure::io)?);
        for v in &self.colours {
            colours.write_all(&v.to_le_bytes()).map_err(Failure::io)?;
        }
        colours.flush().map_err(Failure::io)?;
        drop((index, num, coords, colours));
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

/// Drawings written by `qx-dwg sample` and read back, as the service reads them.
#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * b.abs().max(1.0)
    }

    fn close_all<const N: usize>(a: [f64; N], b: [f64; N]) -> bool {
        a.iter().zip(b).all(|(&x, y)| close(x, y))
    }

    fn near(points: &[[f64; 2]], expected: &[[f64; 2]]) -> bool {
        points.len() == expected.len()
            && points.iter().zip(expected).all(|(&p, &q)| close_all(p, q))
    }

    fn message(failure: Failure) -> String {
        match failure {
            Failure::Unreadable(m) => format!("unreadable: {m}"),
            Failure::Other(m) => m,
            Failure::Usage => "usage".to_string(),
        }
    }

    fn done(result: Result<(), Failure>) {
        if let Err(failure) = result {
            panic!("{}", message(failure));
        }
    }

    /// A folder of the test's own under the system's temporary folder, removed afterwards.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("qx-dwg-read-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }

        /// A synthetic drawing, written as the service's tests write theirs.
        fn drawing(&self, name: &str, spec: Value) -> String {
            let spec_path = self.path(&format!("{name}.json"));
            fs::write(&spec_path, spec.to_string()).unwrap();
            let drawing = self.path(name);
            done(crate::sample::run(&spec_path, &drawing));
            drawing
        }

        fn read(&self, drawing: &str, xrefs: &[String]) -> Folder {
            let folder = self.path("read");
            done(run(drawing, &folder, xrefs));
            Folder::load(Path::new(&folder))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// What a read left in its folder.
    struct Folder {
        drawing: Value,
        objects: Value,
        index: Vec<[u32; 8]>,
        num: Vec<[f64; 7]>,
        coords: Vec<f64>,
        colours: Vec<u32>,
    }

    /// Values in rows of `N`, with nothing left over.
    fn whole<T: Copy, const N: usize>(values: &[T]) -> Vec<[T; N]> {
        let (rows, rest) = values.as_chunks::<N>();
        assert!(rest.is_empty());
        rows.to_vec()
    }

    impl Folder {
        fn load(folder: &Path) -> Folder {
            let bytes = |name: &str| fs::read(folder.join(name)).unwrap();
            let json = |name: &str| -> Value { serde_json::from_slice(&bytes(name)).unwrap() };
            let words = |name: &str| -> Vec<u32> {
                let bytes = bytes(name);
                whole(&bytes).into_iter().map(u32::from_le_bytes).collect()
            };
            let floats = |name: &str| -> Vec<f64> {
                let bytes = bytes(name);
                whole(&bytes).into_iter().map(f64::from_le_bytes).collect()
            };
            let read = Folder {
                drawing: json("drawing.json"),
                objects: json("objects.json"),
                index: whole(&words("index.bin")),
                num: whole(&floats("num.bin")),
                coords: floats("coords.bin"),
                colours: words("colours.bin"),
            };
            read.check();
            read
        }

        /// The files agree as the service reads them: a row per object in each array, each object's points within
        /// the coordinates, its parent before it, and pages numbered from 1 with no gaps, each counting its objects.
        fn check(&self) {
            let count = self.objects["keys"].as_array().unwrap().len();
            assert_eq!(
                (self.index.len(), self.num.len(), self.colours.len()),
                (count, count, count)
            );
            let listed = |list: &str| self.objects[list].as_array().unwrap().len();
            let spaces = self.drawing["spaces"].as_array().unwrap();
            for (i, row) in self.index.iter().enumerate() {
                let [space, kind, layer, block, start, points, _, parent] = row.map(|v| v as usize);
                assert!((1..=spaces.len()).contains(&space));
                assert!(
                    kind < TYPES.len() && layer < listed("layers") && block <= listed("blocks")
                );
                assert!(parent <= i);
                assert!(2 * (start + points) <= self.coords.len());
                if points > 0 {
                    // a NaN pair only ever separates two parts
                    assert!(!self.coords[2 * start].is_nan());
                    assert!(!self.coords[2 * (start + points) - 1].is_nan());
                }
            }
            for (page, space) in spaces.iter().enumerate() {
                assert_eq!(space["number"], page + 1);
                let objects = self
                    .index
                    .iter()
                    .filter(|r| r[0] as usize == page + 1)
                    .count();
                assert_eq!(space["objects"], objects);
            }
        }

        /// Every object of a type, in the order read.
        fn all(&self, kind: usize) -> Vec<usize> {
            (0..self.index.len())
                .filter(|&i| self.index[i][1] as usize == kind)
                .collect()
        }

        fn one(&self, kind: usize) -> usize {
            let found = self.all(kind);
            assert_eq!(found.len(), 1, "one {}", TYPES[kind]);
            found[0]
        }

        /// The one object of a type that a block reference holds.
        fn held(&self, parent: usize, kind: usize) -> usize {
            let found: Vec<usize> = self
                .all(kind)
                .into_iter()
                .filter(|&i| self.parent(i) == Some(parent))
                .collect();
            assert_eq!(found.len(), 1, "one {} in {parent}", TYPES[kind]);
            found[0]
        }

        fn key(&self, i: usize) -> &str {
            self.objects["keys"][i].as_str().unwrap()
        }

        fn layer(&self, i: usize) -> &str {
            self.objects["layers"][self.index[i][2] as usize]
                .as_str()
                .unwrap()
        }

        fn block(&self, i: usize) -> Option<&str> {
            let block = self.index[i][3] as usize;
            (block > 0).then(|| self.objects["blocks"][block - 1].as_str().unwrap())
        }

        fn flags(&self, i: usize) -> u32 {
            self.index[i][6]
        }

        fn parent(&self, i: usize) -> Option<usize> {
            (self.index[i][7] as usize).checked_sub(1)
        }

        fn extent(&self, i: usize) -> [f64; 4] {
            self.num[i][..4].try_into().unwrap()
        }

        fn length(&self, i: usize) -> f64 {
            self.num[i][4]
        }

        fn area(&self, i: usize) -> f64 {
            self.num[i][5]
        }

        fn volume(&self, i: usize) -> f64 {
            self.num[i][6]
        }

        /// The object's chains of points.
        fn parts(&self, i: usize) -> Vec<Vec<[f64; 2]>> {
            let [.., start, count, _, _] = self.index[i].map(|v| v as usize);
            let mut parts = vec![vec![]];
            for k in start..start + count {
                let p = [self.coords[2 * k], self.coords[2 * k + 1]];
                if p[0].is_nan() {
                    parts.push(vec![]);
                } else if let Some(part) = parts.last_mut() {
                    part.push(p);
                }
            }
            parts.retain(|p| !p.is_empty());
            parts
        }

        fn points(&self, i: usize) -> Vec<[f64; 2]> {
            self.parts(i).concat()
        }

        /// The row of one of objects.json's lists (texts, inserts, dims, hatches, viewports) for an object.
        fn row(&self, list: &str, i: usize) -> &Value {
            self.objects[list]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row[0] == i)
                .unwrap()
        }

        fn layers(&self) -> Vec<&str> {
            self.drawing["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["name"].as_str().unwrap())
                .collect()
        }
    }

    #[test]
    fn lines_arcs_circles_and_polylines_keep_their_exact_measures() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 6, "layers": ["A-WALL", "S-SLAB"], "entities": [
            {"type": "line", "layer": "A-WALL", "from": [0, 0], "to": [3, 4]},
            {"type": "polyline", "layer": "S-SLAB", "points": [[0, 0], [10, 0], [10, 10], [0, 10]],
                "bulges": [0, 0, 1, 0], "closed": true},
            {"type": "polyline", "layer": "A-WALL", "points": [[0, 0], [10, 0], [10, 10]]},
            {"type": "circle", "layer": "A-WALL", "centre": [50, 50], "radius": 2},
            {"type": "arc", "layer": "A-WALL", "centre": [0, 0], "radius": 10, "start": 0, "end": 90},
        ]});
        let read = scratch.read(&scratch.drawing("plan.dwg", spec), &[]);
        assert_eq!(read.drawing["format"], FORMAT);
        assert_eq!(read.drawing["units"]["insunits"], 6); // metres, as its header says
        let model = &read.drawing["spaces"][0];
        assert_eq!(
            (&model["name"], &model["kind"]),
            (&json!("Model"), &json!("model"))
        );
        assert_eq!(model["objects"], 5);
        for layer in ["0", "A-WALL", "S-SLAB"] {
            assert!(read.layers().contains(&layer));
        }

        let line = read.one(LINE);
        assert_eq!(read.layer(line), "A-WALL");
        assert_eq!(read.points(line), [[0.0, 0.0], [3.0, 4.0]]);
        assert_eq!(
            (read.length(line), read.area(line), read.flags(line)),
            (5.0, 0.0, 0)
        );
        assert_eq!(read.extent(line), [0.0, 0.0, 3.0, 4.0]);
        assert!(u64::from_str_radix(read.key(line), 16).is_ok()); // its handle, in hex
        let slab = read.all(POLYLINE)[0];
        assert_eq!(read.layer(slab), "S-SLAB");
        assert_eq!(read.flags(slab), CLOSED);
        assert!(close(read.area(slab), 100.0 + PI * 12.5));
        assert!(close(read.length(slab), 30.0 + PI * 5.0));
        assert!(close(read.extent(slab)[3], 15.0)); // the bulge rises half the width above the square
        let open = read.all(POLYLINE)[1];
        assert_eq!(
            (read.length(open), read.area(open), read.flags(open)),
            (20.0, 0.0, 0)
        );
        let circle = read.one(CIRCLE);
        assert_eq!(read.flags(circle), CLOSED);
        assert!(close(read.area(circle), 4.0 * PI) && close(read.length(circle), 4.0 * PI));
        let arc = read.one(ARC);
        assert!(close(read.length(arc), 5.0 * PI) && read.area(arc) == 0.0);
        assert!(close_all(read.extent(arc), [0.0, 0.0, 10.0, 10.0]));
        assert!(read.num.iter().all(|n| n[6] == 0.0)); // nothing here holds a volume
    }

    #[test]
    fn a_dxf_drawing_reads_as_the_same_drawing_in_dwg_does() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 1, "layers": ["A-WALL"], "entities": [
            {"type": "line", "layer": "A-WALL", "from": [0, 0], "to": [120, 0]},
            {"type": "polyline", "layer": "A-WALL", "points": [[0, 0], [120, 0], [120, 96], [0, 96]], "closed": true},
            {"type": "circle", "layer": "A-WALL", "centre": [60, 48], "radius": 6},
            {"type": "text", "layer": "A-WALL", "at": [10, 10], "value": "STORE", "height": 6},
        ]});
        let dwg = scratch.read(&scratch.drawing("plan.dwg", spec.clone()), &[]);
        let dxf = scratch.read(&scratch.drawing("plan.DXF", spec), &[]);
        assert_eq!(dxf.drawing["units"]["insunits"], 1); // inches
        let seen = |f: &Folder| {
            (0..f.index.len())
                .map(|i| {
                    let kind = f.index[i][1] as usize;
                    (
                        kind,
                        f.layer(i).to_string(),
                        f.points(i),
                        f.length(i),
                        f.area(i),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(dxf.index.len(), 4);
        assert_eq!(seen(&dwg), seen(&dxf));
        assert_eq!(dwg.objects["texts"], dxf.objects["texts"]);
    }

    #[test]
    fn a_block_is_placed_once_per_reference_and_never_from_its_definition() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-GLAZ", "A-FRAME"], "blocks": [
            {"name": "WIN", "base": [0, 0], "entities": [
                {"type": "line", "from": [0, 0], "to": [1200, 0]},
                {"type": "polyline", "layer": "A-FRAME", "points": [[0, 0], [100, 0], [100, 100], [0, 100]],
                    "closed": true}]},
            {"name": "UNUSED", "base": [0, 0], "entities": [{"type": "line", "from": [0, 0], "to": [777, 0]}]}],
            "entities": [
                {"type": "insert", "block": "WIN", "layer": "A-GLAZ", "at": [1000, 500], "rotation": 90, "scale": 2,
                    "attributes": {"TYPE": "W1"}},
                {"type": "insert", "block": "WIN", "layer": "A-GLAZ", "at": [5000, 0]}]});
        let read = scratch.read(&scratch.drawing("windows.dwg", spec), &[]);
        let inserts = read.all(INSERT);
        assert_eq!(inserts.len(), 2);
        assert!(inserts.iter().all(|&i| read.block(i) == Some("WIN")));
        assert_eq!(read.objects["blocks"], json!(["WIN"])); // the block no one places counts for nothing

        // layer 0 inside the block takes the reference's layer; the frame keeps its own
        let lines = read.all(LINE);
        let lengths: Vec<f64> = lines.iter().map(|&i| read.length(i)).collect();
        assert!(close_all(lengths.try_into().unwrap(), [2400.0, 1200.0]));
        assert!(lines.iter().all(|&i| read.layer(i) == "A-GLAZ"));
        assert!(lines.iter().all(|&i| read.flags(i) & IN_BLOCK != 0));
        let frames = read.all(POLYLINE);
        let measures: Vec<(f64, f64)> = frames
            .iter()
            .map(|&i| (read.area(i), read.length(i)))
            .collect();
        assert_eq!(measures, [(40_000.0, 800.0), (10_000.0, 400.0)]);
        assert!(frames.iter().all(|&i| read.layer(i) == "A-FRAME"));

        // the first turned a quarter round, doubled and placed
        let (first, line) = (inserts[0], lines[0]);
        assert!(near(
            &read.points(line),
            &[[1000.0, 500.0], [1000.0, 2900.0]]
        ));
        assert_eq!(read.parent(line), Some(first));
        assert!(read.key(line).starts_with(&format!("{}/", read.key(first))));
        assert_eq!(
            read.row("inserts", first),
            &json!([first, "WIN", {"TYPE": "W1"}, 1000.0, 500.0, 90.0, 2.0, 2.0, 1, "WIN"])
        );
        // its attribute is a word of its own, inside the reference
        let attribute = read.one(ATTRIBUTE);
        assert_eq!(read.parent(attribute), Some(first));
        let text = read.row("texts", attribute);
        assert_eq!((&text[1], &text[7]), (&json!("W1"), &json!("TYPE")));
        // the reference's extent takes in all it holds
        let [left, bottom, _, top] = read.extent(first);
        assert!(close_all([left, bottom, top], [800.0, 500.0, 2900.0]));
    }

    #[test]
    fn a_mirrored_or_stretched_reference_measures_what_it_shows() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["X"], "blocks": [{"name": "SQ", "base": [50, 0], "entities": [
                {"type": "polyline", "points": [[0, 0], [100, 0], [100, 100], [0, 100]], "closed": true},
                {"type": "circle", "centre": [50, 50], "radius": 10}]}],
            "entities": [
                {"type": "insert", "block": "SQ", "layer": "X", "at": [0, 0], "scale": [-1, 1]},
                {"type": "insert", "block": "SQ", "layer": "X", "at": [1000, 0], "scale": [2, 1]}]});
        let read = scratch.read(&scratch.drawing("squares.dwg", spec), &[]);
        let [mirrored, stretched] = read.all(INSERT)[..] else {
            panic!("two references")
        };
        assert_eq!(
            (
                &read.row("inserts", mirrored)[6],
                &read.row("inserts", mirrored)[7]
            ),
            (&json!(-1.0), &json!(1.0))
        );

        // mirrored about its base point: the same size, the other way round
        let square = read.held(mirrored, POLYLINE);
        assert_eq!((read.area(square), read.length(square)), (10_000.0, 400.0));
        assert!(close_all(read.extent(square), [-50.0, 0.0, 50.0, 100.0]));
        let circle = read.held(mirrored, CIRCLE);
        assert!(close(read.area(circle), 100.0 * PI) && close(read.length(circle), 20.0 * PI));
        assert_eq!(read.flags(circle) & APPROXIMATE, 0);
        let [left, bottom, right, top] = read.extent(circle);
        assert!(((left + right) / 2.0).abs() < 0.01 && ((bottom + top) / 2.0 - 50.0).abs() < 0.01);

        // stretched twice as wide: straight sides stay exact, the circle becomes an ellipse
        let square = read.held(stretched, POLYLINE);
        assert_eq!((read.area(square), read.length(square)), (20_000.0, 600.0));
        assert_eq!(read.flags(square) & APPROXIMATE, 0);
        let ellipse = read.held(stretched, CIRCLE);
        assert!(close(read.area(ellipse), 200.0 * PI));
        assert_ne!(read.flags(ellipse) & APPROXIMATE, 0);
        let perimeter = PI * (90.0 - 3500f64.sqrt()); // the 20 × 10 ellipse's (Ramanujan)
        assert!((read.length(ellipse) / perimeter - 1.0).abs() < 0.001);
    }

    #[test]
    fn nested_blocks_carry_the_chain_of_references_in_their_keys() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["X"], "blocks": [
                {"name": "INNER", "base": [0, 0], "entities": [{"type": "line", "from": [0, 0], "to": [10, 0]}]},
                {"name": "OUTER", "base": [0, 0], "entities": [
                    {"type": "insert", "block": "INNER", "at": [100, 0], "rotation": 90}]}],
            "entities": [{"type": "insert", "block": "OUTER", "layer": "X", "at": [1000, 1000], "scale": 2}]});
        let read = scratch.read(&scratch.drawing("nested.dwg", spec), &[]);
        let [outer, inner] = read.all(INSERT)[..] else {
            panic!("two references")
        };
        let line = read.one(LINE);
        assert_eq!(
            (read.parent(line), read.parent(inner), read.parent(outer)),
            (Some(inner), Some(outer), None)
        );
        assert!(read
            .key(inner)
            .starts_with(&format!("{}/", read.key(outer))));
        assert!(read.key(line).starts_with(&format!("{}/", read.key(inner))));
        assert_eq!(read.key(line).matches('/').count(), 2);
        // turned inside, doubled outside: 10 long becomes 20, standing up from (1200, 1000)
        assert!(near(
            &read.points(line),
            &[[1200.0, 1000.0], [1200.0, 1020.0]]
        ));
        assert!(close(read.length(line), 20.0));
        assert_eq!(read.layer(line), "X"); // layer 0 all the way down takes the outer reference's layer
        let row = read.row("inserts", inner);
        assert_eq!(
            (&row[1], &row[3], &row[4], &row[5]),
            (
                &json!("INNER"),
                &json!(1200.0),
                &json!(1000.0),
                &json!(90.0)
            )
        );
    }

    #[test]
    fn a_hatch_takes_off_its_islands_and_counts_islands_within_them() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-FLOR"], "entities": [
            {"type": "hatch", "layer": "A-FLOR", "pattern": "ANSI31", "loops": [
                [[0, 0], [10000, 0], [10000, 8000], [0, 8000]],
                [[7000, 3000], [8000, 3000], [8000, 4000], [7000, 4000]]]},
            {"type": "hatch", "layer": "A-FLOR", "loops": [
                [[20000, 0], [20010, 0], [20010, 10], [20000, 10]],
                [[20002, 2], [20008, 2], [20008, 8], [20002, 8]],
                [[20004, 4], [20006, 4], [20006, 6], [20004, 6]]]}]});
        let read = scratch.read(&scratch.drawing("floor.dwg", spec), &[]);
        let [floor, rings] = read.all(HATCH)[..] else {
            panic!("two hatches")
        };
        assert_eq!(
            (read.area(floor), read.length(floor)),
            (79_000_000.0, 40_000.0)
        );
        assert_eq!(read.parts(floor).len(), 2);
        assert_eq!(read.flags(floor), CLOSED);
        assert_eq!(read.row("hatches", floor), &json!([floor, "ANSI31", false]));
        assert_eq!(
            (read.area(rings), read.length(rings)),
            (100.0 - 36.0 + 4.0, 40.0 + 24.0 + 8.0)
        );
        assert_eq!(read.row("hatches", rings), &json!([rings, "SOLID", true]));
    }

    #[test]
    fn texts_keep_their_words_where_they_are_written() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-ROOM"], "entities": [
            {"type": "text", "layer": "A-ROOM", "at": [2500, 4000], "value": "LIVING", "height": 250},
            {"type": "text", "layer": "A-ROOM", "at": [0, 0], "value": "%%c12 BARS AT 200"},
            {"type": "mtext", "layer": "A-ROOM", "at": [0, 1000], "value": "ROOM\\PNAME {\\C1;RED}"}]});
        let read = scratch.read(&scratch.drawing("words.dwg", spec), &[]);
        let [living, bars] = read.all(TEXT)[..] else {
            panic!("two texts")
        };
        assert_eq!(
            read.row("texts", living),
            &json!([living, "LIVING", null, 2500.0, 4000.0, 250.0, 0.0, null])
        );
        // no font to hand: each character 0.8 of the height wide
        assert_eq!(read.extent(living), [2500.0, 4000.0, 3700.0, 4250.0]);
        assert!(read.parts(living).is_empty());
        let bars = read.row("texts", bars);
        assert_eq!(
            (&bars[1], &bars[2]),
            (&json!("Ø12 BARS AT 200"), &json!("%%c12 BARS AT 200"))
        );
        assert_eq!(read.row("texts", read.one(MTEXT))[1], "ROOM\nNAME RED");
    }

    #[test]
    fn dimensions_keep_what_they_measure_and_what_is_written_on_them() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-DIMS"], "entities": [
            {"type": "dimension", "layer": "A-DIMS", "from": [0, -1000], "to": [10000, -1000], "text": "9800"},
            {"type": "dimension", "layer": "A-DIMS", "from": [0, 8500], "to": [5000, 8500]},
            {"type": "dimension", "layer": "A-DIMS", "from": [0, 0], "to": [2500, 0], "text": "<> mm"}]});
        let read = scratch.read(&scratch.drawing("dims.dwg", spec), &[]);
        let [written, measured, around] = read.all(DIMENSION)[..] else {
            panic!("three dimensions")
        };
        let row = |i| {
            let row = read.row("dims", i);
            (row[1].as_f64().unwrap(), row[2].clone(), row[3].clone())
        };
        // a hand-written figure is kept beside the one the drawing measures
        assert_eq!(row(written), (10_000.0, json!("9800"), json!("9800")));
        assert_eq!(row(measured), (5_000.0, json!("5000"), Value::Null));
        assert_eq!(row(around), (2_500.0, json!("2500 mm"), json!("<> mm")));
        assert_eq!(read.row("dims", written)[7], "Aligned");
        // found at its text, 300 above the middle of what it measures
        assert_eq!(read.extent(written), [5000.0, -700.0, 5000.0, -700.0]);
    }

    #[test]
    fn colours_come_from_the_object_its_layer_or_the_reference_around_it() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-RED"], "layer_colours": {"A-RED": 1},
            "blocks": [{"name": "TAG", "base": [0, 0], "entities": [
                {"type": "line", "from": [0, 0], "to": [10, 0], "colour": 0},
                {"type": "line", "from": [0, 10], "to": [10, 10]}]}],
            "entities": [
                {"type": "line", "layer": "A-RED", "from": [0, 0], "to": [1, 0]},
                {"type": "line", "layer": "A-RED", "from": [0, 1], "to": [1, 1], "colour": 3},
                {"type": "line", "from": [0, 2], "to": [1, 2]},
                {"type": "insert", "block": "TAG", "layer": "A-RED", "at": [100, 0], "colour": 5}]});
        let read = scratch.read(&scratch.drawing("colours.dwg", spec), &[]);
        let colours: Vec<u32> = read.all(LINE).iter().map(|&i| read.colours[i]).collect();
        assert_eq!(
            colours,
            [
                0xFF0000, // by layer: A-RED is red
                0x00FF00, // its own green
                0xFFFFFF, // layer 0's white
                0x0000FF, // by block: the reference's blue
                0xFF0000, // by layer on layer 0 inside the block: the reference's layer
            ]
        );
        assert_eq!(read.colours[read.one(INSERT)], 0x0000FF);
    }

    #[test]
    fn each_layout_that_holds_anything_is_a_page_after_model_space() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["A-WALL"],
            "entities": [{"type": "line", "layer": "A-WALL", "from": [0, 0], "to": [10000, 0]}],
            "layouts": [{"name": "A-101", "entities": [
                {"type": "text", "at": [20, 20], "value": "DRAWING NO A-101", "height": 5},
                {"type": "viewport", "centre": [200, 150], "size": [400, 280], "view_centre": [5000, 4000],
                    "scale": 0.01}]}]});
        let read = scratch.read(&scratch.drawing("sheet.dwg", spec), &[]);
        // the document's own first layout tab is empty, so it is no page
        let spaces: Vec<(&str, &str)> = read.drawing["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["name"].as_str().unwrap(), s["kind"].as_str().unwrap()))
            .collect();
        assert_eq!(spaces, [("Model", "model"), ("A-101", "paper")]);
        assert_eq!(read.index[read.one(LINE)][0], 1);
        assert_eq!(read.index[read.one(TEXT)][0], 2);
        // the viewport shows model space around (5000, 4000) at 1:100
        let viewport = read
            .all(VIEWPORT)
            .into_iter()
            .find(|&i| read.row("viewports", i)[1] == 200.0)
            .unwrap();
        let row = read.row("viewports", viewport).as_array().unwrap();
        assert_eq!(
            row[1..10],
            json!([200.0, 150.0, 400.0, 280.0, 5000.0, 4000.0, 28000.0, 0.01, true])
                .as_array()
                .unwrap()[..]
        );
        assert_eq!(read.flags(viewport), ANNOTATION | CLOSED);
    }

    /// A base plan, and a plan that refers to it (an xref) by a path on the drawing author's computer.
    fn base_and_host(scratch: &Scratch) -> (String, String) {
        let base = scratch.drawing(
            "Base.dwg",
            json!({"insunits": 4, "layers": ["A-WALL"],
            "layer_colours": {"A-WALL": 1}, "entities": [
                {"type": "line", "layer": "A-WALL", "from": [0, 0], "to": [10000, 0]},
                {"type": "line", "layer": "A-WALL", "from": [10000, 0], "to": [10000, 8000]}]}),
        );
        let host = scratch.drawing("Host.dwg", json!({"insunits": 4, "layers": ["X-REF", "A-FURN"],
            "blocks": [{"name": "BASE", "base": [0, 0], "xref": "..\\Base\\Base.dwg", "entities": []}],
            "entities": [
                {"type": "insert", "block": "BASE", "layer": "X-REF", "at": [100000, 50000]},
                {"type": "circle", "layer": "A-FURN", "centre": [102000, 52000], "radius": 500}]}));
        (base, host)
    }

    #[test]
    fn a_drawing_it_refers_to_is_placed_once_it_is_given() {
        let scratch = Scratch::new();
        let (base, host) = base_and_host(&scratch);
        let alone = scratch.read(&host, &[]);
        assert_eq!(
            alone.drawing["xrefs"],
            json!([{"name": "BASE", "path": "..\\Base\\Base.dwg", "loaded": false}])
        );
        assert!(alone.all(LINE).is_empty() && !alone.layers().contains(&"BASE|A-WALL"));
        // one that can't be opened stays unloaded, and the rest is read
        let missing = scratch.read(&host, &[format!("BASE={}", scratch.path("Gone.dwg"))]);
        assert_eq!(missing.drawing["xrefs"][0]["loaded"], false);
        assert_eq!(missing.all(CIRCLE).len(), 1);

        let placed = scratch.read(&host, &[format!("BASE={base}")]);
        assert_eq!(placed.drawing["xrefs"][0]["loaded"], true);
        assert!(placed.layers().contains(&"BASE|A-WALL"));
        let reference = placed.one(INSERT);
        let walls = placed.all(LINE);
        let lengths: Vec<f64> = walls.iter().map(|&i| placed.length(i)).collect();
        assert_eq!(lengths, [10_000.0, 8_000.0]);
        for &wall in &walls {
            assert_eq!(placed.layer(wall), "BASE|A-WALL");
            assert_eq!(placed.parent(wall), Some(reference));
            assert_eq!(placed.colours[wall], 0xFF0000); // its layer's colour in the drawing it comes from
        }
        assert_eq!(
            placed.points(walls[0]),
            [[100_000.0, 50_000.0], [110_000.0, 50_000.0]]
        );
        // a drawing given without the name it is referred to by is a mistake
        let unnamed = run(&host, &scratch.path("unnamed"), &[base]);
        assert!(matches!(unnamed, Err(Failure::Usage)));
    }

    #[test]
    fn a_drawing_that_refers_back_to_itself_is_placed_only_once() {
        let scratch = Scratch::new();
        let (_, host) = base_and_host(&scratch);
        let read = scratch.read(&host, &[format!("BASE={host}")]);
        // the host placed once inside itself, where its own reference stops
        assert_eq!(read.all(INSERT).len(), 2);
        let mut circles: Vec<&str> = read.all(CIRCLE).iter().map(|&i| read.layer(i)).collect();
        circles.sort();
        assert_eq!(circles, ["A-FURN", "BASE|A-FURN"]);
    }

    /// A grid block, placed turned a quarter round (1000, 1000) and clipped: it covers x −4000 to 1000 and y 1000 to
    /// 11000, its two long lines standing at x 1000 and −4000 and its cross line lying at y 10000.
    fn grid(clip: Value) -> Value {
        json!({"insunits": 4, "layers": ["GRID", "A-WALL"],
            "blocks": [{"name": "GRID", "base": [0, 0], "entities": [
                {"type": "line", "from": [0, 0], "to": [10000, 0]},
                {"type": "line", "from": [0, 5000], "to": [10000, 5000]},
                {"type": "line", "from": [9000, 0], "to": [9000, 5000]},
                {"type": "hatch", "loops": [[[0, 0], [10000, 0], [10000, 5000], [0, 5000]]]}]}],
            "entities": [
                {"type": "insert", "block": "GRID", "layer": "GRID", "at": [1000, 1000], "rotation": 90, "clip": clip},
                {"type": "line", "layer": "A-WALL", "from": [0, -1000], "to": [1000, -1000]}]})
    }

    #[test]
    fn a_clipped_reference_keeps_only_what_shows() {
        // up to y 9000: the long lines 8000 each, the cross line not at all; a two-corner boundary is a rectangle
        let four = json!([[-5000, 500], [2000, 500], [2000, 9000], [-5000, 9000]]);
        let two = json!([[-5000, 500], [2000, 9000]]);
        for clip in [four, two] {
            let scratch = Scratch::new();
            let read = scratch.read(&scratch.drawing("grid.dwg", grid(clip)), &[]);
            assert_eq!(read.drawing["read"]["clipped"], 1);
            let lines: Vec<f64> = read
                .all(LINE)
                .into_iter()
                .filter(|&i| read.layer(i) == "GRID")
                .map(|i| read.length(i))
                .collect();
            assert!(close_all(lines.try_into().unwrap(), [8000.0, 8000.0]));
            assert!(close(read.area(read.one(HATCH)), 5000.0 * 8000.0));
        }
    }

    #[test]
    fn a_reference_clipped_away_entirely_leaves_nothing_of_it() {
        let scratch = Scratch::new();
        let far = json!([
            [50000, 50000],
            [60000, 50000],
            [60000, 60000],
            [50000, 60000]
        ]);
        let read = scratch.read(&scratch.drawing("grid.dwg", grid(far)), &[]);
        assert!(read.all(INSERT).is_empty() && read.all(HATCH).is_empty());
        let lines: Vec<&str> = read.all(LINE).iter().map(|&i| read.layer(i)).collect();
        assert_eq!(lines, ["A-WALL"]);
    }

    #[test]
    fn a_solid_keeps_its_volume_scaled_as_its_reference_is() {
        let scratch = Scratch::new();
        let spec = json!({"insunits": 4, "layers": ["S-STEEL"],
            "blocks": [{"name": "RUNG", "base": [0, 0], "entities": [
                {"type": "cylinder", "at": [0, 0, 0], "radius": 10, "height": 500}]}],
            "entities": [
                {"type": "box", "layer": "S-STEEL", "at": [1000, 1000, 50], "size": [2000, 300, 100]},
                {"type": "insert", "block": "RUNG", "layer": "S-STEEL", "at": [5000, 0]},
                {"type": "insert", "block": "RUNG", "layer": "S-STEEL", "at": [6000, 0], "scale": 2}]});
        let read = scratch.read(&scratch.drawing("steel.dwg", spec), &[]);
        let [slab, rung, doubled] = read.all(SOLID3D)[..] else {
            panic!("three solids")
        };
        assert!(close(read.volume(slab), 2000.0 * 300.0 * 100.0));
        assert!(close_all(read.extent(slab), [0.0, 850.0, 2000.0, 1150.0])); // seen from above
        let bar = PI * 10.0 * 10.0 * 500.0;
        assert!(close(read.volume(rung), bar));
        assert!(close(read.volume(doubled), 8.0 * bar)); // twice the size holds eight times as much
        assert!(read.all(INSERT).iter().all(|&i| read.volume(i) == 0.0)); // not the references around them
    }

    #[test]
    fn a_file_that_is_not_a_drawing_is_unreadable_and_nothing_is_written() {
        let scratch = Scratch::new();
        let spec = json!({"entities": [{"type": "line", "from": [0, 0], "to": [1000, 0]}]});
        let dwg = fs::read(scratch.drawing("whole.dwg", spec.clone())).unwrap();
        let dxf = fs::read(scratch.drawing("whole.dxf", spec)).unwrap();
        let files: [(&str, &[u8]); 7] = [
            ("words.dwg", b"this is not a drawing"),
            ("words.dxf", b"this is not a drawing"),
            ("empty.dwg", b""),
            ("empty.dxf", b""),
            ("report.dwg", b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n1 0 obj\n"),
            ("header.dwg", &dwg[..128]), // a drawing cut off after its first bytes
            ("half.dxf", &dxf[..dxf.len() / 2]),
        ];
        for (name, bytes) in files {
            let path = scratch.path(name);
            fs::write(&path, bytes).unwrap();
            let folder = scratch.path(&format!("{name}-read"));
            match run(&path, &folder, &[]) {
                Err(Failure::Unreadable(m)) => {
                    assert!(m.starts_with("This drawing can't be opened"), "{name}: {m}")
                }
                Err(other) => panic!("{name}: {}", message(other)),
                Ok(()) => panic!("{name} was read"),
            }
            assert!(!Path::new(&folder).exists(), "{name}");
        }
        let gone = run(&scratch.path("gone.dwg"), &scratch.path("gone"), &[]);
        assert!(matches!(gone, Err(Failure::Unreadable(_))));
    }

    #[test]
    fn reading_again_replaces_the_whole_folder() {
        let scratch = Scratch::new();
        let drawing = scratch.drawing(
            "plan.dwg",
            json!({"entities": [{"type": "line", "from": [0, 0], "to": [1000, 0]}]}),
        );
        let folder = Path::new(&scratch.path("read")).to_path_buf();
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("stale.bin"), b"from an older reader").unwrap();
        done(run(&drawing, &folder.to_string_lossy(), &[]));
        let mut names: Vec<String> = fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "colours.bin",
                "coords.bin",
                "drawing.json",
                "index.bin",
                "num.bin",
                "objects.json"
            ]
        );
        assert!(!folder.with_extension("reading").exists()); // nothing half-written left beside it
    }

    #[test]
    fn a_measurement_prints_as_the_drawing_would_print_it() {
        assert_eq!(printed(10000.0), "10000");
        assert_eq!(printed(9999.9999999), "10000");
        assert_eq!(printed(2.5), "2.5");
        assert_eq!(printed(1234.567), "1234.57");
        assert_eq!(printed(20.001), "20");
        assert_eq!(printed(-3.1), "-3.1");
    }

    #[test]
    fn percent_codes_print_as_their_characters() {
        assert_eq!(plain_text("%%c12 BARS"), "Ø12 BARS");
        assert_eq!(plain_text("90%%D BEND"), "90° BEND");
        assert_eq!(plain_text("%%p5 mm"), "±5 mm");
        assert_eq!(plain_text("100%%% FILL"), "100% FILL");
        assert_eq!(plain_text("%%uNOTE%%u 3"), "NOTE 3"); // underline on and off
        assert_eq!(plain_text("50% AND %%x"), "50% AND %%x");
        assert_eq!(plain_text("END%%"), "END%%");
    }

    #[test]
    fn a_text_box_follows_its_anchor_turn_and_attachment() {
        // no font metrics: a character is 0.8 of the height wide
        let plain = text_box([0.0, 0.0], "AB", 10.0, 0.0, 0.0, (0.0, 0.0));
        assert_eq!(plain, [0.0, 0.0, 16.0, 10.0]);
        let turned = text_box([0.0, 0.0], "AB", 10.0, 0.0, FRAC_PI_2, (0.0, 0.0));
        assert!(close_all(turned, [-10.0, 0.0, 0.0, 16.0]));
        // an MText's own width, centred on its anchor
        let centred = attachment(AttachmentPoint::MiddleCenter);
        let boxed = text_box([100.0, 100.0], "AB", 10.0, 40.0, 0.0, centred);
        assert_eq!(boxed, [80.0, 95.0, 120.0, 105.0]);
        // two lines hanging from the top left, spaced as CAD spaces them
        let top_left = attachment(AttachmentPoint::TopLeft);
        let two = text_box([0.0, 0.0], "A\nB", 10.0, 0.0, 0.0, top_left);
        assert!(close_all(two, [0.0, -33.4, 8.0, 0.0]));
        assert_eq!(attachment(AttachmentPoint::BottomRight), (1.0, 0.0));
    }
}
