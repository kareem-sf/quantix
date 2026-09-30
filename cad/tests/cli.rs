//! `qx-dwg` as the service runs it: a separate program that answers on standard output and says by its exit code
//! whether a drawing couldn't be read (2) or something else went wrong (1). `faces` is tested here because it
//! reads its request from standard input.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn qx_dwg(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qx-dwg"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The regions `qx-dwg faces` finds in segments `[x1, y1, x2, y2]`.
fn faces(segments: &str, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    let request = format!(r#"{{"segments": {segments}, "tolerance": {tolerance}}}"#);
    let done = qx_dwg(&["faces"], &request);
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let answer: serde_json::Value = serde_json::from_slice(&done.stdout).unwrap();
    serde_json::from_value(answer["faces"].clone()).unwrap()
}

/// A ring's area, positive when it runs counter-clockwise.
fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

const SQUARE: &str = "[[0, 0, 10, 0], [10, 0, 10, 10], [10, 10, 0, 10], [0, 10, 0, 0]]";

#[test]
fn faces_finds_the_room_four_walls_close() {
    let found = faces(SQUARE, 0.001);
    assert_eq!(found.len(), 1);
    assert_eq!(signed_area(&found[0]), 100.0); // counter-clockwise
}

#[test]
fn faces_finds_two_rooms_either_side_of_a_shared_wall() {
    let found = faces(
        "[[0, 0, 20, 0], [20, 0, 20, 10], [20, 10, 0, 10], [0, 10, 0, 0], [10, 0, 10, 10]]",
        0.001,
    );
    let areas: Vec<f64> = found.iter().map(|r| signed_area(r)).collect();
    assert_eq!(areas, [100.0, 100.0]);
}

#[test]
fn faces_finds_nothing_in_an_open_shape() {
    let found = faces("[[0, 0, 10, 0], [10, 0, 10, 10], [10, 10, 0, 10]]", 0.001);
    assert!(found.is_empty());
    assert!(faces("[]", 0.001).is_empty());
}

#[test]
fn faces_splits_walls_where_they_cross() {
    // a noughts-and-crosses grid: only the middle square is closed
    let found = faces(
        "[[1, 0, 1, 3], [2, 0, 2, 3], [0, 1, 3, 1], [0, 2, 3, 2]]",
        0.001,
    );
    assert_eq!(
        found,
        [vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]]]
    );
}

#[test]
fn faces_closes_a_gap_only_within_the_tolerance() {
    let gap = "[[0, 0, 10, 0], [10, 0, 10, 10], [10, 10, 0, 10], [0, 10, 0, 0.004]]";
    assert_eq!(faces(gap, 0.01).len(), 1);
    assert!(faces(gap, 0.000001).is_empty());
}

#[test]
fn faces_leaves_out_segments_of_no_length() {
    let with_dots = "[[0, 0, 10, 0], [10, 0, 10, 10], [5, 5, 5, 5], [10, 10, 0, 10], [0, 10, 0, 0], [0, 0, 0, 0]]";
    assert_eq!(faces(with_dots, 0.001), faces(SQUARE, 0.001));
}

#[test]
fn a_request_it_cant_understand_fails_with_the_reason() {
    for request in [
        "not json",
        r#"{"segments": [[0, 0, 10]], "tolerance": 1}"#,
        r#"{"segments": []}"#,
    ] {
        let done = qx_dwg(&["faces"], request);
        assert_eq!(done.status.code(), Some(1), "{request}");
        assert!(String::from_utf8_lossy(&done.stderr).starts_with("Bad request"));
    }
}

#[test]
fn a_drawing_it_cant_read_exits_2_with_a_plain_message() {
    let folder = std::env::temp_dir().join(format!("qx-dwg-cli-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let drawing = folder.join("notes.dwg");
    std::fs::write(&drawing, "these are notes, not a drawing").unwrap();
    let out = folder.join("read");
    let done = qx_dwg(
        &["read", &drawing.to_string_lossy(), &out.to_string_lossy()],
        "",
    );
    let _ = std::fs::remove_dir_all(&folder);
    assert_eq!(done.status.code(), Some(2));
    let message = String::from_utf8_lossy(&done.stderr);
    assert_eq!(
        message.trim(),
        "This drawing can't be opened: nothing in it could be read."
    );
}

#[test]
fn a_command_it_doesnt_know_exits_1_with_the_usage() {
    for args in [
        &[][..],
        &["draw"],
        &["read", "plan.dwg"],
        &["faces", "extra"],
        &["sample", "spec.json"],
    ] {
        let done = qx_dwg(args, "");
        assert_eq!(done.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&done.stderr).starts_with("usage: qx-dwg"));
    }
}

#[test]
fn version_names_the_format_the_service_reads_and_the_libraries_built_in() {
    let done = qx_dwg(&["version"], "");
    assert!(done.status.success());
    // e.g. "4 opencadcodec a35f43e, opencadkernel ee41029"
    let version = String::from_utf8_lossy(&done.stdout).trim().to_string();
    let (format, libraries) = version.split_once(' ').unwrap();
    let service = include_str!("../../service/quantix/documents/cad.py");
    assert!(
        service.contains(&format!("\nFORMAT = {format} ")),
        "the service reads another folder format than {format}"
    );
    let manifest = include_str!("../Cargo.toml");
    for library in libraries.split(", ") {
        let (name, revision) = library.split_once(' ').unwrap();
        let pinned = manifest
            .lines()
            .find(|l| l.starts_with(&format!("{name} =")))
            .unwrap();
        assert!(
            pinned.contains(&format!("rev = \"{revision}\"")),
            "{pinned}"
        );
    }
}
