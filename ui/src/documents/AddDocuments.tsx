import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useRef } from "react";
import { useAddDocuments } from "./queries";

/** "Choose folder" and "Add files" buttons. The folder keeps its structure inside the package. In the desktop app they
 * open the Windows pickers and Quantix reads the files where they are; in a browser they upload them. */
export function AddDocuments({ tenderId, primary = false }: { tenderId: string; primary?: boolean }) {
  const add = useAddDocuments(tenderId);
  const folder = useRef<HTMLInputElement>(null);
  const files = useRef<HTMLInputElement>(null);
  const send = (list: FileList | null) => list && list.length > 0 && add.mutate(Array.from(list));
  const desktop = isTauri();

  async function pick(what: "folder" | "files") {
    if (!desktop) return (what === "folder" ? folder : files).current?.click();
    const chosen = await open(
      what === "folder" ? { directory: true, title: "Choose the tender folder" } : { multiple: true, title: "Add files" },
    );
    if (!chosen) return;
    add.mutate(what === "folder" ? { folder: chosen as string } : { files: [chosen].flat() as string[] });
  }

  const main = primary
    ? "h-9 rounded-lg bg-ink px-4 text-[13px] text-white disabled:bg-line-strong"
    : "h-8 rounded-lg border border-line-strong bg-white px-3 text-[13px] disabled:text-ink-4";

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <button type="button" disabled={add.isPending} onClick={() => void pick("folder")} className={main}>
          {add.isPending ? "Adding files…" : "Choose folder"}
        </button>
        <button
          type="button"
          disabled={add.isPending}
          onClick={() => void pick("files")}
          className="h-8 px-2 text-[13px] text-ink-2 hover:text-ink disabled:text-ink-4"
        >
          Add files
        </button>
      </div>
      <input
        ref={folder}
        type="file"
        multiple
        hidden
        aria-label="Tender folder"
        onChange={(e) => send(e.target.files)}
        {...{ webkitdirectory: "" }}
      />
      <input ref={files} type="file" multiple hidden aria-label="Tender files" onChange={(e) => send(e.target.files)} />
      {add.isError && <p className="text-attention">{add.error.message}</p>}
      {add.data && (
        <p className="text-ink-2">
          {add.data.added} {add.data.added === 1 ? "file" : "files"} added
          {add.data.unchanged > 0 && `, ${add.data.unchanged} already here`}.
        </p>
      )}
    </div>
  );
}
