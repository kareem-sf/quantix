/** Shown while a screen's data arrives, so the window is never blank. */
export function Opening({ error }: { error?: boolean }) {
  return (
    <p className="m-auto text-ink-3">
      {error ? "Waiting for the Quantix service… it starts with the app." : "Opening…"}
    </p>
  );
}
