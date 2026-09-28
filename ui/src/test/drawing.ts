import type { DrawingInfo } from "../takeoff/cad";

/** A packed screen copy as the service sends it: a wall line (object 0) along the x axis through the centre, a
 * door's leaf (object 1) above it, and the word KITCHEN (object 2). */
export function screenCopy(): ArrayBuffer {
  const header = new TextEncoder().encode(
    JSON.stringify({
      stamp: "st1",
      page: 1,
      name: "Model",
      kind: "model",
      origin: [5000, 4000],
      extents: [-10, -10, 10, 10],
      layers: ["A-WALL", "A-DOOR", "A-ROOM"],
      hidden: [],
      types: ["Line", "Text"],
      objects: 3,
      segments: 2,
      texts: [[2, "KITCHEN", 0, -5, 1, 0]],
    }),
  );
  const padded = header.length + ((4 - (header.length % 4)) % 4);
  const buffer = new ArrayBuffer(8 + padded + 2 * 16 + 2 * 4 + 3 * 12 + 3 * 16);
  const bytes = new Uint8Array(buffer);
  bytes.set(new TextEncoder().encode("QXD1"), 0);
  new DataView(buffer).setUint32(4, padded, true);
  bytes.set(header, 8);
  bytes.fill(32, 8 + header.length, 8 + padded);
  let offset = 8 + padded;
  new Float32Array(buffer, offset, 8).set([-10, 0, 10, 0, -2, 6, 2, 6]);
  offset += 32;
  new Uint32Array(buffer, offset, 2).set([0, 1]);
  offset += 8;
  new Uint32Array(buffer, offset, 9).set([0, 0, 0, 1, 0, 0, 2, 1, 0]);
  offset += 36;
  new Float32Array(buffer, offset, 12).set([-10, 0, 10, 0, -2, 6, 2, 6, 0, -5, 4, -4]);
  return buffer;
}

export const drawingInfo: DrawingInfo = {
  document_id: "d3",
  name: "A-201.dwg",
  stamp: "st1",
  pages: [{ number: 1, name: "Model", kind: "model", extents: [4990, 3990, 5010, 4010], objects: 3 }],
  header_units: "millimetres",
  units: null,
  evidence: ["The drawing's header says its units are millimetres."],
  not_read: {},
  layers: [
    { name: "A-WALL", objects: 1, types: { Line: 1 }, prints: true, meaning: "walls" },
    { name: "A-DOOR", objects: 1, types: { Line: 1 }, prints: true, meaning: null },
  ],
  blocks: [],
  meanings: { walls: "Walls" },
};
