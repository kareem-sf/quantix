import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  createDraftScope,
  readFormDraft,
  removeFormDraft,
  useFormDraft,
  writeFormDraft,
} from "./useFormDraft";

describe("form drafts", () => {
  it("isolates persisted fields by namespace, Tender, form and version", () => {
    const storage = window.localStorage;
    const first = createDraftScope("measurements", "tender-a", "drawing", 1);
    const second = createDraftScope("measurements", "tender-a", "drawing", 2);
    const otherTender = createDraftScope(
      "measurements",
      "tender-b",
      "drawing",
      1,
    );

    expect(
      writeFormDraft(
        first,
        { scope: "north", password: "never" },
        ["scope"],
        storage,
      ).error,
    ).toBeNull();
    expect(readFormDraft(first, ["scope", "password"], storage).value).toEqual({
      scope: "north",
    });
    expect(readFormDraft(second, ["scope"], storage).value).toBeNull();
    expect(readFormDraft(otherTender, ["scope"], storage).value).toBeNull();
    expect(storage.length).toBe(1);
    expect(storage.getItem(Object.keys(storage)[0]!)).not.toContain("password");
  });

  it("does not remove a newer edit when an older save is acknowledged", () => {
    const storage = window.localStorage;
    const scope = createDraftScope("forms", "tender", "quote", "v1");
    const written = writeFormDraft(
      scope,
      { subject: "first" },
      ["subject"],
      storage,
    );
    expect(written.revision).toBeTruthy();
    writeFormDraft(scope, { subject: "newer" }, ["subject"], storage);
    expect(removeFormDraft(scope, written.revision, storage).removed).toBe(
      false,
    );
    expect(readFormDraft(scope, ["subject"], storage).value).toEqual({
      subject: "newer",
    });
  });

  it("hydrates once and surfaces storage failures without losing the visible value", () => {
    const storage = {
      getItem: vi.fn(() =>
        JSON.stringify({
          schema: 1,
          namespace: "forms",
          tender_id: "tender",
          form: "form",
          version: "1",
          updated_at: "2026-09-09T00:00:00Z",
          fields: { subject: "saved" },
          revision: "r1",
        }),
      ),
      setItem: vi.fn(() => {
        throw new Error("quota");
      }),
      removeItem: vi.fn(),
      clear: vi.fn(),
      length: 1,
      key: vi.fn(() => "key"),
    } satisfies Storage;
    const scope = createDraftScope("forms", "tender", "form", 1);
    const view = renderHook(() =>
      useFormDraft(scope, { subject: "initial" }, ["subject"], { storage }),
    );
    expect(view.result.current.value).toEqual({ subject: "saved" });
    act(() => view.result.current.setValue({ subject: "typed" }));
    expect(view.result.current.value).toEqual({ subject: "typed" });
    expect(view.result.current.error?.message).toMatch(/copy/i);
    view.rerender();
    expect(view.result.current.value).toEqual({ subject: "typed" });
  });

  it("rebinds when a form version changes so a previous version cannot restore geometry", () => {
    const storage = window.localStorage;
    const first = createDraftScope(
      "measurements",
      "tender",
      "drawing",
      "hash-a:1:12",
    );
    const second = createDraftScope(
      "measurements",
      "tender",
      "drawing",
      "hash-b:2:12",
    );
    writeFormDraft(first, { points: [[0.1, 0.2]] }, ["points"], storage);
    const view = renderHook(
      ({ scope }) =>
        useFormDraft(scope, { points: [] as [number, number][] }, ["points"], {
          storage,
        }),
      { initialProps: { scope: first } },
    );
    expect(view.result.current.value.points).toEqual([[0.1, 0.2]]);
    view.rerender({ scope: second });
    expect(view.result.current.value.points).toEqual([]);
  });

  it("keeps a newer edit when an accepted request began before the first local save", () => {
    const scope = createDraftScope("forms", "tender", "form", 1);
    const view = renderHook(() =>
      useFormDraft(scope, { subject: "" }, ["subject"]),
    );
    const acceptedRevision = view.result.current.revision;
    act(() => view.result.current.setField("subject", "newer"));
    expect(view.result.current.markAccepted(acceptedRevision).removed).toBe(
      false,
    );
    expect(readFormDraft(scope, ["subject"]).value).toEqual({
      subject: "newer",
    });
  });

  it("can skip one explicit reload hydration without removing the stored draft", () => {
    const storage = window.localStorage;
    const scope = createDraftScope("forms", "manager", "profile", 2);
    writeFormDraft(
      scope,
      { subject: "another editor draft" },
      ["subject"],
      storage,
    );

    const view = renderHook(
      ({ hydrate }) =>
        useFormDraft(scope, { subject: "server version" }, ["subject"], {
          hydrate,
          storage,
        }),
      { initialProps: { hydrate: false } },
    );
    expect(view.result.current.value.subject).toBe("server version");
    expect(readFormDraft(scope, ["subject"], storage).value).toEqual({
      subject: "another editor draft",
    });

    view.rerender({ hydrate: true });
    expect(view.result.current.value.subject).toBe("server version");
    view.unmount();
    const reopened = renderHook(() =>
      useFormDraft(scope, { subject: "server version" }, ["subject"], {
        storage,
      }),
    );
    expect(reopened.result.current.value.subject).toBe("another editor draft");
  });

  it("surfaces unavailable local storage instead of silently dropping a draft", () => {
    const scope = createDraftScope("forms", "tender", "form", 1);
    expect(
      readFormDraft(scope, ["subject"], null as unknown as Storage).error
        ?.message,
    ).toMatch(/unavailable|copy/i);
    expect(
      writeFormDraft(
        scope,
        { subject: "draft" },
        ["subject"],
        null as unknown as Storage,
      ).error?.message,
    ).toMatch(/unavailable|copy/i);
  });
});
