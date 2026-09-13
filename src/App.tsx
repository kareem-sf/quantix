import { useState } from "react";
import { HashRouter } from "react-router-dom";
import { ApiContext } from "./api";
import { useServiceConnection } from "./useServiceConnection";
import { ErrorNotice } from "./components/common";
import { ResetGate } from "./features/FactoryReset";
import { ThemeProvider } from "./theme";
import { AppShell } from "./app/AppShell";
import { BootSplash } from "./app/splash/BootSplash";
import {
  createSplashHolds,
  SplashHoldContext,
  useSplashHoldCount,
} from "./app/splash/splash-hold";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";

// Tests assert on the connected app directly; the splash has its own tests.
const showSplash = import.meta.env.MODE !== "test";

export default function App() {
  return (
    <ThemeProvider>
      <TooltipProvider>
        <ConnectedApp />
      </TooltipProvider>
    </ThemeProvider>
  );
}

/**
 * The service connection lives in component state rather than the query cache:
 * an accepted factory reset clears every cached query, and recovery must stay
 * mounted while that happens.
 */
function ConnectedApp() {
  const { api, error, retry } = useServiceConnection();
  const [holds] = useState(() => createSplashHolds(!showSplash));
  const [splash, setSplash] = useState(showSplash);

  return (
    <SplashHoldContext.Provider value={holds}>
      {api ? (
        <ApiContext.Provider value={api}>
          <ResetGate>
            <HashRouter>
              <AppShell />
            </HashRouter>
          </ResetGate>
        </ApiContext.Provider>
      ) : !splash || error ? (
        <ConnectionProblem error={error} onRetry={retry} />
      ) : null}
      {splash ? (
        <SplashHost
          connected={!!api}
          failed={!!error}
          onLift={holds.lift}
          onDone={() => {
            holds.lift();
            setSplash(false);
          }}
          holds={holds}
        />
      ) : null}
    </SplashHoldContext.Provider>
  );
}

function SplashHost({
  connected,
  failed,
  holds,
  ...props
}: {
  connected: boolean;
  failed: boolean;
  holds: ReturnType<typeof createSplashHolds>;
  onLift: () => void;
  onDone: () => void;
}) {
  const pending = useSplashHoldCount(holds);
  // A startup failure lifts the splash onto the connection problem screen.
  return (
    <BootSplash ready={(connected && pending === 0) || failed} {...props} />
  );
}

/** Shown if the service drops after the splash has already lifted, and in tests. */
function ConnectionProblem({
  error,
  onRetry,
}: {
  error: unknown;
  onRetry: () => void;
}) {
  return (
    <main className="flex min-h-svh flex-col justify-center gap-4 bg-background px-[max(2rem,10vw)] py-12 text-foreground">
      {error ? (
        <div className="flex max-w-xl flex-col gap-4">
          <h1 className="text-lg font-semibold tracking-tight">
            The local service is unavailable
          </h1>
          <ErrorNotice error={error} />
          <p role="status" className="text-sm text-muted-foreground">
            Quantix will reconnect automatically when the local service is
            ready.
          </p>
          <div>
            <Button onClick={onRetry}>Try again</Button>
          </div>
        </div>
      ) : (
        <p role="status" className="text-sm text-muted-foreground">
          Connecting to your Tender Office…
        </p>
      )}
    </main>
  );
}
