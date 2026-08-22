import { readdir, readFile, stat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { inflateSync } from "node:zlib";

const brandDirectory = resolve("src/assets/brand");
const explorationDirectory = resolve("docs/brand/exploration");
const iconDirectory = resolve("src-tauri/icons");
const failures = [];
const approvedNativeIconBundleSha256 =
  "ab3cc43f7d89b08f316b465da5bf4077c20e954fc66e1f453a0b4673a0e33d08";

const forbiddenMarkup = [
  "text",
  "filter",
  "linearGradient",
  "radialGradient",
  "image",
  "foreignObject",
  "pattern",
  "mask",
  "clipPath",
];

const forbiddenWarmIdentityColors = [
  "#15242B",
  "#A8462F",
  "#F3F0E8",
  "#D78464",
];

async function read(path) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    failures.push(`${path}: ${error.message}`);
    return null;
  }
}

function forbidMarkup(path, source, allowStroke = false) {
  for (const element of forbiddenMarkup) {
    if (new RegExp(`<${element}\\b`, "i").test(source)) {
      failures.push(`${path}: <${element}> is not permitted`);
    }
  }
  if (!allowStroke && /\bstroke\s*=|\bstroke\s*:/.test(source)) {
    failures.push(`${path}: production mark geometry must use filled paths`);
  }
}

function forbidWarmIdentityColors(path, source) {
  for (const color of forbiddenWarmIdentityColors) {
    if (source.toLowerCase().includes(color.toLowerCase())) {
      failures.push(
        `${path}: obsolete warm identity color ${color} is not permitted`,
      );
    }
  }
}

function assertExactSvgPalette(path, source, allowedColors) {
  const colors = [
    ...source.matchAll(/\b(?:fill|stroke)=["'](#[0-9a-f]{6})["']/gi),
  ].map((match) => match[1].toUpperCase());
  const allowed = new Set(allowedColors.map((color) => color.toUpperCase()));
  const unexpected = [
    ...new Set(colors.filter((color) => !allowed.has(color))),
  ];
  if (unexpected.length > 0) {
    failures.push(
      `${path}: unexpected palette color(s) ${unexpected.join(", ")}`,
    );
  }
  for (const color of allowed) {
    if (!colors.includes(color)) {
      failures.push(`${path}: missing palette color ${color}`);
    }
  }
}

function pngTopLeftAlpha(buffer) {
  const idatChunks = [];
  let offset = 8;
  while (offset + 12 <= buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    if (type === "IDAT") {
      idatChunks.push(buffer.subarray(offset + 8, offset + 8 + length));
    }
    offset += 12 + length;
    if (type === "IEND") break;
  }
  const scanlines = inflateSync(Buffer.concat(idatChunks));
  // The first scanline has no prior row or left pixel, so every PNG filter
  // preserves the first pixel's channel values exactly.
  return scanlines.readUInt8(4);
}

function decodePngRgba(buffer) {
  const idatChunks = [];
  let width;
  let height;
  let colorType;
  let bitDepth;
  let interlace;
  let offset = 8;
  while (offset + 12 <= buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data.readUInt8(8);
      colorType = data.readUInt8(9);
      interlace = data.readUInt8(12);
    } else if (type === "IDAT") {
      idatChunks.push(data);
    }
    offset += 12 + length;
    if (type === "IEND") break;
  }
  if (bitDepth !== 8 || colorType !== 6 || interlace !== 0) return null;
  const raw = inflateSync(Buffer.concat(idatChunks));
  const rowBytes = width * 4;
  const pixels = Buffer.alloc(width * height * 4);
  let rawOffset = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = raw[rawOffset++];
    const rowStart = y * rowBytes;
    for (let x = 0; x < rowBytes; x += 1) {
      const left = x >= 4 ? pixels[rowStart + x - 4] : 0;
      const above = y > 0 ? pixels[rowStart - rowBytes + x] : 0;
      const upperLeft =
        y > 0 && x >= 4 ? pixels[rowStart - rowBytes + x - 4] : 0;
      const value = raw[rawOffset++];
      if (filter === 0) pixels[rowStart + x] = value;
      else if (filter === 1) pixels[rowStart + x] = (value + left) & 0xff;
      else if (filter === 2) pixels[rowStart + x] = (value + above) & 0xff;
      else if (filter === 3)
        pixels[rowStart + x] = (value + Math.floor((left + above) / 2)) & 0xff;
      else if (filter === 4) {
        const estimate = left + above - upperLeft;
        const pa = Math.abs(estimate - left);
        const pb = Math.abs(estimate - above);
        const pc = Math.abs(estimate - upperLeft);
        const predictor =
          pa <= pb && pa <= pc ? left : pb <= pc ? above : upperLeft;
        pixels[rowStart + x] = (value + predictor) & 0xff;
      } else {
        throw new Error(`unsupported PNG filter ${filter}`);
      }
    }
  }
  return pixels;
}

