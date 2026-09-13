import {
  Archive,
  BookCheck,
  Bot,
  Info,
  KeyRound,
  Library,
  Mail,
  RotateCcw,
  SlidersHorizontal,
  Stethoscope,
  Workflow,
  type LucideIcon,
} from "lucide-react";

export type SettingsSectionId =
  | "accounts"
  | "preferences"
  | "manager"
  | "agents"
  | "code"
  | "knowledge"
  | "library"
  | "mail"
  | "backups"
  | "diagnostics"
  | "reset"
  | "about";

export type SettingsGroupId = "office" | "knowledge" | "connections" | "system";

export type SettingsSectionDefinition = {
  id: SettingsSectionId;
  label: string;
  description: string;
  icon: LucideIcon;
  group: SettingsGroupId;
  /** Health capability the running service must report for this section. */
  capability?: string;
  destructive?: boolean;
};

export const settingsGroups: { id: SettingsGroupId; label: string }[] = [
  { id: "office", label: "Office" },
  { id: "knowledge", label: "Knowledge" },
  { id: "connections", label: "Connections" },
  { id: "system", label: "Data and support" },
];

export const settingsSections: SettingsSectionDefinition[] = [
  {
    id: "agents",
    label: "Agent library",
    description: "Reusable professional profiles and instructions",
    icon: Bot,
    group: "office",
    capability: "agent_library",
  },
  {
    id: "code",
    label: "Calculations and code",
    description: "Isolated local execution and runtime setup",
    icon: Workflow,
    group: "office",
    capability: "code_runtime_setup",
  },
  {
    id: "accounts",
    label: "AI accounts",
    description: "API keys and eligible subscriptions",
    icon: KeyRound,
    group: "office",
  },
  {
    id: "manager",
    label: "Tender Manager",
    description: "Personality, tone and working style",
    icon: Bot,
    group: "office",
    capability: "manager_profile",
  },
  {
    id: "preferences",
    label: "Working preferences",
    description: "Default currency and office instructions",
    icon: SlidersHorizontal,
    group: "office",
  },
  {
    id: "knowledge",
    label: "Approved knowledge",
    description: "Reusable notes you've approved",
    icon: BookCheck,
    group: "knowledge",
    capability: "knowledge",
  },
  {
    id: "library",
    label: "Company library",
    description: "Project sheets, CVs and certificates",
    icon: Library,
    group: "knowledge",
    capability: "company_library",
  },
  {
    id: "mail",
    label: "Mail",
    description: "Supplier mail account",
    icon: Mail,
    group: "connections",
    capability: "quotations",
  },
  {
    id: "backups",
    label: "Backups",
    description: "Local backups and recovery",
    icon: Archive,
    group: "system",
    capability: "backups",
  },
  {
    id: "diagnostics",
    label: "Diagnostics",
    description: "Technical details and local files",
    icon: Stethoscope,
    group: "system",
  },
  {
    id: "about",
    label: "About",
    description: "Version and credits",
    icon: Info,
    group: "system",
  },
  {
    id: "reset",
    label: "Reset Quantix",
    description: "Remove all Quantix data",
    icon: RotateCcw,
    group: "system",
    destructive: true,
  },
];

export function availableSettingsSections(capabilities: string[]) {
  return settingsSections.filter(
    (section) =>
      !section.capability || capabilities.includes(section.capability),
  );
}

export function isSettingsSection(
  value: string | null | undefined,
): value is SettingsSectionId {
  return settingsSections.some((section) => section.id === value);
}

/** Route to a settings section, remembering the tender route that opened it. */
export function settingsPath(section: SettingsSectionId, origin?: string) {
  const params = new URLSearchParams({ section });
  if (origin) params.set("return", origin);
  return `/settings?${params.toString()}`;
}
