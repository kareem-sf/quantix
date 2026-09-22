import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router";
import { useCreateTender } from "./queries";

export function NewTender() {
  const navigate = useNavigate();
  const create = useCreateTender();
  const [name, setName] = useState("");
  const [due, setDue] = useState("");

  function submit(event: FormEvent) {
    event.preventDefault();
    create.mutate(
      { name, due_date: due || null },
      { onSuccess: (tender) => navigate(`/tenders/${tender.id}`) },
    );
  }

  return (
    <form onSubmit={submit} className="flex w-[600px] flex-col gap-7 pt-24">
      <div className="flex flex-col gap-1.5">
        <h1 className="text-[28px] font-semibold tracking-tight">Start a tender</h1>
        <p className="text-sm text-ink-2">Name the tender. You can add the package once it’s created.</p>
      </div>

      <div className="flex flex-col gap-4">
        <label className="flex flex-col gap-1.5">
          <span className="font-medium">Tender name</span>
          <input
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Al Noor Primary School, Block B"
            className="h-10 rounded-lg border border-line-strong px-3 text-sm outline-none focus:border-ink"
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="font-medium">
            Submission date <span className="font-normal text-ink-3">optional</span>
          </span>
          <input
            type="date"
            value={due}
            onChange={(e) => setDue(e.target.value)}
            className="h-10 w-56 rounded-lg border border-line-strong px-3 text-sm outline-none focus:border-ink"
          />
        </label>
      </div>

      {create.isError && <p className="text-attention">Couldn’t create the tender: {create.error.message}</p>}

      <button
        type="submit"
        disabled={create.isPending || !name.trim()}
        className="h-10 self-start rounded-lg bg-ink px-5 text-sm font-medium text-white disabled:bg-line-strong disabled:text-ink-3"
      >
        {create.isPending ? "Creating…" : "Start tender"}
      </button>
    </form>
  );
}
