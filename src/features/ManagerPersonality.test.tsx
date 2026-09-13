import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, it, vi } from "vitest";
import { ApiContext, createApi, type Schema } from "../api";
import { ManagerPersonality } from "./ManagerPersonality";
import {
  createDraftScope,
  readFormDraft,
  writeFormDraft,
} from "./useFormDraft";

function makeProfile(version = 1): Schema<"ManagerProfile"> {
  return {
    id: "manager-global",
    version,
    display_name: "Tender Manager",
    title: "Tender Manager",
    persona: "An evidence-led coordinator for construction tenders.",
    personality: {
      description: "Keep the work clear and grounded in the tender evidence.",
      traits: ["Careful", "Direct"],
      communication_style: "Use plain construction language.",
      problem_solving_style: "Break difficult work into checked steps.",
      collaboration_style: "Ask before a decision needs engineer input.",
      uncertainty_handling: "Name unknowns and the next check.",
      initiative: "Suggest the next useful action.",
      explanation_style: "Give the answer first, then the evidence.",
      language_preferences: ["English", "Arabic when requested"],
      working_habits: ["Keep source references with findings"],
    },
    working_preferences: ["State the next action clearly"],
    created_at: "2026-09-10T00:00:00Z",
    updated_at: "2026-09-10T00:00:00Z",
  };
}

function makeDraft(profile: Schema<"ManagerProfile">, description?: string) {
  return {
    display_name: profile.display_name,
    title: profile.title,
    persona: profile.persona,
    personality: {
      description: description ?? profile.personality.description,
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

function renderEditor(
  fetcher: Parameters<typeof createApi>[1],
  props: Parameters<typeof ManagerPersonality>[0] = {},
) {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    fetcher,
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <ManagerPersonality {...props} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return userEvent.setup({ delay: null });
}

beforeEach(() => {
  window.localStorage.clear();
});

it("sends the complete editable profile, including custom Arabic list entries", async () => {
  let body: Record<string, unknown> | undefined;
  const user = renderEditor(async (url, init) => {
    if (init?.method === "PATCH") {
      body = JSON.parse(String(init.body));
      return new Response(JSON.stringify(makeProfile(2)));
    }
    return new Response(JSON.stringify(makeProfile()));
  });

  await screen.findByLabelText("How should your Manager work with you?");
  await user.click(screen.getByText("More options"));
  const traits = screen.getByLabelText("Traits");
  await user.clear(traits);
  await user.type(traits, "تسليح مخصص, مع مراجعة");
  await user.keyboard("{Enter}");
  await user.type(traits, "Structural checks");
  const workingPreferences = screen.getByLabelText(
    "Global working preferences",
  );
  await user.clear(workingPreferences);
  await user.type(workingPreferences, "استخدم المراجع الأصلية");
  await user.keyboard("{Enter}");
  await user.type(workingPreferences, "Keep calculations visible");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));

  await screen.findByText("Manager profile saved as version 2.");
  expect(body).toBeDefined();
  expect(Object.keys(body ?? {}).sort()).toEqual([
    "display_name",
    "expected_version",
    "persona",
    "personality",
    "title",
    "working_preferences",
  ]);
  expect(body).toMatchObject({
    expected_version: 1,
    personality: expect.objectContaining({
      traits: ["تسليح مخصص, مع مراجعة", "Structural checks"],
    }),
    working_preferences: [
      "استخدم المراجع الأصلية",
      "Keep calculations visible",
    ],
  });
  expect(body).not.toHaveProperty("provider");
  expect(body).not.toHaveProperty("api_key");
  expect(body).not.toHaveProperty("spending_authority");
});

it("shows a retry action when the profile cannot be loaded", async () => {
  let attempts = 0;
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "GET") {
      attempts += 1;
      if (attempts < 3) throw new TypeError("offline");
    }
    return new Response(JSON.stringify(makeProfile()));
  });

  expect(
    await screen.findByRole("alert", {}, { timeout: 3000 }),
  ).toHaveTextContent("local service could not be reached");
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(
    await screen.findByLabelText("How should your Manager work with you?"),
  ).toBeInTheDocument();
  expect(attempts).toBe(3);
});

