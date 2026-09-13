import { useRef } from "react";
import {
  ArrowLeft,
  ChevronsUpDown,
  Folder,
  FolderOpen,
  LayoutGrid,
  Monitor,
  Moon,
  PanelLeft,
  PenLine,
  Settings2,
  Sun,
} from "lucide-react";
import { useLocation } from "react-router-dom";
import { tenderPath, useResource, type Schema } from "../api";
import {
  parseRouteContext,
  safeLocalOrigin,
  stripSourceQuery,
  tenderRoute,
  type WorkspaceSection,
} from "../navigation/routes";
import { type Theme } from "../theme";
import { AnimatedNumber } from "@/components/ui/animated-number";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  SidebarRail,
  useSidebar,
} from "@/components/ui/sidebar";
import { originOf, useThemeTransition } from "@/components/ui/theme-toggle";
import { WorkingDots } from "@/components/ui/working-dots";
import { BrandMark } from "./BrandMark";
import { tenderDisplayName } from "./tender-name";
import { sectionLabels, tenderSections } from "./sections";
import {
  availableSettingsSections,
  isSettingsSection,
  settingsGroups,
  settingsPath,
} from "./settings-sections";

type AppSidebarProps = {
  tenders?: Schema<"Tender">[];
  tendersPending: boolean;
  tendersFailed: boolean;
  capabilities: string[];
  onNavigate: (target: string) => void;
  onNewTender: () => void;
};

export function AppSidebar({
  tenders,
  tendersPending,
  tendersFailed,
  capabilities,
  onNavigate,
  onNewTender,
}: AppSidebarProps) {
  const location = useLocation();
  const { state, toggleSidebar, isMobile, setOpenMobile } = useSidebar();
  function selectAction(action: () => void) {
    if (isMobile) setOpenMobile(false);
    action();
  }
  const navigate = (target: string) => selectAction(() => onNavigate(target));
  const expanded = state === "expanded";
  const current = `${location.pathname}${location.search}`;
  const settingsOpen = location.pathname === "/settings";
  const context = location.pathname.startsWith("/tenders/")
    ? parseRouteContext(current)
    : null;
  const activeTender = context?.kind === "tender" ? context : null;
  const origin = activeTender ? stripSourceQuery(current) : undefined;

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              tooltip="Expand the menu"
              aria-label={expanded ? "Collapse the menu" : "Expand the menu"}
              aria-expanded={expanded}
              title={expanded ? "Collapse the menu" : "Expand the menu"}
              onClick={toggleSidebar}
            >
              <BrandMark size={32} data-brand-anchor="" />
              <span className="grid flex-1 text-start leading-tight">
                <span className="truncate font-semibold tracking-tight">
                  Quantix
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  Tender Office
                </span>
              </span>
              <PanelLeft className="ms-auto text-muted-foreground" />
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
        {settingsOpen ? null : (
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                tooltip="New tender"
                onClick={() => selectAction(onNewTender)}
              >
                <PenLine />
                <span>New tender</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        )}
      </SidebarHeader>

      <SidebarContent className="scroll-fade-y">
        {settingsOpen ? (
          <SettingsPages
            capabilities={capabilities}
            search={location.search}
            onNavigate={navigate}
          />
        ) : (
          <SidebarGroup>
            <SidebarGroupLabel>Tenders</SidebarGroupLabel>
            <SidebarMenu>
              {tendersPending ? (
                <SidebarNote>Loading tenders…</SidebarNote>
              ) : tendersFailed ? (
                <SidebarNote>Tenders couldn't be loaded.</SidebarNote>
              ) : tenders?.length === 0 ? (
                <SidebarNote>No tenders yet</SidebarNote>
              ) : null}
              {tenders?.map((tender) => {
                const open = activeTender?.tenderId === tender.id;
                return (
                  <SidebarMenuItem key={tender.id}>
                    <SidebarMenuButton
                      tooltip={tenderDisplayName(tender)}
                      aria-expanded={open}
                      onClick={() =>
                        navigate(tenderRoute(tender.id, "manager"))
                      }
                    >
                      {open ? <FolderOpen /> : <Folder />}
                      <span>{tenderDisplayName(tender)}</span>
                    </SidebarMenuButton>
                    {activeTender && open ? (
                      <TenderPages
                        tenderId={tender.id}
                        section={activeTender.section}
                        onNavigate={navigate}
                      />
                    ) : null}
                  </SidebarMenuItem>
                );
              })}
            </SidebarMenu>
          </SidebarGroup>
        )}
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SettingsMenu
              active={settingsOpen}
              origin={origin}
              onNavigate={navigate}
            />
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}

