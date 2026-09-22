import createClient from "openapi-fetch";
import type { components, paths } from "./schema";

export type Tender = components["schemas"]["TenderOut"];

// The development server forwards /api to the local service and adds the access token.
export const api = createClient<paths>({
  baseUrl: `${window.location.origin}/api`,
  fetch: (request) => globalThis.fetch(request),
});

/** The response data, or an error carrying the service's own message. */
export function must<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.data !== undefined) return result.data;
  const detail = (result.error as { detail?: unknown } | undefined)?.detail;
  throw new Error(typeof detail === "string" ? detail : `The Quantix service answered ${result.response.status}.`);
}
