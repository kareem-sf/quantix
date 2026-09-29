import { useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate, useParams } from "react-router";
import { TeamPanel } from "../office/Office";
import { useTenders } from "../tenders/queries";
import { useNotifications } from "./notify";
import { lastTender, remember, store, stored } from "./place";
import { Palette } from "./Palette";
import { Rail } from "./Rail";
import { Shortcuts } from "./Shortcuts";
import { TENDER_SCREENS } from "./screens";
import { SIDEBAR, ShellContext, TEAM_PANEL, type Shell as ShellState, type Team } from "./context";
import { useFit, usePresence } from "./layout";
import { TitleBar } from "./TitleBar";

/** The window: Quantix's title bar, the sidebar, the screen, and the team beside it. The sidebar and the team follow
 * the tender on screen, or the last one opened when the screen belongs to the firm. Both edges can be dragged; in a
 * narrower window the team floats over the screen and the sidebar folds, then opens over the screen as a drawer. */
export function Shell() {
  const { tenderId } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const tenders = useTenders();
  const tender = tenders.data?.find((t) => t.id === (tenderId ?? lastTender()));

  const [team, setTeam] = useState<Team>(() => ({ open: stored("team", false), channel: null, person: null }));
  const [folded, setFolded] = useState(() => stored("folded", false));
  const [unfolded, setUnfolded] = useState(false); // on Takeoff the sidebar folds for the drawing, unless opened there
  const [palette, setPalette] = useState(false);
  const [shortcuts, setShortcuts] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(() => stored("sidebarWidth", SIDEBAR.usual));
  const [teamWidth, setTeamWidth] = useState(() => stored("teamWidth", TEAM_PANEL.usual));
  const fit = useFit();
  const docked = fit === "wide" || fit === "medium";
  const onTakeoff = location.pathname.endsWith("/takeoff");
  const panel = usePresence(team.open && Boolean(tender));
  useNotifications();

  useEffect(() => remember(location.pathname + location.search), [location]);
  useEffect(() => {
    setUnfolded(false);
    if (onTakeoff) setTeam((t) => ({ ...t, open: false, person: null })); // the drawing gets the room; Ctrl+J reopens
  }, [onTakeoff]);
  useEffect(() => store("team", team.open), [team.open]);
  useEffect(() => store("folded", folded), [folded]);
  useEffect(() => store("sidebarWidth", sidebarWidth), [sidebarWidth]);
  useEffect(() => store("teamWidth", teamWidth), [teamWidth]);
  useEffect(() => setDrawer(false), [location.pathname, fit]); // the drawer closes once it has taken the engineer somewhere

  const shell: ShellState = {
    team,
    showTeam: (channel = null) => setTeam({ open: true, channel: channel ?? team.channel, person: null }),
    hideTeam: () => setTeam({ ...team, open: false, person: null }),
    toggleTeam: () => setTeam({ ...team, open: !team.open, person: null }),
    showPerson: (person) => setTeam({ ...team, open: true, person }),
    folded: docked ? (onTakeoff ? !unfolded : folded) : !drawer,
    toggleSidebar: () => (!docked ? setDrawer(!drawer) : onTakeoff ? setUnfolded(!unfolded) : setFolded(!folded)),
    openPalette: () => setPalette(true),
    openShortcuts: () => setShortcuts(true),
    fit,
    drawer,
    closeDrawer: () => setDrawer(false),
    sidebarWidth,
    // dragged well past its narrowest, the sidebar folds to its icons
    setSidebarWidth: (width) => {
      if (width >= SIDEBAR.least - 40 || !docked) setSidebarWidth(clamp(width, SIDEBAR.least, SIDEBAR.most));
      else if (onTakeoff) setUnfolded(false);
      else setFolded(true);
    },
    teamWidth,
    setTeamWidth: (width) => setTeamWidth(clamp(width, TEAM_PANEL.least, TEAM_PANEL.most)),
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // the key's place, not its letter: the shortcuts work the same with an Arabic keyboard layout
      const key = e.code.replace(/^(Key|Digit)/, "").toLowerCase();
      if (e.ctrlKey && !e.shiftKey && !e.altKey) {
        if (key === "k") setPalette((open) => !open);
        else if (key === "slash") setShortcuts((open) => !open);
        else if (key === "j") setTeam((t) => ({ ...t, open: !t.open, person: null }));
        else if (key === "b") (!docked ? setDrawer : onTakeoff ? setUnfolded : setFolded)((f) => !f);
        else if (tender && /^[1-7]$/.test(key)) navigate(`/tenders/${tender.id}${TENDER_SCREENS[Number(key) - 1][1]}`);
        else return;
        e.preventDefault();
      } else if (e.altKey && !e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        e.preventDefault();
        navigate(e.key === "ArrowLeft" ? -1 : 1);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [navigate, onTakeoff, tender, docked]);

  return (
    <ShellContext.Provider value={shell}>
      <div className="flex h-full flex-col">
        <TitleBar tender={tender} />
        <div className="relative flex min-h-0 grow">
          <Rail tender={tender} />
          <main
            key={location.pathname}
            className="@container flex min-w-0 grow animate-enter flex-col items-center-safe overflow-y-auto"
          >
            <Outlet />
          </main>
          {panel.shown && tender && <TeamPanel tenderId={tender.id} leaving={panel.leaving} />}
        </div>
      </div>
      {palette && <Palette tender={tender} onClose={() => setPalette(false)} />}
      {shortcuts && <Shortcuts onClose={() => setShortcuts(false)} />}
    </ShellContext.Provider>
  );
}

function clamp(width: number, least: number, most: number): number {
  return Math.min(most, Math.max(least, Math.round(width)));
}
