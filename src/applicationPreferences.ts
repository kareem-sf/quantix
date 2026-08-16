import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";

export const DEFAULT_GENERAL_APPLICATION_PREFERENCES: GeneralApplicationPreferences =
  {
    appearance: "system",
    reduced_motion: false,
    high_contrast: false,
    larger_text: false,
    notify_when_attention_needed: false,
  };

export function applyGeneralApplicationPreferences(
  preferences: GeneralApplicationPreferences,
) {
  const root = document.documentElement;
  root.dataset.quantixAppearance = preferences.appearance;
  root.classList.toggle("quantix-reduced-motion", preferences.reduced_motion);
  root.classList.toggle("quantix-high-contrast", preferences.high_contrast);
  root.classList.toggle("quantix-larger-text", preferences.larger_text);
}
