import { render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { ManagerMessage } from "./ManagerMessage";
import userEvent from "@testing-library/user-event";

it("opens every saved engineering result through its addressed record", () => {
  const kinds = ["output", "work_product", "calculation"] as const;
  const sections = [
    "submission?view=documents",
    "manager?view=work-product",
    "manager?view=calculation",
  ];
  render(
    <ManagerMessage
      tenderId="one"
      onSource={() => {}}
      message={{
        id: "saved",
        tender_id: "one",
        role: "manager",
        content: "Work saved.",
        source_ids: [],
        created_at: "",
        result_links: kinds.map((kind, i) => ({
          kind,
          id: `record-${i}`,
          title: `Saved ${kind}`,
          target: `/tenders/one/${sections[i]}&record=record-${i}`,
        })),
      }}
    />,
  );
  kinds.forEach((kind, i) =>
    expect(
      screen.getByRole("link", { name: new RegExp(`Saved ${kind}`) }),
    ).toHaveAttribute("href", `/tenders/one/${sections[i]}&record=record-${i}`),
  );
});

it("keeps long Arabic replies collapsed and excludes system rows from the dialogue", () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      new Response(
        JSON.stringify({ artifact_name: "Scope.pdf", locator: "page 2" }),
      ),
  );
  const longArabic = "راجع نطاق الأعمال للمبنى والرسومات التنفيذية. ".repeat(
    40,
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <div>
          <ManagerMessage
            message={{
              id: "m",
              tender_id: "one",
              role: "manager",
              content: longArabic,
              source_ids: [],
              created_at: "2026-09-09T10:00:00Z",
              result_links: [
                {
                  kind: "plan",
                  id: "plan",
                  title: "Review plan",
                  target: "/tenders/one/work?view=plan-review&record=plan",
                },
              ],
            }}
            tenderId="one"
            onSource={() => {}}
            onArtifact={() => {}}
          />
          <ManagerMessage
            message={{
              id: "system",
              tender_id: "one",
              role: "system",
              content: "Internal run log",
              source_ids: [],
              created_at: "",
              result_links: [],
            }}
            tenderId="one"
            onSource={() => {}}
            onArtifact={() => {}}
          />
        </div>
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  // Replies keep the left-to-right layout; Arabic text orders itself per line.
  expect(document.querySelector(".manager-message")).not.toHaveAttribute("dir");
  expect(
    screen.getByRole("button", { name: "Show full reply" }),
  ).toBeInTheDocument();
  // Plans are approved in the chat card, never through a link to a review screen.
  expect(
    screen.queryByRole("link", { name: /Review plan/ }),
  ).not.toBeInTheDocument();
  expect(screen.queryByText("Internal run log")).not.toBeInTheDocument();
});

it("loads every citation only after opening the source count", async () => {
  const user = userEvent.setup();
  const ids = Array.from({ length: 24 }, (_, i) => `source-${i}`);
  const loaded: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      loaded.push(String(url));
      return new Response(
        JSON.stringify({
          artifact_name: "Drawing.pdf",
          locator: String(url).split("/").at(-1),
        }),
      );
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <ManagerMessage
          message={{
            id: "many",
            tender_id: "one",
            role: "manager",
            content: "The review is ready.",
            source_ids: ids,
            created_at: "",
            result_links: [],
          }}
          tenderId="one"
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(loaded).toHaveLength(0);
  await user.click(screen.getByText("24 sources"));
  expect(
    await screen.findByRole("button", { name: "Drawing.pdf · source-23" }),
  ).toBeVisible();
  expect(loaded).toHaveLength(24);
});

