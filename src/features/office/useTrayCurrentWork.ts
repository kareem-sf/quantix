import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { nativeDesktop } from "../../api";
import { tenderRoute } from "../../navigation/routes";

/** The native menu changes the view; it never submits or resumes work. */
export function useTrayCurrentWork(
  tenderId: string | undefined,
  navigate: (target: string) => void,
) {
  const [error, setError] = useState<Error | null>(null);
  useEffect(() => {
    setError(null);
    if (!nativeDesktop()) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void listen("quantix://tray/current-work", () => {
      if (!disposed && tenderId) navigate(tenderRoute(tenderId, "work"));
    })
      .then((remove) => {
        if (disposed) remove();
        else unsubscribe = remove;
      })
      .catch(() => {
        if (!disposed)
          setError(
            new Error(
              "The Current work tray shortcut is unavailable. Open Work from the workspace navigation.",
            ),
          );
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [navigate, tenderId]);
  return error;
}
