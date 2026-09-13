import { EvidencePicker } from "./EvidencePicker";
import type { Schema } from "../api";
import { FieldError } from "../components/FieldError";

type ActivityDraft = {
  key: number;
  id: string;
  title: string;
  duration: string;
  predecessors: string;
  sourceIds: string[];
  assumptions: string;
};
export type ProgrammeDraft = {
  title: string;
  start: string;
  week: number[];
  holidays: string;
  assumptions: string;
  activities: ActivityDraft[];
};
export const emptyProgramme = (): ProgrammeDraft => ({
  title: "",
  start: "",
  week: [],
  holidays: "",
  assumptions: "",
  activities: [],
});
export function programmeDraft(
  programme: Schema<"ConstructionProgramme">,
): ProgrammeDraft {
  return {
    title: programme.title,
    start: programme.start_date,
    week: programme.working_week,
    holidays: (programme.holidays ?? []).join("\n"),
    assumptions: (programme.assumptions ?? []).join("\n"),
    activities: programme.activities.map((activity, index) => ({
      key: index + 1,
      id: activity.id,
      title: activity.title,
      duration: String(activity.duration_days),
      predecessors: (activity.predecessor_ids ?? []).join(", "),
      sourceIds: activity.source_ids ?? [],
      assumptions: (activity.assumptions ?? []).join("\n"),
    })),
  };
}
const weekdays = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];
const lines = (text: string) =>
  text
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter(Boolean);
export function programmeValue(
  value: ProgrammeDraft,
): Schema<"ConstructionProgramme"> | null {
  if (
    !value.title.trim() ||
    !value.start ||
    !value.week.length ||
    !value.activities.length ||
    value.activities.some(
      (activity) =>
        !activity.id.trim() ||
        !activity.title.trim() ||
        !/^\d+$/.test(activity.duration) ||
        +activity.duration < 1 ||
        +activity.duration > 10000,
    )
  )
    return null;
  return {
    title: value.title.trim(),
    start_date: value.start,
    working_week: [...value.week].sort(),
    holidays: lines(value.holidays),
    assumptions: lines(value.assumptions),
    activities: value.activities.map((activity) => ({
      id: activity.id.trim(),
      title: activity.title.trim(),
      duration_days: Number(activity.duration),
      predecessor_ids: activity.predecessors.split(/[\s,]+/).filter(Boolean),
      source_ids: activity.sourceIds,
      assumptions: lines(activity.assumptions),
    })),
  };
}
export function ProgrammeForm({
  tenderId,
  value,
  onChange,
  error,
}: {
  tenderId: string;
  value: ProgrammeDraft;
  onChange: (value: ProgrammeDraft) => void;
  error?: unknown;
}) {
  const update = (key: number, patch: Partial<ActivityDraft>) =>
    onChange({
      ...value,
      activities: value.activities.map((activity) =>
        activity.key === key ? { ...activity, ...patch } : activity,
      ),
    });
  return (
    <div className="programme-form">
      <p className="muted">
        Enter the construction activities, durations and calendar you have
        reviewed. Working days and dates are calculated from these inputs.
      </p>
      <label>
        Programme title
        <input
          required
          maxLength={300}
          value={value.title}
          onChange={(event) =>
            onChange({ ...value, title: event.target.value })
          }
        />
        <FieldError error={error} path={["programme", "title"]} />
      </label>
      <label>
        Start date
        <input
          type="date"
          required
          value={value.start}
          onChange={(event) =>
            onChange({ ...value, start: event.target.value })
          }
        />
        <FieldError error={error} path={["programme", "start_date"]} />
      </label>
      <fieldset className="programme-week">
        <legend>Working days</legend>
        {weekdays.map((day, index) => (
          <label className="checkbox-label" key={day}>
            <input
              type="checkbox"
              checked={value.week.includes(index)}
              onChange={(event) =>
                onChange({
                  ...value,
                  week: event.target.checked
                    ? [...value.week, index]
                    : value.week.filter((value) => value !== index),
                })
              }
            />
            {day}
          </label>
        ))}
      </fieldset>
      <label>
        Non-working dates
        <textarea
          rows={2}
          value={value.holidays}
          onChange={(event) =>
            onChange({ ...value, holidays: event.target.value })
          }
          placeholder="One YYYY-MM-DD date per line"
        />
        <FieldError error={error} path={["programme", "holidays"]} />
      </label>
      <label>
        Programme assumptions
        <textarea
          rows={2}
          maxLength={200000}
          value={value.assumptions}
          onChange={(event) =>
            onChange({ ...value, assumptions: event.target.value })
          }
          placeholder="One assumption per line"
        />
        <FieldError error={error} path={["programme", "assumptions"]} />
      </label>
      {value.activities.map((activity, index) => (
        <fieldset key={activity.key} className="programme-activity">
          <legend>Activity {index + 1}</legend>
          <div className="form-grid">
            <label>
              Activity ID
              <input
                required
                pattern="[A-Za-z0-9_-]+"
                maxLength={80}
                value={activity.id}
                onChange={(event) =>
                  update(activity.key, { id: event.target.value })
                }
              />
              <FieldError
                error={error}
                path={["programme", "activities", index, "id"]}
              />
            </label>
            <label>
              Duration in working days
              <input
                required
                type="number"
                min={1}
                max={10000}
                step={1}
                value={activity.duration}
                onChange={(event) =>
                  update(activity.key, { duration: event.target.value })
                }
              />
              <FieldError
                error={error}
                path={["programme", "activities", index, "duration_days"]}
              />
            </label>
          </div>
          <label>
            Activity title
            <input
              required
              maxLength={300}
              value={activity.title}
              onChange={(event) =>
                update(activity.key, { title: event.target.value })
              }
            />
            <FieldError
              error={error}
              path={["programme", "activities", index, "title"]}
            />
          </label>
          <label>
            Predecessor activity IDs
            <input
              value={activity.predecessors}
              onChange={(event) =>
                update(activity.key, { predecessors: event.target.value })
              }
              placeholder="Activity IDs separated by commas"
            />
            <FieldError
              error={error}
              path={["programme", "activities", index, "predecessor_ids"]}
            />
          </label>
          <label>
            Activity assumptions
            <textarea
              rows={2}
              value={activity.assumptions}
              onChange={(event) =>
                update(activity.key, { assumptions: event.target.value })
              }
              placeholder="One assumption per line"
            />
            <FieldError
              error={error}
              path={["programme", "activities", index, "assumptions"]}
            />
          </label>
          <EvidencePicker
            tenderId={tenderId}
            selected={activity.sourceIds}
            onChange={(sourceIds) => update(activity.key, { sourceIds })}
          />
          <button
            type="button"
            className="text-button"
            onClick={() =>
              onChange({
                ...value,
                activities: value.activities.filter(
                  (item) => item.key !== activity.key,
                ),
              })
            }
          >
            Remove activity {index + 1}
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        className="button"
        disabled={value.activities.length >= 500}
        onClick={() =>
          onChange({
            ...value,
            activities: [
              ...value.activities,
              {
                key:
                  Math.max(
                    0,
                    ...value.activities.map((activity) => activity.key),
                  ) + 1,
                id: "",
                title: "",
                duration: "",
                predecessors: "",
                sourceIds: [],
                assumptions: "",
              },
            ],
          })
        }
      >
        Add activity
      </button>
    </div>
  );
}
