/** Shown while a screen's data arrives: a few quiet placeholder rows, so the window is never blank and nothing
 * pretends to be empty. */
export function Opening({ error }: { error?: boolean }) {
  if (error) return <p className="m-auto text-ink-3">Waiting for the Quantix service… it starts with the app.</p>;
  return (
    <div role="status" className="flex w-full max-w-[784px] flex-col gap-3 px-8 pt-14" aria-label="Opening…">
      {[40, 70, 100, 90, 60].map((width, n) => (
        <span key={n} className="h-3.5 rounded bg-subtle motion-safe:animate-pulse" style={{ width: `${width}%` }} />
      ))}
    </div>
  );
}
