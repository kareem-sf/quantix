import { useCallback, useEffect, useReducer, useRef } from "react";
import type { SourceSelection } from "../Sources";

export function sourceSelectionKey(selection: SourceSelection) {
  return JSON.stringify([
    "sourceId" in selection ? selection.sourceId : null,
    selection.artifactId,
    selection.version,
    selection.contentHash,
  ]);
}

export type WorkspaceView = "plan" | "team" | "documents" | "activity";
export const WORKSPACE_TABS: WorkspaceView[] = [
  "plan",
  "team",
  "documents",
  "activity",
];
export type WorkspaceMode = "split" | "hidden" | "expanded";
type State = {
  view: WorkspaceView;
  tabs: WorkspaceView[];
  mode: WorkspaceMode;
};
type Action =
  | { type: "open"; view: WorkspaceView }
  | { type: "toggle" | "hide" | "expand" };
// v2: the default became collapsed, so the saved "open" every existing
// workspace already carried had to stop deciding the first view.
const STORAGE_KEY = "quantix.right-workspace.v2";
/** Matches useIsMobile: below this the pane and the conversation cannot sit side by side. */
const NARROW_WIDTH = 768;

function initialState(): State {
  // The conversation is the workspace. The side pane starts out of the way and
  // stays open only once it has been opened on purpose, which the effect below
  // remembers.
  // On a narrow window the open pane replaces the conversation, so it only
  // reopens by itself when there is room for both.
  let mode: WorkspaceMode = "hidden";
  try {
    if (
      localStorage.getItem(STORAGE_KEY) === "split" &&
      window.innerWidth >= NARROW_WIDTH
    )
      mode = "split";
  } catch {
    /* Layout still works when storage is unavailable. */
  }
  return { view: "plan", tabs: WORKSPACE_TABS, mode };
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "open":
      return {
        ...state,
        view: action.view,
        mode: state.mode === "hidden" ? "split" : state.mode,
      };
    case "hide":
      return { ...state, mode: "hidden" };
    case "toggle":
      return { ...state, mode: state.mode === "hidden" ? "split" : "hidden" };
    case "expand":
      return {
        ...state,
        mode: state.mode === "expanded" ? "split" : "expanded",
      };
  }
}

export function useWorkspaceState() {
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  const initialMode = useRef<WorkspaceMode | null>(state.mode);
  useEffect(() => {
    // Only the engineer's own choice is remembered, so starting closed on a
    // narrow window keeps the choice made on a wide one.
    if (initialMode.current === state.mode) return;
    initialMode.current = null;
    try {
      localStorage.setItem(
        STORAGE_KEY,
        state.mode === "hidden" ? "hidden" : "split",
      );
    } catch {
      /* Session state remains usable. */
    }
  }, [state.mode]);
  const open = useCallback(
    (view: WorkspaceView) => dispatch({ type: "open", view }),
    [],
  );
  const toggle = useCallback(() => dispatch({ type: "toggle" }), []);
  const hide = useCallback(() => dispatch({ type: "hide" }), []);
  const expand = useCallback(() => dispatch({ type: "expand" }), []);
  return { ...state, open, toggle, hide, expand };
}
export type WorkspaceController = ReturnType<typeof useWorkspaceState>;
