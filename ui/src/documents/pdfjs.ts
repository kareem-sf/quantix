import * as pdfjs from "pdfjs-dist";
import worker from "pdfjs-dist/build/pdf.worker.min.mjs?url";

// PDF.js's viewer component reads the library from globalThis when it loads, so this module is imported before it.
pdfjs.GlobalWorkerOptions.workerSrc = worker;
(globalThis as { pdfjsLib?: typeof pdfjs }).pdfjsLib = pdfjs;

/** Where PDF.js finds its character maps, standard fonts, colour profiles and image decoders (see vite.config.ts). */
export const ASSETS = {
  cMapUrl: "/pdfjs/cmaps/",
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  iccUrl: "/pdfjs/iccs/",
  wasmUrl: "/pdfjs/wasm/",
};

export { pdfjs };
