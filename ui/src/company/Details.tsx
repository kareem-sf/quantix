import { useEffect, useState, type FormEvent } from "react";
import { useProfile, useRemoveLogo, useSaveLogo, useSaveProfile, type Profile } from "./queries";

const FIELDS = [
  ["name", "Company name", "As it appears on your letterhead"],
  ["cr_number", "Commercial registration", "CR number"],
  ["vat_number", "VAT registration", "VAT number"],
] as const;

/** Who the firm is: the title block and letterhead of every document Quantix writes. */
export function Details() {
  const profile = useProfile();
  const save = useSaveProfile();
  const saveLogo = useSaveLogo();
  const removeLogo = useRemoveLogo();
  const [form, setForm] = useState<Omit<Profile, "has_logo"> | null>(null);
  const [logoVersion, setLogoVersion] = useState(0);
  useEffect(() => {
    if (profile.data && !form) setForm(profile.data);
  }, [profile.data, form]);
  if (!form) return null;
  const field = "rounded-lg border border-line-strong px-3 outline-none focus:border-ink";

  function submit(event: FormEvent) {
    event.preventDefault();
    save.mutate(form!);
  }

  return (
    <div className="flex w-full max-w-[860px] flex-col px-8 pt-7">
      <h1 className="text-[24px] font-semibold tracking-tight">Company details</h1>
      <span className="text-ink-2">Your letterhead: every document, workbook and PDF Quantix builds carries it.</span>

      <form onSubmit={submit} className="mt-6 flex max-w-[520px] flex-col gap-3">
        {FIELDS.map(([key, label, placeholder]) => (
          <label key={key} className="flex flex-col gap-1">
            <span className="text-xs font-semibold text-ink-2">{label}</span>
            <input
              value={form[key]}
              placeholder={placeholder}
              onChange={(e) => setForm({ ...form, [key]: e.target.value })}
              className={`${field} h-9`}
              dir="auto"
            />
          </label>
        ))}
        <label className="flex flex-col gap-1">
          <span className="text-xs font-semibold text-ink-2">Address</span>
          <textarea
            rows={3}
            value={form.address}
            placeholder="Street, city, PO box"
            onChange={(e) => setForm({ ...form, address: e.target.value })}
            className={`${field} py-2`}
            dir="auto"
          />
        </label>
        <span className="flex items-center gap-3">
          <button className="h-9 self-start rounded-lg bg-ink px-4 text-white">Save</button>
          {save.isSuccess && <span className="text-ink-3">Saved</span>}
          {save.isError && <span className="text-attention">{save.error.message}</span>}
        </span>
      </form>

      <div className="mt-8 flex max-w-[520px] flex-col gap-2 border-t border-line pt-5">
        <h2 className="font-semibold text-ink-2">Logo</h2>
        {profile.data?.has_logo ? (
          <img
            src={`/api/company/logo?v=${logoVersion}`}
            alt="Company logo"
            className="max-h-20 self-start rounded-md border border-line p-2"
          />
        ) : (
          <span className="text-ink-3">No logo yet. It goes at the top of every document.</span>
        )}
        <span className="flex items-center gap-4">
          <label className="cursor-pointer font-medium underline underline-offset-4">
            {profile.data?.has_logo ? "Replace logo" : "Choose a logo"}
            <input
              type="file"
              accept="image/png,image/jpeg"
              aria-label="Logo file"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) saveLogo.mutate(file, { onSuccess: () => setLogoVersion((v) => v + 1) });
              }}
            />
          </label>
          {profile.data?.has_logo && (
            <button onClick={() => removeLogo.mutate()} className="text-ink-3 hover:text-ink">
              Remove
            </button>
          )}
        </span>
        {saveLogo.isError && <p className="text-attention">{saveLogo.error.message}</p>}
      </div>
    </div>
  );
}
