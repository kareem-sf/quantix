import { ApiError, type ApiFieldError } from "../api";

/**
 * A compact validation message that can sit directly under the input it
 * describes. The API transport strips pydantic input/context values before
 * they reach this component, so this is safe to render beside credentials.
 */
export function FieldError({
  error,
  name,
  path,
}: {
  error: unknown;
  /** HTML field name, or the final path segment used by the form. */
  name?: string;
  path?: Array<string | number>;
}) {
  const target = path ?? (name ? [name] : undefined);
  const match = target ? findFieldError(error, target) : undefined;
  if (!match) return null;
  return (
    <span className="field-error" role="alert">
      {match.message}
    </span>
  );
}

export function findFieldError(
  error: unknown,
  target: Array<string | number>,
): ApiFieldError | undefined {
  if (!(error instanceof ApiError)) return undefined;
  return error.fieldErrors.find((candidate) => {
    if (candidate.path.length < target.length) return false;
    const suffix = candidate.path.slice(-target.length);
    return suffix.every((part, index) => part === target[index]);
  });
}
