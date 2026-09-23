import { Navigate, Outlet, useRouteError, type RouteObject } from "react-router";
import { Documents } from "../documents/Documents";
import { Estimate } from "../estimate/Estimate";
import { Takeoff } from "../takeoff/Takeoff";
import { DecisionPage } from "../office/DecisionPage";
import { Office } from "../office/Office";
import { Settings } from "../settings/Settings";
import { NewTender } from "../tenders/NewTender";
import { Overview } from "../tenders/Overview";
import { useTenders } from "../tenders/queries";
import { Rail } from "./Rail";

function Shell() {
  return (
    <div className="flex h-full">
      <Rail />
      <main className="flex min-w-0 grow flex-col items-center overflow-y-auto">
        <Outlet />
      </main>
    </div>
  );
}

function ScreenError() {
  const error = useRouteError();
  console.error(error);
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-ink-2">
      <p className="text-sm">Something went wrong on this screen.</p>
      <a href="/" className="font-medium text-ink underline underline-offset-4">
        Reload Quantix
      </a>
    </div>
  );
}

function Home() {
  const tenders = useTenders();
  if (!tenders.data) return null;
  if (tenders.data.length > 0) return <Navigate to={`/tenders/${tenders.data[0].id}`} replace />;
  return <Navigate to="/new" replace />;
}

export const routes: RouteObject[] = [
  {
    element: <Shell />,
    errorElement: <ScreenError />,
    children: [
      { path: "/", element: <Home /> },
      { path: "/new", element: <NewTender /> },
      { path: "/tenders/:tenderId", element: <Overview /> },
      { path: "/tenders/:tenderId/documents", element: <Documents /> },
      { path: "/tenders/:tenderId/office", element: <Office /> },
      { path: "/tenders/:tenderId/takeoff", element: <Takeoff /> },
      { path: "/tenders/:tenderId/estimate", element: <Estimate /> },
      { path: "/tenders/:tenderId/decisions/:decisionId", element: <DecisionPage /> },
      { path: "/settings", element: <Settings /> },
    ],
  },
];