/** While Settings is open, the sidebar lists its sections in place of tenders. */
function SettingsPages({
  capabilities,
  search,
  onNavigate,
}: {
  capabilities: string[];
  search: string;
  onNavigate: (target: string) => void;
}) {
  const params = new URLSearchParams(search);
  const origin = safeLocalOrigin(
    params.get("return") ?? params.get("origin") ?? undefined,
  );
  const sections = availableSettingsSections(capabilities);
  const requested = params.get("section");
  const active =
    isSettingsSection(requested) &&
    sections.some((section) => section.id === requested)
      ? requested
      : "accounts";
  return (
    <>
      <SidebarGroup>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              tooltip={origin ? "Back to the tender" : "Back to tenders"}
              onClick={() => onNavigate(origin ?? "/")}
            >
              <ArrowLeft className="rtl:rotate-180" />
              <span>{origin ? "Back to the tender" : "Back to tenders"}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarGroup>
      <nav aria-label="Settings sections" className="contents">
        {settingsGroups.map((group) => {
          const items = sections.filter(
            (section) => section.group === group.id,
          );
          if (!items.length) return null;
          return (
            <SidebarGroup key={group.id}>
              <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
              <SidebarMenu>
                {items.map((section) => (
                  <SidebarMenuItem key={section.id}>
                    <SidebarMenuButton
                      tooltip={section.label}
                      isActive={active === section.id}
                      aria-current={active === section.id ? "page" : undefined}
                      className={
                        section.destructive
                          ? "text-destructive hover:text-destructive data-active:text-destructive"
                          : undefined
                      }
                      onClick={() =>
                        onNavigate(settingsPath(section.id, origin))
                      }
                    >
                      <section.icon />
                      <span>{section.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroup>
          );
        })}
      </nav>
    </>
  );
}

function SidebarNote({ children }: { children: string }) {
  return (
    <li className="px-2 py-1 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
      {children}
    </li>
  );
}

function TenderPages({
  tenderId,
  section,
  onNavigate,
}: {
  tenderId: string;
  section: WorkspaceSection;
  onNavigate: (target: string) => void;
}) {
  const overview = useResource<Schema<"Overview">>(tenderPath(tenderId), true);
  const findingsToReview =
    overview.data?.findings.filter((finding) => finding.state === "proposed")
      .length ?? 0;
  const toReview =
    findingsToReview + (overview.data?.plan?.status === "proposed" ? 1 : 0);
  const working = (overview.data?.active_runs.length ?? 0) > 0;
  return (
    <SidebarMenuSub>
      {tenderSections.map((item) => (
        <SidebarMenuSubItem key={item}>
          <SidebarMenuSubButton
            render={<button type="button" />}
            className="w-full"
            isActive={section === item}
            aria-current={section === item ? "page" : undefined}
            onClick={() => onNavigate(tenderRoute(tenderId, item))}
          >
            <span>{sectionLabels[item]}</span>
          </SidebarMenuSubButton>
          {item === "manager" && working ? (
            <span className="pointer-events-none absolute end-2 top-1/2 flex -translate-y-1/2 items-center gap-1 text-xs text-emerald-600 dark:text-emerald-400">
              <WorkingDots className="scale-[0.6]" />
              Working
            </span>
          ) : null}
          {item === "work" && toReview > 0 ? (
            <span className="pointer-events-none absolute end-2 top-1/2 flex -translate-y-1/2 items-center gap-1 text-xs text-amber-600 dark:text-amber-400">
              <AnimatedNumber value={toReview} /> to review
            </span>
          ) : null}
        </SidebarMenuSubItem>
      ))}
    </SidebarMenuSub>
  );
}

const themeIcons: Record<Theme, typeof Sun> = {
  light: Sun,
  dark: Moon,
  system: Monitor,
};

/** Compact: open Settings (sections are listed in the sidebar there) and switch theme. */
function SettingsMenu({
  active,
  origin,
  onNavigate,
}: {
  active: boolean;
  origin?: string;
  onNavigate: (target: string) => void;
}) {
  const { theme, transitionTo } = useThemeTransition();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const ThemeIcon = themeIcons[theme];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <SidebarMenuButton
            ref={triggerRef}
            tooltip="Settings"
            isActive={active}
          />
        }
      >
        <Settings2 />
        <span>Settings</span>
        <ChevronsUpDown className="ms-auto" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="start"
        className="w-(--anchor-width) min-w-44"
      >
        <DropdownMenuItem
          onClick={() => onNavigate(settingsPath("accounts", origin))}
        >
          <LayoutGrid />
          All settings
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <ThemeIcon />
            Theme
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(value) =>
                transitionTo(
                  value as Theme,
                  triggerRef.current ? originOf(triggerRef.current) : undefined,
                )
              }
            >
              <DropdownMenuRadioItem value="light">
                <Sun />
                Light
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="dark">
                <Moon />
                Dark
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="system">
                <Monitor />
                System
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
