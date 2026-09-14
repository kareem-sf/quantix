import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import {
  Activity,
  ClipboardCheck,
  Ellipsis,
  Files,
  LayoutGrid,
  ListFilter,
  Maximize2,
  Minimize2,
  PanelRight,
  Users,
  X,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Kbd } from "@/components/ui/kbd";
import { SplitPane } from "@/components/ui/split-pane";
import { useIsMobile } from "@/hooks/use-mobile";
import { cn } from "@/lib/utils";
import type { WorkspaceController, WorkspaceView } from "./workspace-state";

export const workspaceViews: {
  id: WorkspaceView;
  label: string;
  icon: LucideIcon;
  shortcut: string;
}[] = [
  { id: "documents", label: "Documents", icon: Files, shortcut: "D" },
  { id: "team", label: "Team", icon: Users, shortcut: "T" },
  { id: "reviews", label: "Reviews", icon: ClipboardCheck, shortcut: "R" },
  { id: "activity", label: "Activity", icon: Activity, shortcut: "A" },
];

type MenuAction = { label: string; icon: LucideIcon; onSelect: () => void };
export function RightWorkspace({
  workspace,
  title,
  chat,
  context,
  renderView,
  actions = [],
}: {
  workspace: WorkspaceController;
  title: string;
  chat: ReactNode;
  context: ReactNode | ((close: () => void) => ReactNode);
  renderView: (view: WorkspaceView) => ReactNode;
  actions?: MenuAction[];
}) {
  const narrow = useIsMobile();
  const [contextOpen, setContextOpen] = useState(false);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const paneRef = useRef<HTMLElement>(null);
  const paneId = useId();
  const open = workspace.mode !== "hidden";
  const expanded = workspace.mode === "expanded";
  const current = workspaceViews.find((view) => view.id === workspace.view);

  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if (!event.ctrlKey || !event.altKey || event.shiftKey || event.metaKey)
        return;
      if (
        event.target instanceof Element &&
        event.target.closest(
          "input, textarea, select, [contenteditable=true], [role=dialog]",
        )
      )
        return;
      const key = event.key.toLowerCase();
      const view = workspaceViews.find(
        (item) => item.shortcut.toLowerCase() === key,
      );
      if (view) {
        event.preventDefault();
        workspace.open(view.id);
      } else if (key === "w") {
        event.preventDefault();
        workspace.toggle();
        toggleRef.current?.focus();
      } else if (key === "e" && open) {
        event.preventDefault();
        workspace.expand();
      }
    }
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, [workspace.open, workspace.toggle, workspace.expand, open]);

  function openView(view: WorkspaceView) {
    workspace.open(view);
    requestAnimationFrame(() => paneRef.current?.focus());
  }
  function goHome() {
    workspace.home();
    requestAnimationFrame(() => paneRef.current?.focus());
  }
  function closeTab() {
    workspace.close();
    requestAnimationFrame(() => paneRef.current?.focus());
  }

  const workPane = (
    <aside
      id={paneId}
      ref={paneRef}
      tabIndex={-1}
      aria-label="Tender workspace"
      className="quantix-workspace flex min-h-0 min-w-0 flex-1 flex-col bg-background outline-none"
    >
      <Tabs
        value={workspace.view}
        onValueChange={(value) => workspace.open(value as WorkspaceView)}
        className="min-h-0 flex-1 gap-0"
      >
        {workspace.tabs.length > 0 ? (
          <header className="flex h-11 shrink-0 items-center gap-1 border-b px-2">
            {workspace.tabs.length ? (
              <TabsList
                variant="line"
                aria-label="Workspace tabs"
                className="min-w-0 flex-1 justify-start overflow-x-auto overflow-y-hidden p-0"
                style={{ height: 40 }}
              >
                {workspace.tabs.map((view) => {
                  const item = workspaceViews.find(
                    (entry) => entry.id === view,
                  )!;
                  return (
                    <TabsTrigger
                      key={view}
                      value={view}
                      className="h-10 flex-none gap-1.5 px-3 text-xs motion-reduce:transition-none"
                    >
                      <item.icon className="size-3.5" />
                      {item.label}
                    </TabsTrigger>
                  );
                })}
              </TabsList>
            ) : (
              <span className="flex-1" />
            )}
            {workspace.view !== "home" ? (
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Workspace home"
                title="Workspace home"
                onClick={goHome}
              >
                <LayoutGrid />
              </Button>
            ) : null}
            {current ? (
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Close ${current.label} tab`}
                title="Close tab"
                onClick={closeTab}
              >
                <X />
              </Button>
            ) : null}
          </header>
        ) : null}
        {workspace.view === "home" ? (
          <div
            className="quantix-reveal flex min-h-0 flex-1 items-center justify-center overflow-auto p-6"
            aria-label="Workspace launcher"
          >
            <div className="flex w-full max-w-sm flex-col gap-1">
              {workspaceViews.map((view) => (
                <Button
                  key={view.id}
                  variant="ghost"
                  className="h-11 w-full justify-start gap-3 px-3 font-normal"
                  onClick={() => openView(view.id)}
                  aria-keyshortcuts={`Control+Alt+${view.shortcut}`}
                >
                  <view.icon className="text-muted-foreground" />
                  <span>{view.label}</span>
                  <Kbd className="ms-auto hidden text-muted-foreground sm:inline-flex">
                    Ctrl+Alt+{view.shortcut}
                  </Kbd>
                </Button>
              ))}
            </div>
          </div>
        ) : null}
        {workspace.tabs.map((view) => (
          <TabsContent
            key={view}
            value={view}
            keepMounted
            className="min-h-0 flex-1 overflow-auto p-5 focus-visible:outline-none"
          >
            {renderView(view)}
          </TabsContent>
        ))}
      </Tabs>
    </aside>
  );

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background"
      data-workspace-mode={workspace.mode}
    >
      <header className="flex h-11 shrink-0 items-center gap-1 px-3">
        <span
          className="me-auto min-w-0 truncate text-xs text-muted-foreground"
          dir="auto"
        >
          {title}
        </span>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Workspace options"
                title="More options"
              />
            }
          >
            <Ellipsis />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="w-64 rounded-xl p-1.5 motion-reduce:animate-none"
          >
            <DropdownMenuGroup>
              {workspaceViews.map((view) => (
                <DropdownMenuItem
                  key={view.id}
                  onClick={() => openView(view.id)}
                >
                  <view.icon />
                  {view.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
            {actions.length ? (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  {actions.map((action) => (
                    <DropdownMenuItem
                      key={action.label}
                      onClick={action.onSelect}
                    >
                      <action.icon />
                      {action.label}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
              </>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          variant="ghost"
          size="sm"
          className="font-normal text-muted-foreground"
          aria-label="Open reviews"
          onClick={() => openView("reviews")}
        >
          <ClipboardCheck />
          <span className="hidden sm:inline">Reviews</span>
        </Button>
        <Popover open={contextOpen} onOpenChange={setContextOpen}>
          <PopoverTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Tender context"
                title="Tender context"
                className={cn(contextOpen && "bg-muted")}
              />
            }
          >
            <ListFilter />
          </PopoverTrigger>
          <PopoverContent
            align="end"
            sideOffset={10}
            className="w-80 max-w-[calc(100vw-2rem)] gap-0 rounded-2xl p-4 shadow-lg motion-reduce:animate-none"
          >
            <PopoverTitle className="sr-only">Tender context</PopoverTitle>
            {typeof context === "function"
              ? context(() => setContextOpen(false))
              : context}
          </PopoverContent>
        </Popover>
        {open ? (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={expanded ? "Restore split view" : "Expand workspace"}
            title={expanded ? "Restore split view" : "Expand workspace"}
            onClick={workspace.expand}
          >
            {expanded ? <Minimize2 /> : <Maximize2 />}
          </Button>
        ) : null}
        <Button
          ref={toggleRef}
          variant="ghost"
          size="icon-sm"
          className={cn(open && "bg-muted")}
          aria-label={open ? "Hide workspace" : "Show workspace"}
          title={
            open ? "Hide workspace (Ctrl+Alt+W)" : "Show workspace (Ctrl+Alt+W)"
          }
          aria-expanded={open}
          aria-controls={paneId}
          onClick={workspace.toggle}
        >
          <PanelRight />
        </Button>
      </header>
      <SplitPane
        storageKey="quantix.office-split.v2"
        defaultPercent={56}
        minPercent={32}
        maxPercent={74}
        label="Resize conversation and workspace"
        start={chat}
        end={workPane}
        endHidden={!open}
        startHidden={expanded || Boolean(narrow && open)}
      />
    </div>
  );
}