function validatePngPalette(path, buffer) {
  try {
    const pixels = decodePngRgba(buffer);
    if (!pixels) {
      failures.push(
        `${path}: generated icon must be a non-interlaced RGBA PNG`,
      );
      return;
    }
    for (let index = 0; index < pixels.length; index += 4) {
      const [red, green, blue, alpha] = pixels.subarray(index, index + 4);
      // Graphite/silver are intentionally near-neutral. Reject saturated color
      // channels while tolerating minor decoder/anti-aliasing variance.
      if (
        alpha > 16 &&
        Math.max(red, green, blue) - Math.min(red, green, blue) > 24
      ) {
        failures.push(
          `${path}: non-grayscale pixel rgb(${red} ${green} ${blue} / ${alpha}) detected at byte ${index}`,
        );
        return;
      }
    }
  } catch (error) {
    failures.push(
      `${path}: cannot inspect generated palette (${error.message})`,
    );
  }
}

async function validateNativeIconBundleFingerprint() {
  try {
    const relativePaths = (await readdir(iconDirectory, { recursive: true }))
      .map((path) => path.replaceAll("\\", "/"))
      .sort();
    const digest = createHash("sha256");
    for (const relativePath of relativePaths) {
      const path = resolve(iconDirectory, relativePath);
      if (!(await stat(path)).isFile()) continue;
      digest.update(relativePath);
      digest.update("\0");
      digest.update(await readFile(path));
      digest.update("\0");
    }
    const actual = digest.digest("hex");
    if (actual !== approvedNativeIconBundleSha256) {
      failures.push(
        `${iconDirectory}: native icon bundle fingerprint ${actual} does not match the approved grayscale bundle`,
      );
    }
  } catch (error) {
    failures.push(
      `${iconDirectory}: cannot fingerprint native icon bundle (${error.message})`,
    );
  }
}

const productionMarks = [
  ["quantix-mark.svg", ["#3F464D", "#9AA3AB"]],
  ["quantix-mark-mono.svg", ["#3F464D"]],
  ["quantix-mark-inverse.svg", ["#FFFFFF", "#BDC5CC"]],
];

