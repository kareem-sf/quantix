import light from "../design/brand/v4/03-icons/svg/Quantix-Icon-Light.svg";
import dark from "../design/brand/v4/03-icons/svg/Quantix-Icon-Dark.svg";
import lightFlat from "../design/brand/v4/03-icons/svg/Quantix-Icon-Light-Flat.svg";
import darkFlat from "../design/brand/v4/03-icons/svg/Quantix-Icon-Dark-Flat.svg";

/** Original v4 artwork. These imports are bundled locally by Vite. */
export const brandMarks = {
  light: { dimensional: light, flat: lightFlat },
  dark: { dimensional: dark, flat: darkFlat },
} as const;

export function syncBrandMetadata(theme: "light" | "dark") {
  const root = `${import.meta.env.BASE_URL}brand/v4/web/${theme}`;
  for (const [id, file] of [
    ["quantix-favicon", "favicon.svg"],
    ["quantix-touch-icon", "apple-touch-icon.png"],
    ["quantix-manifest", "site.webmanifest"],
  ])
    document.getElementById(id)?.setAttribute("href", `${root}/${file}`);
}
