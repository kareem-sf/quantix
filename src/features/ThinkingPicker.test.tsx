import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { ThinkingPicker } from "./ThinkingPicker";

const option = (
  value: string | null,
  label: string,
): Schema<"ThinkingOption"> => ({
  value,
  label,
  detail: `${label} detail`,
  off: false,
});

function thinking(current: string): Schema<"TenderThinking"> {
  const options = [
    option("low", "Low"),
    option("medium", "Medium"),
    option("high", "High"),
    option(null, "Default"),
  ];
  return {
    connection_id: "codex",
    model_id: "gpt-5.3-codex-spark",
    current: options.find((item) => item.value === current) ?? null,
    options,
    busy: false,
  };
}

function renderPicker(busy = false) {
  let saved = thinking("medium");
  const posts: unknown[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (String(url).endsWith("/ai-thinking") && init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        posts.push(body);
        saved = thinking(body.reasoning);
      }
      return new Response(JSON.stringify(saved));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <ThinkingPicker tenderId="one" busy={busy} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return posts;
}

it("sets how much the AI thinks from the prompt box", async () => {
  const user = userEvent.setup();
  const posts = renderPicker();

  await user.click(
    await screen.findByRole("button", { name: /Thinking: Medium/ }),
  );
  await user.click(await screen.findByRole("menuitemradio", { name: /Low/ }));

  await waitFor(() =>
    expect(posts).toEqual([{ reasoning: "low", engineer_confirmed: true }]),
  );
  expect(
    await screen.findByRole("button", { name: /Thinking: Low/ }),
  ).toBeInTheDocument();
});

it("keeps the level while Tender work is running", async () => {
  const user = userEvent.setup();
  const posts = renderPicker(true);

  await user.click(
    await screen.findByRole("button", { name: /Thinking: Medium/ }),
  );
  expect(
    await screen.findByText("Finish or stop the current work to change this."),
  ).toBeInTheDocument();
  await user.click(await screen.findByRole("menuitemradio", { name: /High/ }));
  expect(posts).toEqual([]);
});