for (const [filename, expectedColors] of productionMarks) {
  const path = resolve(brandDirectory, filename);
  const source = await read(path);
  if (source === null) continue;
  if (!/viewBox=["']0 0 48 48["']/.test(source)) {
    failures.push(`${path}: expected viewBox="0 0 48 48"`);
  }
  forbidMarkup(path, source);
  forbidWarmIdentityColors(path, source);
  assertExactSvgPalette(path, source, expectedColors);
  for (const id of ["evidence-register", "engineer-authority-datum"]) {
    if (!new RegExp(`<path\\b[^>]*\\bid=["']${id}["']`).test(source)) {
      failures.push(`${path}: missing named path ${id}`);
    }
  }
  if ((source.match(/<path\b/g) ?? []).length !== 2) {
    failures.push(`${path}: expected exactly two role-based paths`);
  }
  const fills = [...source.matchAll(/\bfill=["']([^"']+)["']/g)].map(
    (match) => match[1],
  );
  if (
    expectedColors.some((color) => !fills.includes(color)) ||
    fills.some((color) => !expectedColors.includes(color))
  ) {
    failures.push(`${path}: expected only ${expectedColors.join(", ")}`);
  }
}

const appIconPath = resolve(brandDirectory, "quantix-app-icon.svg");
const appIcon = await read(appIconPath);
if (appIcon !== null) {
  forbidMarkup(appIconPath, appIcon, true);
  if (!/viewBox=["']0 0 1024 1024["']/.test(appIcon)) {
    failures.push(`${appIconPath}: expected a 1024×1024 master viewBox`);
  }
  forbidWarmIdentityColors(appIconPath, appIcon);
  assertExactSvgPalette(appIconPath, appIcon, [
    "#3F464D",
    "#FFFFFF",
    "#9AA3AB",
  ]);
  if (
    /\bid=["']app-icon-field["']/.test(appIcon) ||
    /d=["']M0 0h1024v1024H0Z["']/.test(appIcon)
  ) {
    failures.push(`${appIconPath}: application icon must paint no background`);
  }
  for (const id of ["app-icon-counter", "app-icon-face"]) {
    if (!new RegExp(`\\bid=["']${id}["']`).test(appIcon)) {
      failures.push(`${appIconPath}: missing dual-contrast layer ${id}`);
    }
  }
  if (!/transform=["']translate\(48 52\) scale\(18\.5\)["']/.test(appIcon)) {
    failures.push(
      `${appIconPath}: Windows artwork must retain the approved 75% optical footprint`,
    );
  }
}

const splashPath = resolve(brandDirectory, "quantix-mark-splash.svg");
const splash = await read(splashPath);
if (splash !== null) {
  forbidMarkup(splashPath, splash, true);
  forbidWarmIdentityColors(splashPath, splash);
  assertExactSvgPalette(splashPath, splash, ["#3F464D", "#FFFFFF", "#9AA3AB"]);
  for (const id of [
    "quantix-symbol-counter",
    "quantix-symbol-face",
    "quantix-authority-datum",
  ]) {
    if (!new RegExp(`\\bid=["']${id}["']`).test(splash)) {
      failures.push(`${splashPath}: missing addressable splash layer ${id}`);
    }
  }
  if (/background(?:-color)?\s*=|background(?:-color)?\s*:/.test(splash)) {
    failures.push(`${splashPath}: transparent splash must paint no background`);
  }
}

const splashCssPath = resolve("src/splash.css");
const splashCss = await read(splashCssPath);
if (splashCss !== null) forbidWarmIdentityColors(splashCssPath, splashCss);

const splashHtmlPath = resolve("splash.html");
const splashHtml = await read(splashHtmlPath);
if (splashHtml !== null) {
  for (const id of ["quantix-splash-mark", "quantix-authority-datum"]) {
    if (!new RegExp(`\\bid=["']${id}["']`).test(splashHtml)) {
      failures.push(`${splashHtmlPath}: missing live splash element ${id}`);
    }
  }
  const liveCellIds = [
    ...splashHtml.matchAll(/\bid=["'](quantix-cell-\d{2})["']/g),
  ].map((match) => match[1]);
  const expectedCellIds = Array.from(
    { length: 10 },
    (_, index) => `quantix-cell-${String(index + 1).padStart(2, "0")}`,
  );
  if (
    liveCellIds.length !== expectedCellIds.length ||
    expectedCellIds.some((id, index) => liveCellIds[index] !== id)
  ) {
    failures.push(
      `${splashHtmlPath}: expected exactly ten ordered evidence cell groups`,
    );
  }
  for (const id of expectedCellIds) {
    const group = splashHtml.match(
      new RegExp(`<g\\b[^>]*\\bid=["']${id}["'][^>]*>([\\s\\S]*?)<\\/g>`),
    )?.[1];
    if (
      !group ||
      !/quantix-evidence-cell__counter/.test(group) ||
      !/quantix-evidence-cell__face/.test(group)
    ) {
      failures.push(
        `${splashHtmlPath}: ${id} must contain paired counter and face paths`,
      );
    }
  }
  if (/role=["']status["']|Opening your Tender workspace/i.test(splashHtml)) {
    failures.push(
      `${splashHtmlPath}: startup status belongs to the main shell`,
    );
  }
}

const indexPath = resolve("index.html");
const indexHtml = await read(indexPath);
if (
  indexHtml !== null &&
  !indexHtml.includes("/src/assets/brand/quantix-mark.svg")
) {
  failures.push(`${indexPath}: final SVG favicon is not wired`);
}

const obsoleteAssets = [
  "provenance-gate.svg",
  "provenance-gate-mono.svg",
  "provenance-gate-inverse.svg",
  "provenance-gate-splash.svg",
  "witness-register.svg",
  "witness-register-mono.svg",
  "candidate-a.svg",
  "candidate-b.svg",
  "candidate-c.svg",
  "candidate-d.svg",
  "candidate-e.svg",
  "candidate-f.svg",
  "round-one.svg",
];

for (const filename of obsoleteAssets) {
  const path = resolve(explorationDirectory, filename);
  try {
    await stat(path);
    failures.push(`${path}: rejected identity asset must be removed`);
  } catch {
    // Absence is required.
  }
}

const pngDimensions = new Map([
  ["32x32.png", [32, 32]],
  ["64x64.png", [64, 64]],
  ["128x128.png", [128, 128]],
  ["128x128@2x.png", [256, 256]],
  ["icon.png", [512, 512]],
  ["StoreLogo.png", [50, 50]],
]);

await validateNativeIconBundleFingerprint();

for (const [filename, [expectedWidth, expectedHeight]] of pngDimensions) {
  const path = resolve(iconDirectory, filename);
  try {
    const buffer = await readFile(path);
    const signature = buffer.subarray(0, 8).toString("hex");
    if (signature !== "89504e470d0a1a0a") {
      failures.push(`${path}: not a valid PNG signature`);
      continue;
    }
    const width = buffer.readUInt32BE(16);
    const height = buffer.readUInt32BE(20);
    const pngColorType = buffer.readUInt8(25);
    if (width !== expectedWidth || height !== expectedHeight) {
      failures.push(
        `${path}: expected ${expectedWidth}×${expectedHeight}, found ${width}×${height}`,
      );
    }
    if (pngColorType !== 6) {
      failures.push(`${path}: generated icon must be 32-bit RGBA`);
    } else if (pngTopLeftAlpha(buffer) !== 0) {
      failures.push(`${path}: generated icon corner must be transparent`);
    }
    validatePngPalette(path, buffer);
  } catch (error) {
    failures.push(`${path}: ${error.message}`);
  }
}

try {
  const generatedFiles = await readdir(iconDirectory, { recursive: true });
  for (const relativePath of generatedFiles) {
    if (!relativePath.toLowerCase().endsWith(".png")) continue;
    const path = resolve(iconDirectory, relativePath);
    if (pngDimensions.has(relativePath)) continue;
    validatePngPalette(path, await readFile(path));
  }
} catch (error) {
  failures.push(
    `${iconDirectory}: cannot inspect generated PNG families (${error.message})`,
  );
}

for (const filename of ["icon.ico", "icon.icns"]) {
  const path = resolve(iconDirectory, filename);
  try {
    const details = await stat(path);
    if (details.size < 100) failures.push(`${path}: generated bundle is empty`);
    const bundle = await readFile(path);
    for (let offset = 0; offset <= bundle.length - 8; offset += 1) {
      if (
        bundle.subarray(offset, offset + 8).toString("hex") !==
        "89504e470d0a1a0a"
      )
        continue;
      const pngEnd = bundle.indexOf(
        Buffer.from("49454e44ae426082", "hex"),
        offset,
      );
      if (pngEnd >= 0)
        validatePngPalette(path, bundle.subarray(offset, pngEnd + 8));
    }
  } catch (error) {
    failures.push(`${path}: ${error.message}`);
  }
}

try {
  const icoPath = resolve(iconDirectory, "icon.ico");
  const ico = await readFile(icoPath);
  const imageCount = ico.readUInt16LE(4);
  const sizes = new Set();
  for (let index = 0; index < imageCount; index += 1) {
    const entryOffset = 6 + index * 16;
    const width = ico.readUInt8(entryOffset) || 256;
    const height = ico.readUInt8(entryOffset + 1) || 256;
    if (width === height) sizes.add(width);
  }
  for (const requiredSize of [16, 24, 32, 48, 64, 256]) {
    if (!sizes.has(requiredSize)) {
      failures.push(
        `${icoPath}: missing the standard Windows ${requiredSize}×${requiredSize} frame`,
      );
    }
  }
} catch (error) {
  failures.push(`${resolve(iconDirectory, "icon.ico")}: ${error.message}`);
}

const buildScriptPath = resolve("src-tauri/build.rs");
const buildScript = await read(buildScriptPath);
if (
  buildScript !== null &&
  !buildScript.includes("cargo:rerun-if-changed=icons/icon.ico")
) {
  failures.push(
    `${buildScriptPath}: Windows executable must rebuild when icon.ico changes`,
  );
}

if (failures.length > 0) {
  console.error("Identity asset validation failed:\n");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log(
    "Validated the Quantix Evidence Register, transparent splash, favicon, and generated icon bundle.",
  );
}
