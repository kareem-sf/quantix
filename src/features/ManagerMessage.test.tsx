import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { ManagerMessage } from "./ManagerMessage";
import userEvent from "@testing-library/user-event";

it("opens every saved engineering result through its addressed record", () => {
  const kinds = [
    "requirement",
    "boq_item",
    "work_product",
    "calculation",
  ] as const;
  const sections = [
    "submission?view=requirements",
    "estimate?view=boq",
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
  expect(document.querySelector(".manager-message")).toHaveAttribute(
    "dir",
    "auto",
  );
  expect(
    screen.getByRole("button", { name: "Show full reply" }),
  ).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /Review plan/ })).toHaveAttribute(
    "href",
    "/tenders/one/work?view=plan-review&record=plan",
  );
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

it("groups many findings without filling the chat while preserving every canonical link", async () => {
  const links = Array.from({ length: 8 }, (_, index) => ({
    kind: "finding" as const,
    id: `f-${index}`,
    title: `Finding ${index + 1}`,
    target: `/tenders/one/work?view=decisions&record=f-${index}`,
  }));
  const user = userEvent.setup();
  render(
    <ManagerMessage
      tenderId="one"
      message={{
        id: "m",
        tender_id: "one",
        role: "manager",
        content: "Your review is ready.",
        source_ids: [],
        created_at: "",
        result_links: links,
      }}
      onSource={() => {}}
    />,
  );
  expect(screen.getAllByRole("link")).toHaveLength(1);
  expect(screen.getByRole("link", { name: /8 findings/ })).toHaveAttribute(
    "href",
    links[0].target,
  );
  expect(screen.queryByText("Finding 8")).not.toBeInTheDocument();
  await user.click(screen.getByText("Show individual findings"));
  expect(screen.getByRole("link", { name: /Finding 8/ })).toHaveAttribute(
    "href",
    links[7].target,
  );
});
