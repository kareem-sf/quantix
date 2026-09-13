import { useEffect, useRef, useState, type FormEvent } from "react";
import { RotateCcw, Save, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { ApiError, useApi, useResource, type Schema } from "../api";
import { FieldError } from "../components/FieldError";
import { ErrorNotice, Loading } from "../components/common";
import { createDraftScope, useFormDraft, writeFormDraft } from "./useFormDraft";

type ManagerProfile = Schema<"ManagerProfile">;
type Personality = Schema<"Personality">;
type PersonalityForm = {
  description: string;
  traits: string;
  communication_style: string;
  problem_solving_style: string;
  collaboration_style: string;
  uncertainty_handling: string;
  initiative: string;
  explanation_style: string;
  language_preferences: string;
  working_habits: string;
};
type ManagerForm = {
  display_name: string;
  title: string;
  persona: string;
  personality: PersonalityForm;
  working_preferences: string;
};
type PersonalityTextField = Exclude<
  keyof Personality,
  "traits" | "language_preferences" | "working_habits"
>;
type PersonalityListField =
  "traits" | "language_preferences" | "working_habits";

const DRAFT_FIELDS = [
  "display_name",
  "title",
  "persona",
  "personality",
  "working_preferences",
] as const;

const PERSONALITY_TEXT_FIELDS: ReadonlyArray<{
  key: PersonalityTextField;
  label: string;
  help: string;
}> = [
  {
    key: "communication_style",
    label: "Communication style",
    help: "Describe how the Manager should communicate with you.",
  },
  {
    key: "problem_solving_style",
    label: "Problem-solving style",
    help: "Describe how the Manager should work through a difficult tender question.",
  },
  {
    key: "collaboration_style",
    label: "Collaboration style",
    help: "Describe how the Manager should involve you and other colleagues.",
  },
  {
    key: "uncertainty_handling",
    label: "Uncertainty handling",
    help: "Describe how unknowns, gaps and assumptions should be presented.",
  },
  {
    key: "initiative",
    label: "Initiative",
    help: "Describe when the Manager should suggest a next action.",
  },
  {
    key: "explanation_style",
    label: "Explanation style",
    help: "Describe the level and order of detail that helps you decide.",
  },
];

const PERSONALITY_LIST_FIELDS: ReadonlyArray<{
  key: PersonalityListField;
  label: string;
}> = [
  { key: "traits", label: "Traits" },
  { key: "language_preferences", label: "Language preferences" },
  { key: "working_habits", label: "Working habits" },
];

export type ManagerPersonalityProps = {
  onClose?: () => void;
  onSaved?: (profile: ManagerProfile) => void;
};

/** Edit the global, versioned Tender Manager profile. */
export function ManagerPersonality({
  onClose,
  onSaved,
}: ManagerPersonalityProps = {}) {
  const api = useApi();
  const queryClient = useQueryClient();
  const profileQuery = useResource<ManagerProfile>("/manager-profile");
  const loadedProfile = useRef<ManagerProfile | null>(null);
  const [profile, setProfile] = useState<ManagerProfile | null>(null);

  useEffect(() => {
    if (profileQuery.data && !loadedProfile.current) {
      loadedProfile.current = profileQuery.data;
      setProfile(profileQuery.data);
    }
  }, [profileQuery.data]);

  const acceptProfile = (next: ManagerProfile) => {
    loadedProfile.current = next;
    setProfile(next);
    queryClient.setQueryData(["/manager-profile"], next);
  };

  if (!profile) {
    if (profileQuery.isPending) {
      return (
        <section className="manager-personality manager-personality-state">
          <Loading>Loading your Manager profile…</Loading>
        </section>
      );
    }
    return (
      <section className="manager-personality manager-personality-state">
        <div className="manager-personality-state-heading">
          <h2>Customize your Manager</h2>
          <p className="muted">
            The Manager profile could not be loaded. Your saved work is not
            changed.
          </p>
        </div>
        <ErrorNotice error={profileQuery.error} />
        <button
          type="button"
          className="button"
          onClick={() => void profileQuery.refetch()}
        >
          <RotateCcw size={16} aria-hidden="true" />
          Retry
        </button>
      </section>
    );
  }

  return (
    <ManagerPersonalityForm
      profile={profile}
      onClose={onClose}
      onProfileLoaded={acceptProfile}
      onSaved={(next) => {
        acceptProfile(next);
        onSaved?.(next);
      }}
      api={api}
    />
  );
}

function ManagerPersonalityForm({
  profile,
  onClose,
  onProfileLoaded,
  onSaved,
  api,
}: {
  profile: ManagerProfile;
  onClose?: () => void;
  onProfileLoaded: (profile: ManagerProfile) => void;
  onSaved: (profile: ManagerProfile) => void;
  api: ReturnType<typeof useApi>;
}) {
  const [pending, setPending] = useState(false);
  const [loadingSavedVersion, setLoadingSavedVersion] = useState(false);
  const [saved, setSaved] = useState(false);
  const [savedVersion, setSavedVersion] = useState<number | null>(null);
  const [newerEditsPending, setNewerEditsPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [conflict, setConflict] = useState(false);
  const profileScopeKey = `${profile.id}:${profile.version}`;
  const [skipDraftHydrationScope, setSkipDraftHydrationScope] = useState<
    string | null
  >(null);
  const [carryForward, setCarryForward] = useState<{
    scopeKey: string;
    value: ManagerForm;
  } | null>(null);
  const initialValue =
    carryForward?.scopeKey === profileScopeKey
      ? carryForward.value
      : profileToForm(profile);
  const formScope = createDraftScope(
    "manager-profile",
    profile.id,
    "personality",
    profile.version,
  );
  const formDraft = useFormDraft<ManagerForm>(
    formScope,
    initialValue,
    DRAFT_FIELDS,
    { hydrate: skipDraftHydrationScope !== profileScopeKey },
  );
  const latestValue = useRef(formDraft.value);
  const latestRevision = useRef(formDraft.revision);
  latestValue.current = formDraft.value;
  latestRevision.current = formDraft.revision;

  useEffect(() => {
    if (carryForward?.scopeKey === profileScopeKey) setCarryForward(null);
  }, [carryForward, profileScopeKey]);

  const draft = formDraft;

  const updatePersonalityText = (
    field: PersonalityTextField,
    value: string,
  ) => {
    draft.setValue((current) => ({
      ...current,
      personality: { ...current.personality, [field]: value },
    }));
    setSaved(false);
    setSavedVersion(null);
  };

  const updatePersonalityList = (
    field: PersonalityListField,
    value: string,
  ) => {
    draft.setValue((current) => ({
      ...current,
      personality: {
        ...current.personality,
        [field]: value,
      },
    }));
    setSaved(false);
    setSavedVersion(null);
  };

  const updateWorkingPreferences = (value: string) => {
    draft.setField("working_preferences", value);
    setSaved(false);
    setSavedVersion(null);
  };

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const submittedRevision = draft.revision;
    const submittedValue = draft.value;
    setPending(true);
    setError(null);
    setConflict(false);
    setSaved(false);
    setSavedVersion(null);
    const prepared = prepareEdit(submittedValue, profile.version);
    if ("error" in prepared) {
      setError(prepared.error);
      setPending(false);
      return;
    }
    try {
      const next = await api.patch<ManagerProfile>(
        "/manager-profile",
        prepared.payload,
      );
      const currentRevision = latestRevision.current;
      const hasNewerEdits =
        currentRevision !== submittedRevision ||
        latestValue.current !== submittedValue;
      if (!hasNewerEdits) {
        draft.markAccepted(submittedRevision);
      } else {
        // The accepted request only owns the revision it submitted. Carry a
        // newer in-flight edit into the saved version's scoped draft before
        // the form rebinds to that version.
        setCarryForward({
          scopeKey: `${next.id}:${next.version}`,
          value: latestValue.current,
        });
        const migrated = writeFormDraft(
          createDraftScope(
            "manager-profile",
            next.id,
            "personality",
            next.version,
          ),
          latestValue.current,
          DRAFT_FIELDS,
        );
        if (migrated.error) setError(migrated.error);
      }
      onSaved(next);
      setSavedVersion(next.version);
      setNewerEditsPending(hasNewerEdits);
      setSaved(true);
    } catch (failure) {
      setError(failure);
      setConflict(failure instanceof ApiError && failure.status === 409);
    } finally {
      setPending(false);
    }
  }

  async function loadSavedVersion() {
    if (pending || loadingSavedVersion) return;
    setLoadingSavedVersion(true);
    try {
      const next = await api.get<ManagerProfile>("/manager-profile");
      // Loading a saved version is the explicit discard action. Only clear
      // after the request succeeds so a failed reload keeps the user's draft.
      draft.clear();
      // A different editor may already have a draft for the newer version.
      // The fetched server snapshot wins for this explicit reload without
      // consuming that other draft; a normal close/reopen hydrates as usual.
      setSkipDraftHydrationScope(`${next.id}:${next.version}`);
      onProfileLoaded(next);
      setError(null);
      setConflict(false);
      setSaved(false);
      setSavedVersion(null);
    } catch (failure) {
      setError(failure);
    } finally {
      setLoadingSavedVersion(false);
    }
  }

  const conflictMessage = conflict ? (
    <div className="manager-personality-conflict">
      <div>
        <strong>A newer Manager version is saved.</strong>
        <p>
          Your edits are kept in this form. Load the saved version only when you
          are ready to review it and discard these edits.
        </p>
      </div>
      <button
        type="button"
        className="button"
        disabled={pending || loadingSavedVersion}
        onClick={() => void loadSavedVersion()}
      >
        {loadingSavedVersion
          ? "Loading saved version…"
          : "Load saved version and discard my edits"}
      </button>
    </div>
  ) : null;

  return (
    <section
      className="manager-personality"
      aria-labelledby="manager-personality-heading"
    >
      <header className="manager-personality-header">
        <div>
          <h2 id="manager-personality-heading">Customize your Manager</h2>
          <p className="manager-personality-lede">
            Set the working style used by your Tender Manager across projects.
            Current tasks keep the profile version they started with.
          </p>
        </div>
        {onClose ? (
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label="Close Manager customization"
          >
            <X size={20} aria-hidden="true" />
          </button>
        ) : null}
      </header>

      <p className="manager-personality-version">
        Editing Manager profile version <strong>{profile.version}</strong>
      </p>

      <form className="manager-personality-form" onSubmit={save}>
        <fieldset>
          <legend className="manager-visually-hidden">
            Manager personality
          </legend>
          <div className="manager-personality-primary">
            <label>
              <span className="manager-personality-primary-label">
                How should your Manager work with you?
              </span>
              <textarea
                required
                rows={6}
                maxLength={2000}
                dir="auto"
                aria-label="How should your Manager work with you?"
                value={draft.value.personality.description}
                onChange={(event) =>
                  updatePersonalityText("description", event.target.value)
                }
                aria-describedby="manager-description-help"
              />
              <span id="manager-description-help" className="field-help">
                Describe the working relationship you want. Up to 2,000
                characters.
              </span>
              <FieldError error={error} path={["personality", "description"]} />
            </label>
          </div>

          <details className="manager-personality-more">
            <summary>More options</summary>
            <div className="manager-personality-more-body">
              <section aria-labelledby="manager-identity-heading">
                <div className="manager-personality-section-heading">
                  <h3 id="manager-identity-heading">Manager details</h3>
                  <p className="muted">
                    The name and professional description shown in the office.
                  </p>
                </div>
                <div className="manager-personality-grid">
                  <label>
                    Manager name
                    <input
                      required
                      maxLength={120}
                      dir="auto"
                      value={draft.value.display_name}
                      onChange={(event) => {
                        draft.setField("display_name", event.target.value);
                        setSaved(false);
                        setSavedVersion(null);
                      }}
                    />
                    <FieldError error={error} name="display_name" />
                  </label>
                  <label>
                    Title
                    <input
                      required
                      maxLength={200}
                      dir="auto"
                      value={draft.value.title}
                      onChange={(event) => {
                        draft.setField("title", event.target.value);
                        setSaved(false);
                        setSavedVersion(null);
                      }}
                    />
                    <FieldError error={error} name="title" />
                  </label>
                </div>
                <label>
                  Persona
                  <textarea
                    required
                    rows={4}
                    maxLength={2000}
                    dir="auto"
                    value={draft.value.persona}
                    onChange={(event) => {
                      draft.setField("persona", event.target.value);
                      setSaved(false);
                      setSavedVersion(null);
                    }}
                  />
                  <FieldError error={error} name="persona" />
                </label>
              </section>

              <section aria-labelledby="manager-style-heading">
                <div className="manager-personality-section-heading">
                  <h3 id="manager-style-heading">Working style</h3>
                  <p className="muted">
                    Use your own words; these fields do not use a fixed set of
                    choices.
                  </p>
                </div>
                <div className="manager-personality-stack">
                  {PERSONALITY_TEXT_FIELDS.map(({ key, label, help }) => (
                    <label key={key}>
                      {label}
                      <textarea
                        required
                        rows={3}
                        maxLength={2000}
                        dir="auto"
                        aria-label={label}
                        value={draft.value.personality[key]}
                        onChange={(event) =>
                          updatePersonalityText(key, event.target.value)
                        }
                        aria-describedby={`manager-${key}-help`}
                      />
                      <span id={`manager-${key}-help`} className="field-help">
                        {help} Up to 2,000 characters.
                      </span>
                      <FieldError error={error} path={["personality", key]} />
                    </label>
                  ))}
                </div>
              </section>

              <section aria-labelledby="manager-lists-heading">
                <div className="manager-personality-section-heading">
                  <h3 id="manager-lists-heading">Traits and preferences</h3>
                  <p className="muted">
                    One entry per line, up to 20 entries and 500 characters per
                    entry.
                  </p>
                </div>
                <div className="manager-personality-stack">
                  {PERSONALITY_LIST_FIELDS.map(({ key, label }) => (
                    <label key={key}>
                      {label}
                      <textarea
                        rows={4}
                        maxLength={10019}
                        dir="auto"
                        aria-label={label}
                        value={draft.value.personality[key]}
                        onChange={(event) =>
                          updatePersonalityList(key, event.target.value)
                        }
                        aria-describedby={`manager-${key}-list-help`}
                      />
                      <span
                        id={`manager-${key}-list-help`}
                        className="field-help"
                      >
                        Leave blank when there is no preference. Keep each entry
                        on its own line.
                      </span>
                      <FieldError error={error} path={["personality", key]} />
                    </label>
                  ))}
                  <label>
                    Global working preferences
                    <textarea
                      rows={4}
                      maxLength={10019}
                      dir="auto"
                      aria-label="Global working preferences"
                      value={draft.value.working_preferences}
                      onChange={(event) =>
                        updateWorkingPreferences(event.target.value)
                      }
                      aria-describedby="manager-working-preferences-help"
                    />
                    <span
                      id="manager-working-preferences-help"
                      className="field-help"
                    >
                      These preferences apply across Tenders. One entry per
                      line, up to 20 entries and 500 characters per entry.
                    </span>
                    <FieldError error={error} name="working_preferences" />
                  </label>
                </div>
              </section>
            </div>
          </details>
        </fieldset>

        <p className="manager-personality-boundary">
          Personality changes guide communication and working style. They do not
          change Tender source access, model billing or approval decisions.
        </p>
        <ErrorNotice error={error || draft.error} />
        {conflictMessage}
        {saved ? (
          <p role="status" className="success-text">
            Manager profile saved as version {savedVersion ?? profile.version}.
            {newerEditsPending ? (
              <span>
                {" "}
                Your newer edits are still unsaved. Choose Save Manager to keep
                them.
              </span>
            ) : null}
          </p>
        ) : null}
        <div className="manager-personality-actions">
          {onClose ? (
            <button
              type="button"
              className="button"
              disabled={pending}
              onClick={onClose}
            >
              Cancel
            </button>
          ) : null}
          <button type="submit" className="button primary" disabled={pending}>
            <Save size={16} aria-hidden="true" />
            {pending ? "Saving…" : "Save Manager"}
          </button>
        </div>
      </form>
    </section>
  );
}

function profileToForm(profile: ManagerProfile): ManagerForm {
  return {
    display_name: profile.display_name,
    title: profile.title,
    persona: profile.persona,
    personality: {
      description: profile.personality.description,
      traits: profile.personality.traits.join("\n"),
      communication_style: profile.personality.communication_style,
      problem_solving_style: profile.personality.problem_solving_style,
      collaboration_style: profile.personality.collaboration_style,
      uncertainty_handling: profile.personality.uncertainty_handling,
      initiative: profile.personality.initiative,
      explanation_style: profile.personality.explanation_style,
      language_preferences: profile.personality.language_preferences.join("\n"),
      working_habits: profile.personality.working_habits.join("\n"),
    },
    working_preferences: profile.working_preferences.join("\n"),
  };
}

function prepareEdit(
  value: ManagerForm,
  expectedVersion: number,
): { payload: Schema<"ManagerProfileEdit"> } | { error: ApiError } {
  const traits = normalizeList(value.personality.traits, "Traits", [
    "personality",
    "traits",
  ]);
  if ("error" in traits) return traits;
  const languagePreferences = normalizeList(
    value.personality.language_preferences,
    "Language preferences",
    ["personality", "language_preferences"],
  );
  if ("error" in languagePreferences) return languagePreferences;
  const workingHabits = normalizeList(
    value.personality.working_habits,
    "Working habits",
    ["personality", "working_habits"],
  );
  if ("error" in workingHabits) return workingHabits;
  const workingPreferences = normalizeList(
    value.working_preferences,
    "Global working preferences",
    ["working_preferences"],
  );
  if ("error" in workingPreferences) return workingPreferences;
  return {
    payload: {
      expected_version: expectedVersion,
      display_name: value.display_name,
      title: value.title,
      persona: value.persona,
      personality: {
        description: value.personality.description,
        traits: traits.values,
        communication_style: value.personality.communication_style,
        problem_solving_style: value.personality.problem_solving_style,
        collaboration_style: value.personality.collaboration_style,
        uncertainty_handling: value.personality.uncertainty_handling,
        initiative: value.personality.initiative,
        explanation_style: value.personality.explanation_style,
        language_preferences: languagePreferences.values,
        working_habits: workingHabits.values,
      },
      working_preferences: workingPreferences.values,
    },
  };
}

function normalizeList(
  value: string,
  label: string,
  path: Array<string | number>,
): { values: string[] } | { error: ApiError } {
  const values = value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (values.length > 20) {
    return {
      error: localFieldError(path, `${label} can contain up to 20 entries.`),
    };
  }
  if (values.some((line) => line.length > 500)) {
    return {
      error: localFieldError(
        path,
        `${label} entries can contain up to 500 characters each.`,
      ),
    };
  }
  return { values };
}

function localFieldError(path: Array<string | number>, message: string) {
  return new ApiError(message, 422, undefined, [{ path, message }]);
}
