import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { MailReplyCheck } from "./MailReplyCheck";
function setup(ready = true) {
  const writes: { path: string; body: unknown }[] = [],
    settings = vi.fn();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        writes.push({
          path: new URL(String(url)).pathname,
          body: JSON.parse(String(init.body)),
        });
        return Response.json({
          checked: 30,
          matched: 2,
          skipped: 28,
          more_available: true,
          warnings: ["One message was too large."],
        });
      }
      return Response.json({ imap_ready: ready, smtp_ready: ready });
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <MailReplyCheck onSettings={settings} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes, settings };
}
it("checks a bounded mailbox only on the engineer's click", async () => {
  const { user, writes } = setup();
  const button = screen.getByRole("button", { name: "Check replies" });
  await waitFor(() => expect(button).toBeEnabled());
  expect(writes).toEqual([]);
  await user.click(button);
  await screen.findByText(
    "Checked 30 messages · 2 replies registered · 28 skipped",
  );
  expect(screen.getByText(/More messages are available/)).toBeInTheDocument();
  expect(writes).toEqual([
    { path: "/api/mail/sync", body: { max_messages: 30 } },
  ]);
});
it("links missing mail configuration back through the operational origin callback", async () => {
  const { user, settings, writes } = setup(false);
  await user.click(
    await screen.findByRole("button", { name: "Set up supplier mail" }),
  );
  expect(settings).toHaveBeenCalledOnce();
  expect(writes).toEqual([]);
  expect(screen.getByRole("button", { name: "Check replies" })).toBeDisabled();
});
