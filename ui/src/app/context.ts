import { createContext, useContext } from "react";
import type { Fit } from "./layout";

/** The team panel: open or not, whose chat it shows (a person's id, "team", or null for the Tender Manager), and a
 * profile opened over it. */
export type Team = { open: boolean; channel: string | null; person: string | null };

export type Shell = {
  team: Team;
  showTeam: (channel?: string | null) => void;
  hideTeam: () => void;
  toggleTeam: () => void;
  showPerson: (id: string | null) => void;
  folded: boolean;
  toggleSidebar: () => void;
  openPalette: () => void;
  openShortcuts: () => void;
  fit: Fit;
  /** In a narrow window the sidebar opens over the screen, as a drawer. */
  drawer: boolean;
  closeDrawer: () => void;
  sidebarWidth: number;
  setSidebarWidth: (width: number) => void;
  teamWidth: number;
  setTeamWidth: (width: number) => void;
};

/** The usual widths, which a double-click on an edge brings back, and how far each can be dragged. */
export const SIDEBAR = { usual: 224, least: 184, most: 360, folded: 52 };
export const TEAM_PANEL = { usual: 400, least: 320, most: 720 };

export const ShellContext = createContext<Shell | null>(null);

export function useShell(): Shell {
  const shell = useContext(ShellContext);
  if (!shell) throw new Error("useShell is used outside the Quantix shell");
  return shell;
}
