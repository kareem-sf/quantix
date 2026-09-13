import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useState,
  type ReactNode,
} from "react";
import { syncBrandMetadata } from "./brand";

export type Theme = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

type ThemeContextValue = {
  theme: Theme;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);
const STORAGE_KEY = "quantix-theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function initialTheme(): Theme {
  if (typeof window === "undefined") return "light";
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    return saved === "dark" || saved === "system" ? saved : "light";
  } catch {
    return "light";
  }
}

export function systemTheme(): ResolvedTheme {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function")
    return "light";
  return window.matchMedia(DARK_QUERY).matches ? "dark" : "light";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(initialTheme);
  const [system, setSystem] = useState<ResolvedTheme>(systemTheme);
  const resolvedTheme = theme === "system" ? system : theme;

  useEffect(() => {
    if (theme !== "system" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(DARK_QUERY);
    const update = () => setSystem(query.matches ? "dark" : "light");
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, [theme]);

  // Layout effect: a view transition snapshots the page right after a
  // synchronous theme change, so the class must already be on the root.
  useLayoutEffect(() => {
    const root = document.documentElement;
    // shadcn components read the .dark class; legacy feature styles still
    // read data-theme until those views are rebuilt.
    root.classList.toggle("dark", resolvedTheme === "dark");
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme;
    syncBrandMetadata(resolvedTheme);
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Storage can be unavailable in a locked-down WebView. The in-memory
      // theme still applies for the current session.
    }
  }, [theme, resolvedTheme]);

  const setTheme = useCallback((next: Theme) => setThemeState(next), []);
  const toggleTheme = useCallback(
    () => setThemeState(resolvedTheme === "light" ? "dark" : "light"),
    [resolvedTheme],
  );

  return (
    <ThemeContext.Provider
      value={{ theme, resolvedTheme, setTheme, toggleTheme }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("useTheme must be used inside ThemeProvider");
  return context;
}

/** The applied theme, also usable by components rendered without a provider. */
export function useResolvedTheme(): ResolvedTheme {
  const context = useContext(ThemeContext);
  if (context) return context.resolvedTheme;
  return typeof document !== "undefined" &&
    document.documentElement.classList.contains("dark")
    ? "dark"
    : "light";
}
