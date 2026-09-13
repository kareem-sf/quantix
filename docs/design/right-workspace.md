# Right workspace redesign — 12 September 2026

The user's supplied ChatGPT desktop screenshots govern this redesign of the entire area to the right of the Manager conversation, including its global toolbar, context popover, content launcher, tabs and expanded view. This replaces the permanently open office-card resting state. Keep the conversation, draft text, source identity and all existing engineering authority intact.

## Layout and behavior

Use a white/light or semantic dark canvas, a 44px quiet top toolbar, fine dividers, small monochrome icons, rounded hover backgrounds and spacious content. Right-aligned actions: More options, Reviews, Tender context, Show/hide workspace. Context is a rounded anchored popover with Tender status and source rows, Import and View all. Tailor actions to real Quantix capabilities; do not invent Share, Fork, Archive, terminal or account permissions.

The right canvas starts with a centered launcher for Documents, Office, Reviews and Activity. Opening one creates a tab; the tab strip provides a home action, expand/restore and close-tab controls. The entire workspace can collapse so chat occupies the width, or expand so the workspace occupies the main area. Desktop split retains draggable/keyboard resizing. Narrow screens display the open workspace as the main surface. Manager and visited tabs stay mounted during hide/expand/tab changes. Closing a tab is an explicit close; source links reopen Documents and results reopen Reviews, including from a collapsed state. State from one Tender must not leak into another. Persist only layout preference, not private source content.

Documents offers an uncluttered searchable file list and the real inline source viewer, preserving versions, pages, sheets, ranges, zoom, download and measurement. The full document register remains reachable. Office uses the existing live staff/desk implementation with compact unboxed rows, a full-width selected desk and an optional office conversation. Reviews uses real plan review, findings and saved-result components. Activity uses real run/system history with existing controls. Compatibility errors remain actionable; controls must not imply staff readiness while incompatible.

## Components and validation

Reuse installed shadcn/Base UI Button, Tabs, DropdownMenu, Popover, Input, Kbd and Separator; Amicro action buttons/loaders already in the project; Transitions.dev's short panel-reveal timing. Preserve the existing narrow-hit-area SplitPane because its documented replacement previously swallowed edge clicks; extend it to retain mounted panes while hidden. No new bitmap UI or decorative chart/carousel assets are needed for this reference-driven code-native interface.

Use exact visible labels, keyboard focus, accessible names, reduced-motion handling and shortcuts that do not intercept typing. Test hide/show, expand/restore, tab closing/switching, draft preservation, source/result opening, Tender scoping, compatibility and keyboard behavior. Verify light/dark, narrow windows and scaling in the running development app. No release package, real Tender approval, account mutation or commercial send.
