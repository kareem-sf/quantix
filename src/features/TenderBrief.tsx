import { useResource, tenderPath, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";

export function TenderBrief({ tenderId }: { tenderId: string }) {
  const profile = useResource<Schema<"TenderProfile">>(
    `${tenderPath(tenderId)}/profile`,
  );
  if (profile.isPending) return <Loading>Loading the Tender brief…</Loading>;
  if (!profile.data) return <ErrorNotice error={profile.error} />;
  const unknown = profile.data.measurement_method
    ? profile.data.measurement_method
    : "Not yet confirmed";
  return (
    <section className="tender-brief" aria-labelledby="tender-brief-heading">
      <h2 id="tender-brief-heading">Tender brief</h2>
      <p className="muted">
        Company defaults are not treated as confirmed facts for this Tender.
      </p>
      <dl>
        <dt>Measurement method</dt>
        <dd>{unknown}</dd>
        <dt>Currencies</dt>
        <dd>{profile.data.currencies?.join(", ") || "Not yet confirmed"}</dd>
        <dt>Timezone</dt>
        <dd>{profile.data.timezone || "Not yet confirmed"}</dd>
      </dl>
    </section>
  );
}
