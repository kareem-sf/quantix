import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { nativeDesktop } from "../../api";

/**
 * The desktop app opens hidden behind a separate transparent splash window
 * (splash.html, src-tauri/src/splash.rs). Once the first screen has its data,
 * this asks the desktop shell to finish the splash and show the main window.
 * In a browser there is no splash, so it finishes at once.
 */
export function BootSplash({
  ready,
  onLift,
  onDone,
}: {
  ready: boolean;
  onLift?: () => void;
  onDone: () => void;
}) {
  const callbacks = useRef({ onLift, onDone });
  callbacks.current = { onLift, onDone };

  useEffect(() => {
    if (!ready && nativeDesktop()) return;
    let cancelled = false;
    const finished = nativeDesktop()
      ? invoke("finish_splash").catch(() => undefined)
      : Promise.resolve();
    void finished.then(() => {
      if (cancelled) return;
      callbacks.current.onLift?.();
      callbacks.current.onDone();
    });
    return () => {
      cancelled = true;
    };
  }, [ready]);

  return null;
}