it("keeps every edit after a version conflict until the engineer loads the saved version", async () => {
  let patchCalls = 0;
  let getCalls = 0;
  const savedProfile = {
    ...makeProfile(2),
    personality: {
      ...makeProfile(2).personality,
      description: "Saved by another change.",
    },
  };
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "PATCH") {
      patchCalls += 1;
      return new Response(
        JSON.stringify({ detail: "A newer Manager version was saved." }),
        {
          status: 409,
        },
      );
    }
    getCalls += 1;
    return new Response(
      JSON.stringify(getCalls === 1 ? makeProfile() : savedProfile),
    );
  });

  const description = await screen.findByLabelText(
    "How should your Manager work with you?",
  );
  await user.clear(description);
  await user.type(description, "Keep my complete working note.");
  await user.click(screen.getByText("More options"));
  const preferences = screen.getByLabelText("Global working preferences");
  await user.clear(preferences);
  await user.type(preferences, "Do not lose this list.");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "A newer Manager version was saved.",
  );
  expect(description).toHaveValue("Keep my complete working note.");
  expect(preferences).toHaveValue("Do not lose this list.");
  expect(patchCalls).toBe(1);

  await user.click(
    screen.getByRole("button", {
      name: "Load saved version and discard my edits",
    }),
  );
  expect(
    await screen.findByDisplayValue("Saved by another change."),
  ).toBeInTheDocument();
  expect(getCalls).toBe(2);
});

it("shows the fetched saved version on explicit reload without consuming another editor's target-version draft", async () => {
  const versionOne = makeProfile(1);
  const versionTwo = {
    ...makeProfile(2),
    personality: {
      ...makeProfile(2).personality,
      description: "The exact saved server version.",
    },
  };
  const targetScope = createDraftScope(
    "manager-profile",
    versionTwo.id,
    "personality",
    versionTwo.version,
  );
  const targetDraft = makeDraft(versionTwo, "Another editor's v2 draft.");
  writeFormDraft(targetScope, targetDraft, [
    "display_name",
    "title",
    "persona",
    "personality",
    "working_preferences",
  ]);
  let getCalls = 0;
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "PATCH") {
      return new Response(
        JSON.stringify({ detail: "A newer Manager version was saved." }),
        {
          status: 409,
        },
      );
    }
    getCalls += 1;
    return new Response(
      JSON.stringify(getCalls === 1 ? versionOne : versionTwo),
    );
  });

  const description = await screen.findByLabelText(
    "How should your Manager work with you?",
  );
  await user.clear(description);
  await user.type(description, "My v1 edit stays visible after conflict.");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));
  await screen.findByRole("alert");
  await user.click(
    screen.getByRole("button", {
      name: "Load saved version and discard my edits",
    }),
  );

  expect(
    await screen.findByDisplayValue("The exact saved server version."),
  ).toBeInTheDocument();
  expect(
    readFormDraft(targetScope, ["personality"]).value?.personality,
  ).toMatchObject({ description: "Another editor's v2 draft." });
  expect(getCalls).toBe(2);
});

it("reports the saved version and callback after a successful save", async () => {
  const onSaved = vi.fn();
  const user = renderEditor(
    async (_url, init) =>
      new Response(
        JSON.stringify(
          init?.method === "PATCH" ? makeProfile(2) : makeProfile(),
        ),
      ),
    { onSaved },
  );

  await screen.findByLabelText("How should your Manager work with you?");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));
  await screen.findByText("Manager profile saved as version 2.");
  expect(onSaved).toHaveBeenCalledWith(expect.objectContaining({ version: 2 }));
});

it("keeps a newer draft edit made while the save is in flight", async () => {
  let resolvePatch: ((response: Response) => void) | undefined;
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "PATCH") {
      return new Promise<Response>((resolve) => {
        resolvePatch = resolve;
      });
    }
    return new Response(JSON.stringify(makeProfile()));
  });

  const description = await screen.findByLabelText(
    "How should your Manager work with you?",
  );
  await user.clear(description);
  await user.type(description, "First submitted instruction.");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));
  await user.clear(description);
  await user.type(description, "New edit while saving.");
  resolvePatch?.(new Response(JSON.stringify(makeProfile(2))));

  await screen.findByText("Manager profile saved as version 2.");
  expect(description).toHaveValue("New edit while saving.");
  expect(screen.getByRole("status")).toHaveTextContent(
    "Your newer edits are still unsaved",
  );
});