it("asks for the engineer's OK on what the job produced, right under the reply", async () => {
  const approvals = Array.from({ length: 8 }, (_, index) => ({
    kind: "finding" as const,
    id: `f-${index}`,
    title: `Finding ${index + 1}`,
    detail: "From the site-visit notice [619cd37489a5445d885fd34c4ee30d3f].",
    state: "waiting" as const,
    can_reject: true,
  }));
  const posts: Array<{ path: string; body: Record<string, unknown> }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const body = JSON.parse(String(init?.body ?? "{}"));
      posts.push({ path: new URL(String(url)).pathname, body });
      return Response.json({
        ...approvals.find((item) => item.id === body.id),
        state: "accepted",
      });
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <ManagerMessage
          tenderId="one"
          message={{
            id: "m",
            tender_id: "one",
            role: "manager",
            content: "Your review is ready.",
            source_ids: [],
            created_at: "",
            result_links: approvals.map((item) => ({
              kind: "finding" as const,
              id: item.id,
              title: item.title,
              target: `/tenders/one/work?view=decisions&record=${item.id}`,
            })),
            approvals,
          }}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const card = screen.getByRole("region", { name: "Needs your OK" });
  expect(card).toHaveTextContent("8 findings need your OK");
  // No links to a separate review screen, and no raw evidence IDs.
  expect(screen.queryAllByRole("link")).toHaveLength(0);
  expect(card).not.toHaveTextContent(/[0-9a-f]{32}/);
  expect(screen.queryByText("Finding 8")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show all 8" }));
  expect(screen.getByText("Finding 8")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Accept: Finding 1" }));
  expect(posts[0]).toEqual({
    path: "/api/tenders/one/approvals",
    body: { kind: "finding", id: "f-0", decision: "accept", note: "" },
  });
  expect(await screen.findByText("Accepted")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Accept all 7" }));
  // The decisions show at once, and the card says how many are saved so far.
  expect(card).toHaveTextContent("8 findings · 8 accepted");
  await waitFor(() => expect(posts).toHaveLength(8));
  await waitFor(() =>
    expect(within(card).queryByRole("status")).not.toBeInTheDocument(),
  );
});

it("offers the Manager's suggested answers with the recommended one picked", async () => {
  const onAnswer = vi.fn(async () => undefined);
  const user = userEvent.setup();
  render(
    <ManagerMessage
      tenderId="one"
      message={{
        id: "q",
        tender_id: "one",
        role: "manager",
        content: "The BOQ has no rows yet, so nothing can be priced.",
        source_ids: [],
        created_at: "",
        result_links: [],
        approvals: [],
        question: {
          text: "How should I get the quantities?",
          choices: [
            {
              label: "Take off from the drawings first",
              detail: "",
              recommended: false,
            },
            {
              label: "Read the BOQ PDF and build the rows",
              detail: "17 pages · about 10 minutes",
              recommended: true,
            },
            {
              label: "Wait, I'll import an Excel BOQ",
              detail: "",
              recommended: false,
            },
          ],
        },
      }}
      onSource={() => {}}
      onAnswer={onAnswer}
    />,
  );
  const question = screen.getByRole("region", { name: "Question for you" });
  expect(
    screen.getByRole("radio", { name: /Read the BOQ PDF/ }),
  ).toHaveAttribute("aria-checked", "true");
  expect(question).toHaveTextContent("Recommended");
  await user.click(screen.getByRole("button", { name: "Send" }));
  expect(onAnswer).toHaveBeenCalledWith("Read the BOQ PDF and build the rows");

  await user.type(
    screen.getByLabelText("Write another answer"),
    "Use the tender drawings rev C",
  );
  await user.click(screen.getByRole("button", { name: "Send" }));
  expect(onAnswer).toHaveBeenLastCalledWith("Use the tender drawings rev C");
});

it("shows an answered question with the engineer's pick and no Send", () => {
  render(
    <ManagerMessage
      tenderId="one"
      message={{
        id: "q",
        tender_id: "one",
        role: "manager",
        content: "Pick one.",
        source_ids: [],
        created_at: "",
        result_links: [],
        approvals: [],
        question: {
          text: "Who sends the forms?",
          choices: [
            {
              label: "Ask the client for the email",
              detail: "",
              recommended: true,
            },
            { label: "Mohammed Al-Rumaih", detail: "", recommended: false },
          ],
        },
      }}
      answer="Mohammed Al-Rumaih"
      onSource={() => {}}
    />,
  );
  expect(
    screen.getByRole("radio", { name: /Mohammed Al-Rumaih/ }),
  ).toHaveAttribute("aria-checked", "true");
  expect(
    screen.queryByRole("button", { name: "Send" }),
  ).not.toBeInTheDocument();
});
