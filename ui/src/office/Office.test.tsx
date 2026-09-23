import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { Decision, Message, Staff } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const rania: Staff = {
  id: "s1",
  name: "Rania Farouk",
  role: "Tender Manager",
  is_manager: true,
  status: "active",
  now: "Reviewing Omar's result",
  profile: { experience_years: 19, opinions: "Distrusts quantities nobody has measured." },
};
const omar: Staff = {
  id: "s2",
  name: "Omar Haddad",
  role: "Quantity Surveyor",
  is_manager: false,
  status: "active",
  now: null,
  profile: { experience_years: 11, working_style: "Measures twice.", opinions: "Never trusts a BOQ quantity." },
};
const at = "2026-09-23T10:42:00Z";
const said = (id: number, sender: string, channel: string, text: string, kind = "message"): Message => ({
  id,
  sender,
  channel,
  kind,
  text,
  created_at: at,
});
const question: Decision = {
  id: "q1",
  raised_by: "s1",
  title: "Tender security wording",
  text: "The conditions ask for a 1% tender security. Shall we price the bond at 1%?",
  options: ["Yes, 1%", "Ask the client first"],
  status: "waiting",
  answer: null,
  created_at: at,
};
const ready = { office_mode: "engineer" as const, office_ai: { connection_id: "c1", model: "m" } };

describe("Overview and decisions", () => {
  it("asks for the office's AI before the team can start", async () => {
    fakeService({ tenders: [tender] });
    openApp("/tenders/t1");
    expect(await screen.findByText(/so the team can start work/)).toBeInTheDocument();
  });

  it("lists what needs the engineer and takes an answer", async () => {
    const service = fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      decisions: [question],
      messages: [said(1, "s1", "s1", "Omar found the tender security clause.")],
    });
    const router = openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "1 decision needs you" })).toBeInTheDocument();
    expect(await screen.findByText("Omar found the tender security clause.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: /Tender security wording/ }));

    await userEvent.click(await screen.findByRole("radio", { name: /Yes, 1%/ }));
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    expect(service.state.decisions[0]).toMatchObject({ status: "answered", answer: "Yes, 1%" });
  });

  it("sends the engineer's request to the Manager", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania] });
    openApp("/tenders/t1");

    await userEvent.type(await screen.findByLabelText("Message the office"), "Review the package{Enter}");
    await waitFor(() => expect(service.state.messages.at(-1)).toMatchObject({ channel: "s1", text: "Review the package" }));
  });
});

describe("Office", () => {
  it("shows the team room as the team wrote it, and who each person is", async () => {
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      officeState: "working",
      messages: [
        said(1, "engineer", "team", "Check the tender security, please."),
        said(2, "s1", "team", "Rania asked Omar to find the tender security clause", "task"),
        said(3, "s2", "team", "The bond wording is unusual; it may be unconditional.", "concern"),
      ],
    });
    openApp("/tenders/t1/office");

    const room = await screen.findByRole("region", { name: "Conversation" });
    expect(await within(room).findByText("Check the tender security, please.")).toBeInTheDocument();
    expect(within(room).getByText("Rania asked Omar to find the tender security clause")).toBeInTheDocument();
    expect(within(room).getByText("· raised a concern")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Quantix" })).getByText("Reviewing Omar's result")).toBeInTheDocument();

    await userEvent.click(within(room).getByRole("button", { name: "About Omar Haddad" }));
    const profile = await screen.findByRole("complementary", { name: "Omar Haddad" });
    expect(within(profile).getByText("Never trusts a BOQ quantity.")).toBeInTheDocument();
    expect(within(profile).getByText("Idle")).toBeInTheDocument();
  });

  it("messages a person directly and can stop the office", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania, omar], officeState: "working" });
    openApp("/tenders/t1/office");

    await userEvent.click(await screen.findByRole("button", { name: "Omar Haddad" }));
    await userEvent.type(screen.getByLabelText("Message"), "Use the BOQ units{Enter}");
    await waitFor(() => expect(service.state.messages.at(-1)).toMatchObject({ channel: "s2", text: "Use the BOQ units" }));

    await userEvent.click(screen.getByRole("button", { name: "Stop the office" }));
    await waitFor(() => expect(service.state.officeState).toBe("paused"));
  });
});
