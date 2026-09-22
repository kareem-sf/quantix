import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../../api";
import { ApprovalDetail } from "./ApprovalDetail";

it("shows one finding in full with its sources and decides it there", async () => {
  const user = userEvent.setup();
  let state = "waiting";
  const posts: unknown[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = new URL(String(url)).pathname;
      if (init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        posts.push(body);
        state = body.decision === "accept" ? "accepted" : "rejected";
        return Response.json({
          kind: "finding",
          id: "f1",
          title: "Visit dates conflict",
          state,
        });
      }
      if (path.includes("/evidence/"))
        return Response.json({
          artifact_name: "Visit notice.pdf",
          locator: "page 1",
        });
      return Response.json({
        kind: "finding",
        id: "f1",
        title: "Visit dates conflict",
        detail:
          "The notice sets 19/08/2026 [619cd37489a5445d885fd34c4ee30d3f] while the gate pass prints 06/09/2026, so the client must confirm the date before anyone travels to site.",
        state,
        can_reject: true,
        facts: ["Risk"],
        source_ids: ["619cd37489a5445d885fd34c4ee30d3f"],
      });
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <ApprovalDetail
          tenderId="one"
          recordKey="finding:f1"
          onSource={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByRole("heading", { name: "Visit dates conflict" }),
  ).toBeInTheDocument();
  expect(screen.getByText("Risk").parentElement).toHaveTextContent(
    "Finding · Risk",
  );
  // The whole text is there, not a two-line preview, and IDs become source links.
  expect(
    screen.getByText(
      /client must confirm the date before anyone travels to site/,
    ),
  ).toBeInTheDocument();
  expect(document.body.textContent).not.toMatch(/[0-9a-f]{32}/);
  expect(
    await screen.findByRole("button", { name: /Visit notice.pdf · page 1/ }),
  ).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Accept" }));
  expect(posts).toEqual([
    { kind: "finding", id: "f1", decision: "accept", note: "" },
  ]);
  expect(await screen.findByText("Accepted")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Accept" }),
  ).not.toBeInTheDocument();

  // A slip can be put right from the same place.
  await user.click(screen.getByRole("button", { name: "Reject instead" }));
  expect(posts.at(-1)).toEqual({
    kind: "finding",
    id: "f1",
    decision: "reject",
    note: "",
  });
  expect(await screen.findByText("Rejected")).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Accept instead" }),
  ).toBeInTheDocument();
});
