# Right workspace implementation plan

**Goal:** Match the supplied desktop workspace interaction model using Quantix's real workflows.
**Spec:** `docs/design/right-workspace.md`
**Architecture:** Small reusable workspace chrome/controller and workflow views, integrated into TenderOfficeWorkspace. Keep source/result APIs and authority unchanged.

- [x] Write behavior tests for collapsed/expanded modes, source/result navigation, keyboard access, tab lifecycle and Manager draft preservation.
- [x] Add workspace chrome with toolbar, More options menu, context popover, launcher, tabs and layout persistence. Extend SplitPane visibility without remounting its children.
- [x] Integrate document list/viewer, existing plan/findings review, run history and live office. Adapt embedded office presentation to compact rows and a focused desk. Keep existing feature handlers and guardrails.
- [x] Run affected UI suites and typecheck; review all changes and verify real-app journeys in light/dark and narrow views. Record validation and any native-specific limits; keep the user's debug app running.
