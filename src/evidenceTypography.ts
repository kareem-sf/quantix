import type { EvidenceLanguage } from "./bindings/EvidenceLanguage";
import type { EvidenceLocation } from "./bindings/EvidenceLocation";
import type { TextDirection } from "./bindings/TextDirection";

type EvidenceTextAttributes = {
  dir: "auto" | "ltr" | "rtl";
  lang?: "ar" | "en";
};

export function evidenceLanguageTag(
  language: EvidenceLanguage | string,
): EvidenceTextAttributes["lang"] {
  switch (language.trim().toLowerCase()) {
    case "arabic":
      return "ar";
    case "english":
      return "en";
    default:
      return undefined;
  }
}

export function evidenceTextDirection(
  direction: TextDirection | string,
): EvidenceTextAttributes["dir"] {
  switch (direction.trim().toLowerCase()) {
    case "right_to_left":
      return "rtl";
    case "left_to_right":
      return "ltr";
    default:
      return "auto";
  }
}

export function evidenceTextAttributes(
  location: Pick<EvidenceLocation, "direction" | "language">,
): EvidenceTextAttributes {
  return {
    dir: evidenceTextDirection(location.direction),
    lang: evidenceLanguageTag(location.language),
  };
}
