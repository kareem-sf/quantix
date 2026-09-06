import { Activity, useCallback, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Menu, Plus, Settings as SettingsIcon, Upload, X } from "lucide-react";
import {
  ApiContext,
  connect,
  tenderPath,
  useResource,
  type Schema,
} from "./api";
import { Empty, ErrorNotice, Loading } from "./components/ui";
import { Files } from "./features/Files";
import { Manager } from "./features/Manager";
import { Settings } from "./features/Settings";
import { SourceDrawer, type SourceSelection } from "./features/Sources";
import { ImportPackage, NewTender } from "./features/TenderDialogs";
import { Work } from "./features/Work";
import { Estimate } from "./features/Estimate";
import { ProjectMap } from "./features/ProjectMap";

type Tab = "Manager" | "Project map" | "Files" | "Work" | "Estimate";
export default function App() {
  const connection = useQuery({
    queryKey: ["connection"],
    queryFn: connect,
    retry: 1,
    staleTime: Infinity,
  });
  if (connection.isPending)
    return (
      <div className="connection-screen">
        <div className="wordmark">Quantix</div>
        <Loading>Connecting to your Tender Office…</Loading>
      </div>
    );
  if (!connection.data)
    return (
      <div className="connection-screen">
        <div className="wordmark">Quantix</div>
        <h1>The local service is unavailable</h1>
        <ErrorNotice error={connection.error} />
        <button
          className="button primary"
          onClick={() => void connection.refetch()}
        >
          Try again
        </button>
      </div>
    );
  return (
    <ApiContext.Provider value={connection.data}>
      <Office />
    </ApiContext.Provider>
  );
}
function Office() {
  const tenders = useResource<Schema<"Tender">[]>("/tenders");
  const settings = useResource<Schema<"Settings">>("/settings");
  const health = useResource<Schema<"Health">>("/health");
  const [selectedId, setSelectedId] = useState<string | null>(null),
    [settingsOpen, setSettingsOpen] = useState(false),
    [newOpen, setNewOpen] = useState(false),
    [navOpen, setNavOpen] = useState(false);
  const tenderId = selectedId ?? tenders.data?.[0]?.id;
  const closeNew = useCallback(() => setNewOpen(false), []);
  return (
    <div className="app-shell">
      <button
        className="mobile-menu icon-button"
        onClick={() => setNavOpen((value) => !value)}
        aria-label="Toggle tender navigation"
      >
        <Menu size={22} />
      </button>
      <aside className={`sidebar ${navOpen ? "sidebar-open" : ""}`}>
        <div className="sidebar-brand">
          <div className="wordmark">Quantix</div>
          <button
            className="mobile-close icon-button"
            aria-label="Close tender navigation"
            onClick={() => setNavOpen(false)}
          >
            <X size={20} />
          </button>
        </div>
        <button className="new-tender-button" onClick={() => setNewOpen(true)}>
          <Plus size={23} />
          New tender
        </button>
        <div className="sidebar-label">Tenders</div>
        <nav className="tender-nav" aria-label="Tenders">
          {tenders.data?.map((tender) => (
            <button
              key={tender.id}
              className={
                tender.id === tenderId && !settingsOpen ? "selected" : ""
              }
              aria-current={
                tender.id === tenderId && !settingsOpen ? "page" : undefined
              }
              onClick={() => {
                setSelectedId(tender.id);
                setSettingsOpen(false);
                setNavOpen(false);
              }}
              title={tender.name}
            >
              {tender.name}
            </button>
          ))}
          {tenders.isPending ? (
            <p className="sidebar-empty">Loading tenders…</p>
          ) : tenders.data?.length === 0 ? (
            <p className="sidebar-empty">No tenders yet</p>
          ) : null}
        </nav>
        <div className="sidebar-bottom">
          <button
            className={`settings-button ${settingsOpen ? "selected" : ""}`}
            aria-current={settingsOpen ? "page" : undefined}
            onClick={() => {
              setSettingsOpen(true);
              setNavOpen(false);
            }}
          >
            <SettingsIcon size={26} />
            Settings
          </button>
          <p>Saved on this device</p>
        </div>
      </aside>
      <main className="main-shell">
        <ErrorNotice error={tenders.error || health.error} />
        {tenderId ? (
          <Activity mode={settingsOpen ? "hidden" : "visible"}>
            <TenderWorkspace
              key={tenderId}
              tenderId={tenderId}
              settings={settings.data}
              capabilities={health.data?.capabilities ?? []}
              onSettings={() => setSettingsOpen(true)}
            />
          </Activity>
        ) : null}
        {settingsOpen ? (
          <Settings />
        ) : tenderId ? null : tenders.isPending ? (
          <Loading>Loading the Tender Office…</Loading>
        ) : (
          <div className="office-empty">
            <Empty
              title="Create your first tender"
              action={
                <button
                  className="button primary"
                  onClick={() => setNewOpen(true)}
                >
                  <Plus size={18} />
                  New tender
                </button>
              }
            >
              Add the tender documents and work with the manager to prepare your
              submission.
            </Empty>
          </div>
        )}
      </main>
      {newOpen ? (
        <NewTender
          onClose={closeNew}
          onCreated={(tender) => {
            setSelectedId(tender.id);
            setSettingsOpen(false);
            closeNew();
            setNavOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}
function TenderWorkspace({
  tenderId,
  settings,
  capabilities,
  onSettings,
}: {
  tenderId: string;
  settings?: Schema<"Settings">;
  capabilities: string[];
  onSettings: () => void;
}) {
  const overview = useResource<Schema<"Overview">>(tenderPath(tenderId), true);
  const artifacts = useResource<Schema<"Artifact">[]>(
    `${tenderPath(tenderId)}/artifacts`,
    (overview.data?.active_runs.length ?? 0) > 0,
  );
  const documentState = JSON.stringify([
    overview.data?.coverage,
    overview.data?.tender.revision,
    overview.data?.active_runs.length,
  ]);
  useEffect(() => {
    void artifacts.refetch();
  }, [documentState, artifacts.refetch]);
  const [tab, setTab] = useState<Tab>("Manager"),
    [importOpen, setImportOpen] = useState(false),
    [source, setSource] = useState<SourceSelection | null>(null);
  const closeImport = useCallback(() => setImportOpen(false), []),
    closeSource = useCallback(() => setSource(null), []);
  const openImport = () => setImportOpen(true);
  if (overview.isPending) return <Loading>Loading tender…</Loading>;
  if (!overview.data) return <ErrorNotice error={overview.error} />;
  return (
    <>
      <header className="tender-header">
        <div className="project-heading">
          <div>
            <h1>{overview.data.tender.name}</h1>
            <p>Tender preparation</p>
          </div>
          <button className="button" onClick={openImport}>
            <Upload size={21} />
            Add files
          </button>
        </div>
        <nav className="tabs" aria-label="Tender sections">
          {(
            [
              "Manager",
              ...(capabilities.includes("project_map") ? ["Project map"] : []),
              "Files",
              "Work",
              ...(capabilities.includes("estimates") ? ["Estimate"] : []),
            ] as Tab[]
          ).map((item) => (
            <button
              key={item}
              className={tab === item ? "active" : ""}
              aria-current={tab === item ? "page" : undefined}
              onClick={() => setTab(item)}
            >
              {item}
            </button>
          ))}
        </nav>
      </header>
      <div className="workspace-body">
        <ErrorNotice error={overview.error || artifacts.error} />
        <Activity mode={tab === "Manager" ? "visible" : "hidden"}>
          <Manager
            overview={overview.data}
            artifacts={artifacts.data ?? []}
            settings={settings}
            onImport={openImport}
            onSettings={onSettings}
            onSource={setSource}
          />
        </Activity>
        {tab === "Project map" && capabilities.includes("project_map") ? (
          <ProjectMap
            tenderId={tenderId}
            artifacts={artifacts.data ?? []}
            onSource={setSource}
          />
        ) : tab === "Files" ? (
          artifacts.isPending ? (
            <Loading>Loading document register…</Loading>
          ) : (
            <Files
              tenderId={tenderId}
              artifacts={artifacts.data ?? []}
              onImport={openImport}
              onSource={setSource}
              meaningAvailable={capabilities.includes("meaning_search")}
              activeRuns={overview.data.active_runs}
              onWork={() => setTab("Work")}
            />
          )
        ) : tab === "Work" ? (
          <Work
            tenderId={tenderId}
            onChanges={() => {
              setTab("Manager");
              requestAnimationFrame(() =>
                document
                  .querySelector<HTMLTextAreaElement>(
                    '[aria-label="Message to Tender Manager"]',
                  )
                  ?.focus(),
              );
            }}
            onSource={setSource}
          />
        ) : tab === "Estimate" && capabilities.includes("estimates") ? (
          <Estimate
            tenderId={tenderId}
            defaultCurrency={settings?.default_currency ?? ""}
            outputsAvailable={capabilities.includes("outputs")}
            onSource={setSource}
          />
        ) : null}
      </div>
      {importOpen ? (
        <ImportPackage tenderId={tenderId} onClose={closeImport} />
      ) : null}
      {source ? (
        <SourceDrawer
          key={"sourceId" in source ? source.sourceId : `${source.artifactId}:${source.page ?? 1}`}
          tenderId={tenderId}
          selection={source}
          artifacts={artifacts.data ?? []}
          onClose={closeSource}
        />
      ) : null}
    </>
  );
}
