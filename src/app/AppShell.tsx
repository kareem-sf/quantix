import { useCallback, useEffect, useState } from "react";
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";
import { FolderPlus, Plus } from "lucide-react";
import { useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { ImportPackage, NewTender } from "../features/TenderDialogs";
import { useTrayCurrentWork } from "../features/office/useTrayCurrentWork";
import { parseRouteContext, tenderRoute } from "../navigation/routes";
import { Button } from "@/components/ui/button";
import { EffectFilters } from "@/components/ui/effect-filters";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { AppSidebar } from "./AppSidebar";
import { DevTools } from "./DevTools";
import { SettingsRoute } from "./SettingsRoute";
import { TenderWorkspace } from "./TenderWorkspace";
import { useSplashHold } from "./splash/splash-hold";

const LAST_TENDER_KEY = "quantix.last-tender.v1";
const showDevTools = import.meta.env.DEV && import.meta.env.MODE !== "test";

function readLastTender() {
  try {
    return window.localStorage.getItem(LAST_TENDER_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

function rememberTender(id: string) {
  try {
    window.localStorage.setItem(LAST_TENDER_KEY, id);
  } catch {
    // Storage can be unavailable in a locked-down WebView; the route still works.
  }
}

export function AppShell() {
  const health = useResource<Schema<"Health">>("/health");
  const settings = useResource<Schema<"Settings">>("/settings");
  // While a tender still carries its package name, Quantix is identifying the
  // project; keep the list fresh so the real name appears when it is saved.
  const [naming, setNaming] = useState(false);
  const tenders = useResource<Schema<"Tender">[]>("/tenders", naming);
  useEffect(() => {
    setNaming(
      !!tenders.data?.some((tender) => tender.name_source === "pending"),
    );
  }, [tenders.data]);
  useSplashHold(tenders.isPending);
  const location = useLocation();
  const navigate = useNavigate();
  const context = location.pathname.startsWith("/tenders/")
    ? parseRouteContext(`${location.pathname}${location.search}`)
    : null;
  const tenderId = context?.kind === "tender" ? context.tenderId : undefined;
  const [newOpen, setNewOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const go = useCallback((target: string) => void navigate(target), [navigate]);
  const trayError = useTrayCurrentWork(tenderId, go);
  const closeNew = useCallback(() => setNewOpen(false), []);
  const closeImport = useCallback(() => setImportOpen(false), []);

  useEffect(() => {
    if (tenderId) rememberTender(tenderId);
  }, [tenderId]);

  return (
    <SidebarProvider className="h-svh overflow-hidden">
      <EffectFilters />
      <AppSidebar
        tenders={tenders.data}
        tendersPending={tenders.isPending}
        tendersFailed={!!tenders.error && !tenders.data}
        capabilities={health.data?.capabilities ?? []}
        onNavigate={go}
        onNewTender={() => setNewOpen(true)}
      />
      <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
        <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2 md:hidden">
          <SidebarTrigger aria-label="Open navigation" />
          <span className="text-sm font-medium">Navigation</span>
        </div>
        {trayError ? (
          <div className="px-4 pt-3">
            <ErrorNotice error={trayError} />
          </div>
        ) : null}
        <Routes>
          <Route
            path="/tenders/*"
            element={
              <TenderWorkspace
                health={health.data}
                settings={settings.data}
                onImport={() => setImportOpen(true)}
              />
            }
          />
          <Route path="/settings" element={<SettingsRoute />} />
          <Route
            path="*"
            element={
              <Home
                tenders={tenders.data}
                pending={tenders.isPending}
                error={tenders.error}
                onRetry={() => void tenders.refetch()}
                onNewTender={() => setNewOpen(true)}
              />
            }
          />
        </Routes>
      </SidebarInset>
      {newOpen ? (
        <NewTender
          onClose={closeNew}
          onCreated={(tender) => {
            closeNew();
            void navigate(tenderRoute(tender.id, "manager"));
          }}
        />
      ) : null}
      {importOpen && tenderId ? (
        <ImportPackage tenderId={tenderId} onClose={closeImport} />
      ) : null}
      {showDevTools ? <DevTools /> : null}
    </SidebarProvider>
  );
}

function Home({
  tenders,
  pending,
  error,
  onRetry,
  onNewTender,
}: {
  tenders?: Schema<"Tender">[];
  pending: boolean;
  error: unknown;
  onRetry: () => void;
  onNewTender: () => void;
}) {
  if (pending) return <Loading>Loading your tenders…</Loading>;
  if (!tenders)
    return (
      <div className="flex flex-col items-start gap-3 p-6">
        <ErrorNotice error={error} />
        <Button variant="outline" onClick={onRetry}>
          Try again
        </Button>
      </div>
    );
  const remembered = readLastTender();
  const target =
    tenders.find((tender) => tender.id === remembered) ?? tenders[0];
  if (target)
    return <Navigate replace to={tenderRoute(target.id, "manager")} />;
  return (
    <Empty className="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <FolderPlus />
        </EmptyMedia>
        <EmptyTitle>
          <h1 className="text-lg font-semibold tracking-tight">
            Create your first tender
          </h1>
        </EmptyTitle>
        <EmptyDescription>
          Add the tender documents and work with the Tender Manager to prepare
          your submission.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button onClick={onNewTender}>
          <Plus data-icon="inline-start" />
          New tender
        </Button>
      </EmptyContent>
    </Empty>
  );
}
