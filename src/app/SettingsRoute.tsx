import { useLocation, useNavigate } from "react-router-dom";
import { Settings } from "../features/Settings";

export function SettingsRoute() {
  const location = useLocation();
  const navigate = useNavigate();
  const params = new URLSearchParams(location.search);

  function update(patch: Record<string, string | null>) {
    const next = new URLSearchParams(location.search);
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) next.delete(key);
      else next.set(key, value);
    }
    const query = next.toString();
    void navigate(`/settings${query ? `?${query}` : ""}`, { replace: true });
  }

  // Section navigation, the way back and the theme live in the app sidebar.
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto w-full max-w-5xl px-6 py-8">
        <Settings
          section={params.get("section") ?? undefined}
          onSection={(section) => update({ section })}
          connectionId={params.get("connection") ?? undefined}
          onConnectionClose={() => update({ connection: null })}
        />
      </div>
    </div>
  );
}
