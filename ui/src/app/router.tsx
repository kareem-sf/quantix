import { useEffect } from "react";
import { Navigate, useParams, useRouteError, useSearchParams, type RouteObject } from "react-router";
import { Details } from "../company/Details";
import { Rules } from "../company/Rules";
import { Documents } from "../documents/Documents";
import { Estimate } from "../estimate/Estimate";
import { Library } from "../library/Library";
import { TenderQueries } from "../takeoff/TenderQueries";
import { Takeoff } from "../takeoff/Takeoff";
import { Settings } from "../settings/Settings";
import { Directory } from "../subcontract/Directory";
import { Subcontract } from "../subcontract/Subcontract";
import { Submission } from "../submission/Submission";
import { NewTender } from "../tenders/NewTender";
import { Desk } from "../tenders/Desk";
import { Overview } from "../tenders/Overview";
import { Tenders } from "../tenders/Tenders";
import { useTenders } from "../tenders/queries";
import { TEAM, useDecisions } from "../office/queries";
import { About } from "../about/About";
import { Opening } from "./Opening";
import { Shell } from "./Shell";
import { useShell } from "./context";

/** A screen that failed: said in its own area, with the sidebar and title bar still there to go elsewhere. */
function ScreenError() {
  const error = useRouteError();
  console.error(error);
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-ink-2">
      <p className="text-sm">Something went wrong on this screen.</p>
      <button onClick={() => window.location.reload()} className="font-medium text-ink underline underline-offset-4">
        Try again
      </button>
    </div>
  );
}

/** Quantix opens on the Desk, or on starting the first tender. */
function Home() {
  const tenders = useTenders();
  if (!tenders.data) return <Opening error={tenders.isError} />;
  return <Navigate to={tenders.data.length ? "/desk" : "/new"} replace />;
}

/** A question is answered where it was asked: an old link to its page opens the asker's chat beside the Overview. */
function OpenDecision() {
  const { tenderId = "", decisionId } = useParams();
  const decisions = useDecisions(tenderId);
  const { showTeam } = useShell();
  const asker = decisions.data?.find((d) => d.id === decisionId)?.raised_by;
  useEffect(() => {
    if (decisions.data) showTeam(asker ?? null);
  }, [decisions.data]); // eslint-disable-line react-hooks/exhaustive-deps
  return decisions.data ? <Navigate to={`/tenders/${tenderId}`} replace /> : null;
}

/** The Office screen became the team panel: an old link to it opens the panel on that chat, over the Overview. */
function OpenTeam() {
  const { tenderId } = useParams();
  const [params] = useSearchParams();
  const { showTeam } = useShell();
  const channel = params.get("with") ?? TEAM; // the old Office opened on the team room
  useEffect(() => showTeam(channel), []); // eslint-disable-line react-hooks/exhaustive-deps
  return <Navigate to={`/tenders/${tenderId}`} replace />;
}

export const routes: RouteObject[] = [
  {
    element: <Shell />,
    errorElement: <ScreenError />,
    children: [
      {
        errorElement: <ScreenError />,
        children: [
          { path: "/", element: <Home /> },
          { path: "/desk", element: <Desk /> },
          { path: "/tenders", element: <Tenders /> },
          { path: "/new", element: <NewTender /> },
          { path: "/tenders/:tenderId", element: <Overview /> },
          { path: "/tenders/:tenderId/documents", element: <Documents /> },
          { path: "/tenders/:tenderId/office", element: <OpenTeam /> },
          { path: "/tenders/:tenderId/takeoff", element: <Takeoff /> },
          { path: "/tenders/:tenderId/queries", element: <TenderQueries /> },
          { path: "/tenders/:tenderId/estimate", element: <Estimate /> },
          { path: "/tenders/:tenderId/subcontract", element: <Subcontract /> },
          { path: "/tenders/:tenderId/submission", element: <Submission /> },
          {
            path: "/tenders/:tenderId/decisions/:decisionId",
            element: <OpenDecision />,
          },
          { path: "/settings", element: <Settings /> },
          { path: "/about", element: <About /> },
          { path: "/library", element: <Library /> },
          { path: "/directory", element: <Directory /> },
          { path: "/rules", element: <Rules /> },
          { path: "/company", element: <Details /> },
        ],
      },
    ],
  },
];
