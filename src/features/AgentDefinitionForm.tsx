import { useState, type FormEvent } from "react";
import {
  GenerationControls,
  type GenerationSettings,
} from "../components/GenerationControls";
import { ErrorNotice } from "../components/common";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  DEFAULT_GENERATION_SETTINGS,
  stableKey,
  type FormMode,
  type ProfessionalProfile,
} from "./agentLibraryTypes";

export function AgentDefinitionForm({
  mode,
  onSave,
}: {
  mode: FormMode;
  onSave: (
    payload: {
      profile: ProfessionalProfile;
      generation_settings: GenerationSettings;
    },
    idempotencyKey: string,
  ) => Promise<void>;
}) {
  const initial =
    mode.kind === "edit" ? profileToForm(mode.definition.profile) : blankForm();
  const [form, setForm] = useState(initial);
  const [settings, setSettings] = useState<GenerationSettings>(
    mode.kind === "edit"
      ? mode.definition.generation_settings
      : DEFAULT_GENERATION_SETTINGS,
  );
  const [pending, setPending] = useState(false);
  const [profileOptionsOpen, setProfileOptionsOpen] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [idempotencyKey] = useState(() =>
    stableKey(mode.kind === "create" ? "definition-create" : "definition-edit"),
  );

  function field(name: keyof ProfileForm, value: string) {
    setForm((current) => ({ ...current, [name]: value }));
  }

  function personality(name: keyof PersonalityForm, value: string) {
    setForm((current) => ({
      ...current,
      personality: { ...current.personality, [name]: value },
    }));
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const missing = firstMissingField(form);
    if (missing) {
      setProfileOptionsOpen(missing.advanced);
      setError(
        new Error(`Complete ${missing.label} before saving this professional.`),
      );
      requestAnimationFrame(() => document.getElementById(missing.id)?.focus());
      return;
    }
    setPending(true);
    setError(null);
    try {
      await onSave(
        {
          profile: formToProfile(form),
          generation_settings: settings,
        },
        idempotencyKey,
      );
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="flex flex-col gap-5" onSubmit={submit} noValidate>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="agent-display-name">Display name</FieldLabel>
          <Input
            id="agent-display-name"
            required
            maxLength={120}
            value={form.display_name}
            onChange={(event) => field("display_name", event.target.value)}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field>
            <FieldLabel htmlFor="agent-role">Role</FieldLabel>
            <Input
              id="agent-role"
              required
              maxLength={200}
              value={form.role}
              onChange={(event) => field("role", event.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="agent-title">Title</FieldLabel>
            <Input
              id="agent-title"
              required
              maxLength={200}
              value={form.title}
              onChange={(event) => field("title", event.target.value)}
            />
          </Field>
        </div>
        <Field>
          <FieldLabel htmlFor="agent-persona">
            Professional description
          </FieldLabel>
          <Textarea
            id="agent-persona"
            required
            maxLength={2000}
            value={form.persona}
            onChange={(event) => field("persona", event.target.value)}
          />
          <FieldDescription>
            Describe the professional judgement and working approach this person
            brings.
          </FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor="agent-personality-description">
            How this professional works
          </FieldLabel>
          <Textarea
            id="agent-personality-description"
            required
            maxLength={2000}
            value={form.personality.description}
            onChange={(event) => personality("description", event.target.value)}
          />
        </Field>
      </FieldGroup>

      <details
        className="rounded-lg border p-4"
        open={profileOptionsOpen}
        onToggle={(event) => setProfileOptionsOpen(event.currentTarget.open)}
      >
        <summary className="cursor-pointer font-medium">
          More profile options · includes required fields
        </summary>
        <div className="mt-4 flex flex-col gap-5">
          <FieldSet>
            <FieldLegend>Professional scope</FieldLegend>
            <FieldGroup>
              {PROFILE_LIST_FIELDS.map(({ key, label, help }) => (
                <Field key={key}>
                  <FieldLabel htmlFor={`agent-${key}`}>{label}</FieldLabel>
                  <Textarea
                    id={`agent-${key}`}
                    required
                    maxLength={10019}
                    value={form[key]}
                    onChange={(event) => field(key, event.target.value)}
                  />
                  <FieldDescription>
                    {help ?? "Enter one item per line."}
                  </FieldDescription>
                </Field>
              ))}
              <Field>
                <FieldLabel htmlFor="agent-requested-tools">
                  Requested tools
                </FieldLabel>
                <Textarea
                  id="agent-requested-tools"
                  maxLength={10019}
                  value={form.requested_tool_ids}
                  onChange={(event) =>
                    field("requested_tool_ids", event.target.value)
                  }
                />
                <FieldDescription>
                  Tool IDs requested by the profile. This list grants no
                  permission.
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="agent-creation-reason">
                  Creation reason
                </FieldLabel>
                <Textarea
                  id="agent-creation-reason"
                  required
                  maxLength={2000}
                  value={form.creation_reason}
                  onChange={(event) =>
                    field("creation_reason", event.target.value)
                  }
                />
              </Field>
            </FieldGroup>
          </FieldSet>
          <FieldSet>
            <FieldLegend>Working style</FieldLegend>
            <FieldGroup>
              {PERSONALITY_TEXT_FIELDS.map(({ key, label }) => (
                <Field key={key}>
                  <FieldLabel htmlFor={`agent-personality-${key}`}>
                    {label}
                  </FieldLabel>
                  <Textarea
                    id={`agent-personality-${key}`}
                    required
                    maxLength={2000}
                    value={form.personality[key]}
                    onChange={(event) => personality(key, event.target.value)}
                  />
                </Field>
              ))}
              {PERSONALITY_LIST_FIELDS.map(({ key, label }) => (
                <Field key={key}>
                  <FieldLabel htmlFor={`agent-personality-${key}`}>
                    {label}
                  </FieldLabel>
                  <Textarea
                    id={`agent-personality-${key}`}
                    maxLength={10019}
                    value={form.personality[key]}
                    onChange={(event) => personality(key, event.target.value)}
                  />
                  <FieldDescription>
                    Enter one item per line, or leave blank.
                  </FieldDescription>
                </Field>
              ))}
            </FieldGroup>
          </FieldSet>
        </div>
      </details>

      <GenerationControls value={settings} onChange={setSettings} />
      <ErrorNotice error={error} />
      <div className="flex justify-end gap-2">
        <Button type="submit" disabled={pending}>
          {pending
            ? "Saving…"
            : mode.kind === "create"
              ? "Create professional"
              : "Save new version"}
        </Button>
      </div>
    </form>
  );
}

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

type ProfileForm = Omit<
  ProfessionalProfile,
  | "personality"
  | "specialisms"
  | "responsibilities"
  | "objectives"
  | "methods"
  | "deliverables"
  | "success_criteria"
  | "context_needs"
  | "requested_tool_ids"
> & {
  personality: PersonalityForm;
  specialisms: string;
  responsibilities: string;
  objectives: string;
  methods: string;
  deliverables: string;
  success_criteria: string;
  context_needs: string;
  requested_tool_ids: string;
};

const PROFILE_LIST_FIELDS: ReadonlyArray<{
  key:
    | "specialisms"
    | "responsibilities"
    | "objectives"
    | "methods"
    | "deliverables"
    | "success_criteria"
    | "context_needs";
  label: string;
  help?: string;
}> = [
  { key: "specialisms", label: "Specialisms" },
  { key: "responsibilities", label: "Responsibilities" },
  { key: "objectives", label: "Objectives" },
  { key: "methods", label: "Methods" },
  { key: "deliverables", label: "Deliverables" },
  { key: "success_criteria", label: "Success criteria" },
  {
    key: "context_needs",
    label: "Context needed",
    help: "List the information this professional normally needs. Do not add Tender source IDs.",
  },
];

const PERSONALITY_TEXT_FIELDS: ReadonlyArray<{
  key:
    | "communication_style"
    | "problem_solving_style"
    | "collaboration_style"
    | "uncertainty_handling"
    | "initiative"
    | "explanation_style";
  label: string;
}> = [
  { key: "communication_style", label: "Communication style" },
  { key: "problem_solving_style", label: "Problem-solving style" },
  { key: "collaboration_style", label: "Collaboration style" },
  { key: "uncertainty_handling", label: "Uncertainty handling" },
  { key: "initiative", label: "Initiative" },
  { key: "explanation_style", label: "Explanation style" },
];

const PERSONALITY_LIST_FIELDS: ReadonlyArray<{
  key: "traits" | "language_preferences" | "working_habits";
  label: string;
}> = [
  { key: "traits", label: "Traits" },
  { key: "language_preferences", label: "Language preferences" },
  { key: "working_habits", label: "Working habits" },
];

function blankForm(): ProfileForm {
  return {
    display_name: "",
    role: "",
    title: "",
    specialisms: "",
    persona: "",
    personality: {
      description: "",
      traits: "",
      communication_style: "",
      problem_solving_style: "",
      collaboration_style: "",
      uncertainty_handling: "",
      initiative: "",
      explanation_style: "",
      language_preferences: "",
      working_habits: "",
    },
    responsibilities: "",
    objectives: "",
    methods: "",
    deliverables: "",
    success_criteria: "",
    context_needs: "",
    requested_tool_ids: "",
    creation_reason: "",
  };
}

function profileToForm(profile: ProfessionalProfile): ProfileForm {
  return {
    ...profile,
    specialisms: linesText(profile.specialisms),
    responsibilities: linesText(profile.responsibilities),
    objectives: linesText(profile.objectives),
    methods: linesText(profile.methods),
    deliverables: linesText(profile.deliverables),
    success_criteria: linesText(profile.success_criteria),
    context_needs: linesText(profile.context_needs),
    requested_tool_ids: linesText(profile.requested_tool_ids),
    personality: {
      ...profile.personality,
      traits: linesText(profile.personality.traits),
      language_preferences: linesText(profile.personality.language_preferences),
      working_habits: linesText(profile.personality.working_habits),
    },
  };
}

function formToProfile(form: ProfileForm): ProfessionalProfile {
  return {
    ...form,
    specialisms: splitLines(form.specialisms),
    responsibilities: splitLines(form.responsibilities),
    objectives: splitLines(form.objectives),
    methods: splitLines(form.methods),
    deliverables: splitLines(form.deliverables),
    success_criteria: splitLines(form.success_criteria),
    context_needs: splitLines(form.context_needs),
    requested_tool_ids: splitLines(form.requested_tool_ids),
    personality: {
      ...form.personality,
      traits: splitLines(form.personality.traits),
      language_preferences: splitLines(form.personality.language_preferences),
      working_habits: splitLines(form.personality.working_habits),
    },
  };
}

function splitLines(value: string) {
  return value
    .split(/\r?\n/u)
    .map((item) => item.trim())
    .filter(Boolean);
}

function linesText(value: string[]) {
  return value.join("\n");
}

function firstMissingField(form: ProfileForm) {
  const fields: Array<{
    label: string;
    id: string;
    value: string;
    advanced: boolean;
  }> = [
    {
      label: "Display name",
      id: "agent-display-name",
      value: form.display_name,
      advanced: false,
    },
    { label: "Role", id: "agent-role", value: form.role, advanced: false },
    { label: "Title", id: "agent-title", value: form.title, advanced: false },
    {
      label: "Professional description",
      id: "agent-persona",
      value: form.persona,
      advanced: false,
    },
    {
      label: "How this professional works",
      id: "agent-personality-description",
      value: form.personality.description,
      advanced: false,
    },
    ...PROFILE_LIST_FIELDS.map(({ key, label }) => ({
      label,
      id: `agent-${key}`,
      value: form[key],
      advanced: true,
    })),
    ...PERSONALITY_TEXT_FIELDS.map(({ key, label }) => ({
      label,
      id: `agent-personality-${key}`,
      value: form.personality[key],
      advanced: true,
    })),
    {
      label: "Creation reason",
      id: "agent-creation-reason",
      value: form.creation_reason,
      advanced: true,
    },
  ];
  return fields.find(({ value }) => !value.trim()) ?? null;
}
