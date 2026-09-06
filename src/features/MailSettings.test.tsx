import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { MailSettings } from "./MailSettings";

function setup(ready = true, error = false) {
  let settings: Schema<"MailSettings"> = {
    smtp_host: "smtp.example.com",
    smtp_port: 465,
    smtp_security: "ssl",
    smtp_username: "engineer",
    from_address: "tenders@example.com",
    imap_host: "imap.example.com",
    imap_port: 993,
    imap_username: "engineer",
    imap_mailbox: "INBOX",
    smtp_ready: ready,
    imap_ready: ready,
    detail: "",
  };
  const writes: { path: string; body: Record<string, unknown> }[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method !== "GET") {
        const body = JSON.parse(String(init?.body));
        writes.push({ path: String(url), body });
        if (error)
          return new Response(
            JSON.stringify({ detail: "Credential storage is unavailable." }),
            { status: 400 },
          );
        if (String(url).endsWith("/sync"))
          return new Response(
            JSON.stringify({
              checked: 30,
              matched: 2,
              skipped: 28,
              more_available: true,
              warnings: ["One reply exceeds the import size limit."],
            }),
          );
        const {
          smtp_password: _smtp,
          imap_password: _imap,
          ...publicFields
        } = body;
        settings = { ...settings, ...publicFields };
      }
      return new Response(JSON.stringify(settings));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <MailSettings />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes };
}

it("stores write-only mail passwords and clears them after saving", async () => {
  const { user, writes } = setup();
  expect(
    await screen.findByText("SMTP configured · connection not tested"),
  ).toBeInTheDocument();
  await fill(user, screen.getByLabelText("SMTP password"), "smtp-secret");
  await fill(user, screen.getByLabelText("IMAP password"), "imap-secret");
  await user.selectOptions(screen.getByLabelText("SMTP security"), "starttls");
  await user.click(screen.getByRole("button", { name: "Save mail settings" }));
  await waitFor(() =>
    expect(screen.getByLabelText("SMTP password")).toHaveValue(""),
  );
  expect(screen.getByLabelText("IMAP password")).toHaveValue("");
  expect(writes[0].body).toMatchObject({
    smtp_password: "smtp-secret",
    imap_password: "imap-secret",
    smtp_security: "starttls",
  });
  expect(writes[0].body).not.toHaveProperty("smtp_ready");
  expect(writes[0].body).not.toHaveProperty("api_key");
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Mail settings saved",
  );
});

it("leaves stored passwords untouched when fields are blank and reports save failure", async () => {
  const { user, writes } = setup(true, true);
  await user.click(
    await screen.findByRole("button", { name: "Save mail settings" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Credential storage is unavailable.",
  );
  expect(writes[0].body).not.toHaveProperty("smtp_password");
  expect(writes[0].body).not.toHaveProperty("imap_password");
});

it("checks the read-only mailbox only on click and shows bounded results", async () => {
  const { user, writes } = setup();
  const sync = await screen.findByRole("button", {
    name: "Check supplier replies",
  });
  expect(writes).toHaveLength(0);
  await user.click(sync);
  expect(
    await screen.findByText(/Checked 30 messages · 2 replies registered/),
  ).toBeInTheDocument();
  expect(screen.getByText(/More messages are available/)).toBeInTheDocument();
  expect(
    screen.getByText("One reply exceeds the import size limit."),
  ).toBeInTheDocument();
  expect(writes).toEqual([
    { path: "http://localhost/api/mail/sync", body: { max_messages: 30 } },
  ]);
});

it("does not enable inbox access without configured credentials", async () => {
  setup(false);
  expect(
    await screen.findByRole("button", { name: "Check supplier replies" }),
  ).toBeDisabled();
});

async function fill(
  user: ReturnType<typeof userEvent.setup>,
  input: HTMLElement,
  value: string,
) {
  await user.click(input);
  await user.paste(value);
}
