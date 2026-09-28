import { useEffect } from "react";
import {
  Navigate,
  useParams,
  useRouteError,
  useSearchParams,
  type RouteObject,
} from "react-router";
import { Details } from "../company/Details";
import { Rules } from "../company/Rules";
import { Documents } from "../documents/Documents";
import { Estimate } from "../estimate/Estimate";
import { Library } from "../library/Library";
import { TenderQueries } from "../takeoff/TenderQueries";
import { Takeoff } from "../takeoff/Takeoff";
import { DecisionPage } from "../office/DecisionPage";
import { Settings } from "../settings/Settings";
import { Directory } from "../subcontract/Directory";
import { Subcontract } from "../subcontract/Subcontract";
import { Submission } from "../submission/Submission";
import { NewTender } from "../tenders/NewTender";
import { Overview } from "../tenders/Overview";
import { useTenders } from "../tenders/queries";
import { TEAM } from "../office/queries";
import { Opening } from "./Opening";
import { lastTender, placeIn } from "./place";
import { Shell } from "./Shell";
import { useShell } from "./context";

/** A screen that failed: said in its own area, with the sidebar and title bar still there to go elsewhere. */
function ScreenError() {
  const error = useRouteError();
  console.error(error);
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-ink-2">
      <p className="text-sm">Something went wrong on this screen.</p>
      <button
        onClick={() => window.location.reload()}
        className="font-medium text-ink underline underline-offset-4"
      >
        Try again
      </button>
    </div>
  );
}

/** Quantix opens where the engineer left off: the last tender, on the screen they were on. */
function Home() {
  const tenders = useTenders();
  if (!tenders.data) return <Opening error={tenders.isError} />;
  const last =
    tenders.data.find((t) => t.id === lastTender()) ?? tenders.data[0];
  return <Navigate to={last ? placeIn(last.id) : "/new"} replace />;
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
            element: <DecisionPage />,
          },
          { path: "/settings", element: <Settings /> },
          { path: "/library", element: <Library /> },
          { path: "/directory", element: <Directory /> },
          { path: "/rules", element: <Rules /> },
          { path: "/company", element: <Details /> },
        ],
      },
    ],
  },
];
