import { createContext, useContext } from "react";

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
};

export const ShellContext = createContext<Shell | null>(null);

export function useShell(): Shell {
  const shell = useContext(ShellContext);
  if (!shell) throw new Error("useShell is used outside the Quantix shell");
  return shell;
}
