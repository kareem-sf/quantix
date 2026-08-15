from __future__ import annotations

import argparse
from pathlib import Path

from docling.datamodel.accelerator_options import AcceleratorDevice, AcceleratorOptions
from docling.datamodel.base_models import InputFormat
from docling.datamodel.object_detection_engine_options import (
    OnnxRuntimeObjectDetectionEngineOptions,
)
from docling.datamodel.pipeline_options import (
    LayoutObjectDetectionOptions,
    OcrMode,
    PdfPipelineOptions,
    RapidOcrOptions,
)
from docling.document_converter import (
    DocumentConverter,
    ImageFormatOption,
    PdfFormatOption,
)
from docling_core.types.doc import ImageRefMode


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument(
        "--input-format",
        choices=(InputFormat.PDF.value, InputFormat.DOCX.value, InputFormat.XLSX.value),
        required=True,
    )
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--artifacts-path", type=Path, required=True)
    parser.add_argument("--document-timeout", type=float, required=True)
    parser.add_argument("--num-threads", type=int, required=True)
    parser.add_argument(
        "--ocr-mode",
        choices=(OcrMode.DEFAULT.value, OcrMode.FULL_PAGE.value),
        default=OcrMode.DEFAULT.value,
    )
    parser.add_argument("--ocr-lang", default="ch")
    return parser.parse_args()


def pdf_options(options: argparse.Namespace) -> PdfPipelineOptions:
    layout = LayoutObjectDetectionOptions.from_preset("layout_heron_default")
    layout.engine_options = OnnxRuntimeObjectDetectionEngineOptions()
    return PdfPipelineOptions(
        artifacts_path=options.artifacts_path,
        document_timeout=options.document_timeout,
        accelerator_options=AcceleratorOptions(
            num_threads=options.num_threads,
            device=AcceleratorDevice.CPU,
        ),
        enable_remote_services=False,
        allow_external_plugins=False,
        ocr_options=RapidOcrOptions(
            mode=OcrMode(options.ocr_mode),
            lang=[options.ocr_lang],
        ),
        layout_options=layout,
    )


def main() -> None:
    options = arguments()
    input_format = InputFormat(options.input_format)
    pipeline_options = pdf_options(options)
    converter = DocumentConverter(
        allowed_formats=[input_format],
        format_options={
            InputFormat.PDF: PdfFormatOption(pipeline_options=pipeline_options),
            InputFormat.IMAGE: ImageFormatOption(pipeline_options=pipeline_options),
        },
    )
    result = converter.convert(options.input, raises_on_error=True)
    options.output_dir.mkdir(parents=True, exist_ok=True)
    result.document.save_as_json(
        options.output_dir / f"{options.input.stem}.json",
        image_mode=ImageRefMode.PLACEHOLDER,
    )


if __name__ == "__main__":
    main()