it("keeps a newer in-flight edit when its next-version draft cannot be stored", async () => {
  const storage = window.localStorage;
  const failingStorage = {
    get length() {
      return storage.length;
    },
    clear: () => storage.clear(),
    getItem: (key: string) => storage.getItem(key),
    key: (index: number) => storage.key(index),
    removeItem: (key: string) => storage.removeItem(key),
    setItem: (key: string, value: string) => {
      if (key.endsWith(":2")) throw new Error("quota");
      storage.setItem(key, value);
    },
  } satisfies Storage;
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: failingStorage,
  });
  let resolvePatch: ((response: Response) => void) | undefined;
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "PATCH") {
      return new Promise<Response>((resolve) => {
        resolvePatch = resolve;
      });
    }
    return new Response(JSON.stringify(makeProfile()));
  });

  const description = await screen.findByLabelText(
    "How should your Manager work with you?",
  );
  await user.clear(description);
  await user.type(description, "First submitted instruction.");
  await user.click(screen.getByRole("button", { name: "Save Manager" }));
  await user.clear(description);
  await user.type(description, "New edit survives storage failure.");
  resolvePatch?.(new Response(JSON.stringify(makeProfile(2))));

  await screen.findByText("Manager profile saved as version 2.");
  expect(description).toHaveValue("New edit survives storage failure.");
  expect(screen.getByRole("alert")).toHaveTextContent(
    "draft could not be saved",
  );
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: storage,
  });
});

it("keeps in-memory edits when storage stops working during the save", async () => {
  let saving = false;
  let resolvePatch: ((response: Response) => void) | undefined;
  const originalSetItem = Storage.prototype.setItem;
  const write = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, key: string, value: string) {
      if (saving) throw new Error("synthetic storage failure");
      originalSetItem.call(this, key, value);
    });
  try {
    const user = renderEditor(async (_url, init) => {
      if (init?.method === "PATCH") {
        saving = true;
        return new Promise<Response>((resolve) => {
          resolvePatch = resolve;
        });
      }
      return new Response(JSON.stringify(makeProfile()));
    });
    const description = await screen.findByLabelText(
      "How should your Manager work with you?",
    );
    await user.clear(description);
    await user.type(description, "Submitted version.");
    await user.click(screen.getByRole("button", { name: "Save Manager" }));
    await user.clear(description);
    await user.type(description, "Keep this newer edit in memory.");
    resolvePatch?.(new Response(JSON.stringify(makeProfile(2))));
    await screen.findByText("Manager profile saved as version 2.");
    expect(description).toHaveValue("Keep this newer edit in memory.");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "draft could not be saved",
    );
  } finally {
    write.mockRestore();
  }
});

it("keeps every advanced control available and submits from the keyboard", async () => {
  let patchCalls = 0;
  const user = renderEditor(async (_url, init) => {
    if (init?.method === "PATCH") patchCalls += 1;
    return new Response(
      JSON.stringify(makeProfile(init?.method === "PATCH" ? 2 : 1)),
    );
  });

  await screen.findByLabelText("How should your Manager work with you?");
  await user.click(screen.getByText("More options"));
  for (const label of [
    "Manager name",
    "Title",
    "Persona",
    "Traits",
    "Communication style",
    "Problem-solving style",
    "Collaboration style",
    "Uncertainty handling",
    "Initiative",
    "Explanation style",
    "Language preferences",
    "Working habits",
    "Global working preferences",
  ]) {
    expect(screen.getByLabelText(label)).toBeInTheDocument();
  }
  await user.click(screen.getByLabelText("Manager name"));
  await user.keyboard("{Enter}");
  await waitFor(() => expect(patchCalls).toBe(1));
});

it("retains the editable draft when the same profile is closed and reopened", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) =>
      new Response(
        JSON.stringify(
          init?.method === "PATCH" ? makeProfile(2) : makeProfile(),
        ),
      ),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const renderPage = () =>
    render(
      <QueryClientProvider client={client}>
        <ApiContext.Provider value={api}>
          <ManagerPersonality />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
  const first = renderPage();
  const user = userEvent.setup({ delay: null });
  const description = await screen.findByLabelText(
    "How should your Manager work with you?",
  );
  await user.clear(description);
  await user.type(
    description,
    "Keep this draft while I review another screen.",
  );
  first.unmount();
  renderPage();

  expect(
    await screen.findByDisplayValue(
      "Keep this draft while I review another screen.",
    ),
  ).toBeInTheDocument();
});
