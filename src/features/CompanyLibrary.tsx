import { useState } from "react";
import { useApi } from "../api";
import { ErrorNotice } from "../components/common";

export function CompanyLibrary() {
  const api = useApi();
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  async function propose() {
    setError(null);
    try {
      await api.post("/company-assets", {
        kind: "certificate",
        title: "Company certificate",
        verified: false,
        idempotency_key: `asset-${Date.now()}`,
      });
      setSaved(true);
    } catch (failure) {
      setError(failure);
    }
  }
  return (
    <section aria-labelledby="company-library-heading">
      <h2 id="company-library-heading">Company library</h2>
      <p className="muted">
        Company files stay separate from the current Tender. Expired
        certificates cannot satisfy eligibility.
      </p>
      <ErrorNotice error={error} />
      <button type="button" className="button" onClick={propose}>
        Add a company file
      </button>
      {saved ? (
        <p>
          Saved as a proposal. Approve reuse before it can be offered to a
          Tender.
        </p>
      ) : null}
    </section>
  );
}
