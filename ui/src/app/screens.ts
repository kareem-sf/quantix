import {
  IconAddressBook,
  IconBook2,
  IconBuilding,
  IconFileText,
  IconHelpCircle,
  IconLayoutDashboard,
  IconListCheck,
  IconPackage,
  IconRuler2,
  IconSend,
  IconTable,
  type Icon,
} from "@tabler/icons-react";

/** A tender's screens in the order a tender is worked: read, take off, price, get quotes, query, submit. Ctrl+1
 * to Ctrl+7 open them. */
export const TENDER_SCREENS: [label: string, path: string, icon: Icon][] = [
  ["Overview", "", IconLayoutDashboard],
  ["Documents", "/documents", IconFileText],
  ["Takeoff", "/takeoff", IconRuler2],
  ["Estimate", "/estimate", IconTable],
  ["Subcontract", "/subcontract", IconPackage],
  ["Queries", "/queries", IconHelpCircle],
  ["Submission", "/submission", IconSend],
];

/** What the firm keeps across tenders. */
export const COMPANY_SCREENS: [label: string, path: string, icon: Icon][] = [
  ["Company library", "/library", IconBook2],
  ["Directory", "/directory", IconAddressBook],
  ["Company rules", "/rules", IconListCheck],
  ["Company details", "/company", IconBuilding],
];

/** The screen a path shows, for the title bar. */
export function screenOf(pathname: string): string {
  const inTender = /^\/tenders\/[^/]+(\/[^/]+)?/.exec(pathname);
  if (inTender) {
    if (inTender[1] === "/decisions") return "Decision";
    return TENDER_SCREENS.find(([, path]) => path === (inTender[1] ?? ""))?.[0] ?? "";
  }
  if (pathname === "/desk") return "Desk";
  if (pathname === "/tenders") return "Tenders";
  if (pathname === "/settings") return "Settings";
  if (pathname === "/about") return "About Quantix";
  if (pathname === "/new") return "New tender";
  return COMPANY_SCREENS.find(([, path]) => pathname.startsWith(path))?.[0] ?? "";
}
