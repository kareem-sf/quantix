# Quantix v4 app identity

The complete supplied `Quantix-Transparent-Brand-Package-v4` is preserved, unchanged, in [`v4/`](v4/README.md): 96 files, 12,626,918 bytes. SHA-256 comparison against the original Desktop package found no differences. All 62 PNG exports were checked for transparent corners. Keep the original directory contents intact; application decisions and derived copies live outside it.

## Application mapping

- `src/brand.ts` imports the original Light/Dark icon SVGs directly from the preserved package. Vite bundles these as local app assets. `BrandMark` uses the supplied theme variant; compact marks at 24px or smaller use the supplied flat version. No raster regeneration or geometry editing.
- The sidebar, Manager message avatar, empty conversation, startup, reset recovery and About use that shared mark. The old magnifying-glass placeholder and its solid background tiles have been removed. The visible name is **Quantix**. Logo backgrounds and center openings remain transparent.
- The complete supplied web icon set is copied to `public/brand/v4/web`. The main app switches its favicon, touch icon and manifest with its resolved theme, including an explicit theme different from the OS preference. The companion page uses the package's default transparent favicon.
- The supplied desktop icon sets are copied to `src-tauri/icons/brand-v4`. Tauri uses the dark/yellow set for the executable, window and tray so the mark remains recognizable on the OS chrome. Both provided light and dark sets are retained. No release package was built.

## Palette and typography

`src/styles/brand.css` owns the app's semantic colors. Yellow `#FFD21F` comes from the flat dark artwork; graphite `#1D2025`/`#2D3137` and pearl `#F8F8F5` come from the dimensional artwork. The yellow is used for prominent controls, with graphite text. Tonal backgrounds and darker gold text/focus colors are application derivatives, not alterations to the original artwork. Destructive and other status colors remain semantic.

Contrast checks: graphite on yellow buttons **11.27:1**; light-theme gold links `#775700` on white **6.67:1**; dark-theme gold links on graphite **13.01:1**; primary text **13.08:1** light / **15.35:1** dark. Legacy solid red/gold controls retain white text in light mode and graphite in dark mode, independently of yellow CTA text.

The package contains no fonts or wordmark artwork. The existing Geist interface typography and language fallbacks are retained. The unrelated historical Lastoria file is not introduced as a v4 font.

## Maintenance

Use the shared `BrandMark`; do not wrap it in a colored tile or redraw the Q. Preserve aspect ratio. Keep legacy link colors separate from yellow action backgrounds so yellow text is not placed on a white surface. For a future package update, retain this source directory and add a new version before switching the app mapping. Historical v1/v2 work remains available but does not define the active identity.
