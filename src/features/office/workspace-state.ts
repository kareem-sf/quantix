import { useCallback, useEffect, useReducer } from "react";
import type { SourceSelection } from "../Sources";

export function sourceSelectionKey(selection: SourceSelection) {
  return JSON.stringify([
    "sourceId" in selection ? selection.sourceId : null,
    selection.artifactId,
    selection.version,
    selection.contentHash,
  ]);
}

export type WorkspaceView = "documents" | "office" | "reviews" | "activity" | "research";
export type WorkspaceMode = "split" | "hidden" | "expanded";
type State = {
  view: WorkspaceView | "home";
  tabs: WorkspaceView[];
  mode: WorkspaceMode;
};
type Action =
  | { type: "open"; view: WorkspaceView }
  | { type: "home" | "toggle" | "hide" | "expand" | "close" };
// v2: the default became collapsed, so the saved "open" every existing
// workspace already carried had to stop deciding the first view.
const STORAGE_KEY = "quantix.right-workspace.v2";

function initialState(): State {
  // The conversation is the workspace. The side pane starts out of the way and
  // stays open only once it has been opened on purpose, which the effect below
  // remembers.
  let mode: WorkspaceMode = "hidden";
  try {
    if (localStorage.getItem(STORAGE_KEY) === "split") mode = "split";
  } catch {
    /* Layout still works when storage is unavailable. */
  }
  return { view: "home", tabs: [], mode };
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "open":
      return {
        ...state,
        view: action.view,
        tabs: state.tabs.includes(action.view)
          ? state.tabs
          : [...state.tabs, action.view],
        mode: state.mode === "hidden" ? "split" : state.mode,
      };
    case "home":
      return { ...state, view: "home" };
    case "hide":
      return { ...state, mode: "hidden" };
    case "toggle":
      return { ...state, mode: state.mode === "hidden" ? "split" : "hidden" };
    case "expand":
      return {
        ...state,
        mode: state.mode === "expanded" ? "split" : "expanded",
      };
    case "close": {
      const tabs = state.tabs.filter((tab) => tab !== state.view);
      return { ...state, tabs, view: tabs.at(-1) ?? "home" };
    }
  }
}

export function useWorkspaceState() {
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  useEffect(() => {
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
  const home = useCallback(() => dispatch({ type: "home" }), []);
  const toggle = useCallback(() => dispatch({ type: "toggle" }), []);
  const hide = useCallback(() => dispatch({ type: "hide" }), []);
  const expand = useCallback(() => dispatch({ type: "expand" }), []);
  const close = useCallback(() => dispatch({ type: "close" }), []);
  return { ...state, open, home, toggle, hide, expand, close };
}
export type WorkspaceController = ReturnType<typeof useWorkspaceState>;
