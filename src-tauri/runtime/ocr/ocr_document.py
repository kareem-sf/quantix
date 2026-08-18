from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
from pathlib import Path

import numpy
import pypdfium2 as pdfium
from rapidocr import EngineType, RapidOCR

_SCALE = 2


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--artifacts-path", type=Path, required=True)
    parser.add_argument("--document-timeout", type=float, required=True)
    parser.add_argument("--max-file-size", type=int, required=True)
    parser.add_argument("--max-num-pages", type=int, required=True)
    parser.add_argument("--num-threads", type=int, required=True)
    return parser.parse_args()


def engine_params(artifacts: Path, num_threads: int) -> dict[str, object]:
    return {
        "EngineConfig.onnxruntime.intra_op_num_threads": num_threads,
        "Det.model_path": str(artifacts / "PP-OCRv6_det_small.onnx"),
        "Det.engine_type": EngineType.ONNXRUNTIME,
        "Cls.model_path": str(artifacts / "ch_ppocr_mobile_v2.0_cls_mobile.onnx"),
        "Cls.engine_type": EngineType.ONNXRUNTIME,
        "Rec.model_path": str(artifacts / "PP-OCRv6_rec_small.onnx"),
        "Rec.engine_type": EngineType.ONNXRUNTIME,
    }


def recognized_lines(engine: RapidOCR, page: pdfium.PdfPage) -> list[dict[str, object]]:
    bitmap = page.render(scale=_SCALE)
    image = numpy.array(bitmap.to_pil())
    result = engine(image, use_det=True, use_cls=True, use_rec=True)
    if result is None or result.boxes is None or result.txts is None:
        return []
    lines: list[dict[str, object]] = []
    for box, text in zip(result.boxes.tolist(), result.txts):
        if not text.strip():
            continue
        coordinates = numpy.array(box)
        lines.append(
            {
                "top": float(coordinates[:, 1].min()) / _SCALE,
                "bottom": float(coordinates[:, 1].max()) / _SCALE,
                "left": float(coordinates[:, 0].min()) / _SCALE,
                "right": float(coordinates[:, 0].max()) / _SCALE,
                "text": text,
            }
        )
    return lines


def reading_order(lines: list[dict[str, object]]) -> list[dict[str, object]]:
    if not lines:
        return lines
    ordered = sorted(lines, key=lambda line: (line["top"], line["left"]))
    heights = [line["bottom"] - line["top"] for line in ordered]
    band = max(statistics.median(heights) * 0.8, 4.0)
    clusters: list[list[dict[str, object]]] = []
    for line in ordered:
        if not clusters or line["top"] - clusters[-1][-1]["top"] > band:
            clusters.append([line])
        else:
            clusters[-1].append(line)
    for cluster in clusters:
        cluster.sort(key=lambda line: line["left"])
    return [line for cluster in clusters for line in cluster]


def paragraph_groups(lines: list[dict[str, object]]) -> list[list[dict[str, object]]]:
    if not lines:
        return []
    heights = [line["bottom"] - line["top"] for line in lines]
    gap_threshold = max(statistics.median(heights) * 1.6, 8.0)
    groups: list[list[dict[str, object]]] = [[lines[0]]]
    for line in lines[1:]:
        if line["top"] - groups[-1][-1]["bottom"] > gap_threshold:
            groups.append([line])
        else:
            groups[-1].append(line)
    return groups


def main() -> None:
    options = arguments()
    if options.input.stat().st_size > options.max_file_size:
        sys.exit("input exceeds the approved maximum file size")
    deadline = time.monotonic() + options.document_timeout
    document = pdfium.PdfDocument(options.input)
    try:
        if len(document) > options.max_num_pages:
            sys.exit("input exceeds the approved maximum page count")
        engine = RapidOCR(params=engine_params(options.artifacts_path, options.num_threads))
        markdown: list[str] = []
        locations: list[dict[str, object]] = []
        for page_number, page in enumerate(document, start=1):
            if time.monotonic() > deadline:
                sys.exit("document conversion exceeded its time budget")
            lines = reading_order(recognized_lines(engine, page))
            for group in paragraph_groups(lines):
                text = "\n".join(str(line["text"]) for line in group)
                char_start = sum(len(part) + 2 for part in markdown)
                markdown.append(text)
                locations.append(
                    {
                        "kind": "paragraph",
                        "text": text,
                        "page": page_number,
                        "char_start": char_start,
                        "char_end": char_start + len(text),
                        "bbox": [
                            min(line["left"] for line in group),
                            min(line["top"] for line in group),
                            max(line["right"] for line in group),
                            max(line["bottom"] for line in group),
                        ],
                    }
                )
        options.output_dir.mkdir(parents=True, exist_ok=True)
        stem = options.input.stem
        (options.output_dir / f"{stem}.md").write_text("\n\n".join(markdown) + "\n", encoding="utf-8")
        (options.output_dir / f"{stem}.locations.json").write_text(
            json.dumps({"schema_version": 1, "locations": locations}, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
    finally:
        document.close()


if __name__ == "__main__":
    main()
