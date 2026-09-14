import { accountNextAction, accountStageLabel } from "./AISetup";

type Account = Parameters<typeof accountStageLabel>[0];

function account(
  protocol: "codex" | "grok_build" | "openai_responses",
  stage: Account["stage"],
): Account {
  return {
    stage,
    access_kind: protocol === "openai_responses" ? "api_key" : "subscription",
    supported: true,
    software: { state: "attention" },
    connection: {
      enabled: true,
      protocol,
      provider_id: protocol === "openai_responses" ? "openai" : protocol,
      auth_type: protocol === "openai_responses" ? "api_key" : "client_login",
      billing: protocol === "openai_responses" ? "metered" : "subscription",
    },
  } as Account;
}

it.each(["codex", "grok_build"] as const)(
  "gives %s original-client preparation guidance after interruption",
  (protocol) => {
    const value = account(protocol, "needs_preparation");
    expect(accountStageLabel(value)).toBe("Needs client repair");
    expect(accountNextAction(value)).toBe("Repair the official client.");
    expect(accountNextAction(account(protocol, "needs_sign_in"))).not.toContain(
      "API key",
    );
    expect(accountStageLabel(account(protocol, "needs_sign_in"))).toBe(
      "Sign in to provider",
    );
  },
);

it("keeps direct API credential guidance distinct", () => {
  expect(
    accountNextAction(account("openai_responses", "needs_credentials")),
  ).toBe("Save an API key.");
  expect(
    accountStageLabel(account("openai_responses", "needs_credentials")),
  ).toBe("Add API key");
});
