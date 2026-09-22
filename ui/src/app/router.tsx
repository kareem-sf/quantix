import { Navigate, Outlet, type RouteObject } from "react-router";
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

function Home() {
  const tenders = useTenders();
  if (!tenders.data) return null;
  if (tenders.data.length > 0) return <Navigate to={`/tenders/${tenders.data[0].id}`} replace />;
  return <Navigate to="/new" replace />;
}

export const routes: RouteObject[] = [
  {
    element: <Shell />,
    children: [
      { path: "/", element: <Home /> },
      { path: "/new", element: <NewTender /> },
      { path: "/tenders/:tenderId", element: <Overview /> },
      { path: "/settings", element: <Settings /> },
    ],
  },
];
