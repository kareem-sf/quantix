import { useEffect, useState } from "react";
import { Outlet, useLocation, useNavigate, useParams } from "react-router";
import { TeamPanel } from "../office/Office";
import { useTenders } from "../tenders/queries";
import { useNotifications } from "./notify";
import { lastTender, remember, store, stored } from "./place";
import { Palette } from "./Palette";
import { Rail } from "./Rail";
import { TENDER_SCREENS } from "./screens";
import { ShellContext, type Shell as ShellState, type Team } from "./context";
import { TitleBar } from "./TitleBar";

/** The window: Quantix's title bar, the sidebar, the screen, and the team beside it. The sidebar and the team follow
 * the tender on screen, or the last one opened when the screen belongs to the firm. */
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
  const onTakeoff = location.pathname.endsWith("/takeoff");
  useNotifications();

  useEffect(() => remember(location.pathname + location.search), [location]);
  useEffect(() => {
    setUnfolded(false);
    if (onTakeoff) setTeam((t) => ({ ...t, open: false, person: null })); // the drawing gets the room; Ctrl+J reopens
  }, [onTakeoff]);
  useEffect(() => store("team", team.open), [team.open]);
  useEffect(() => store("folded", folded), [folded]);

  const shell: ShellState = {
    team,
    showTeam: (channel = null) => setTeam({ open: true, channel: channel ?? team.channel, person: null }),
    hideTeam: () => setTeam({ ...team, open: false, person: null }),
    toggleTeam: () => setTeam({ ...team, open: !team.open, person: null }),
    showPerson: (person) => setTeam({ ...team, open: true, person }),
    folded: onTakeoff ? !unfolded : folded,
    toggleSidebar: () => (onTakeoff ? setUnfolded(!unfolded) : setFolded(!folded)),
    openPalette: () => setPalette(true),
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // the key's place, not its letter: the shortcuts work the same with an Arabic keyboard layout
      const key = e.code.replace(/^(Key|Digit)/, "").toLowerCase();
      if (e.ctrlKey && !e.shiftKey && !e.altKey) {
        if (key === "k") setPalette((open) => !open);
        else if (key === "j") setTeam((t) => ({ ...t, open: !t.open, person: null }));
        else if (key === "b") (onTakeoff ? setUnfolded : setFolded)((f) => !f);
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
  }, [navigate, onTakeoff, tender]);

  return (
    <ShellContext.Provider value={shell}>
      <div className="flex h-full flex-col">
        <TitleBar tender={tender} />
        <div className="relative flex min-h-0 grow">
          <Rail tender={tender} />
          <main className="flex min-w-0 grow flex-col items-center-safe overflow-y-auto">
            <Outlet />
          </main>
          {team.open && tender && <TeamPanel tenderId={tender.id} />}
        </div>
      </div>
      {palette && <Palette tender={tender} onClose={() => setPalette(false)} />}
    </ShellContext.Provider>
  );
}
