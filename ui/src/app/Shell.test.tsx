import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import { windowOf } from "../test/window";

const school = { id: "t1", name: "Al Noor Primary School", due_date: "2026-10-14", created_at: "2026-09-23T08:00:00Z" };
const warehouse = { id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };

const sidebar = () => screen.getByRole("navigation", { name: "Quantix" });
const isFolded = () => within(sidebar()).queryByText("Company") === null; // folded to icons, the Company label goes
const overview = () => screen.findByRole("heading", { name: "Nothing needs you right now" });

function drag(edge: HTMLElement, from: number, ...to: number[]) {
  fireEvent.pointerDown(edge, { button: 0, clientX: from });
  for (const x of to) fireEvent.pointerMove(window, { clientX: x });
  fireEvent.pointerUp(window);
}

describe("The shell's keyboard", () => {
  it("goes back and forward with Alt+Left and Alt+Right", async () => {
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1");
    await overview();

    await userEvent.keyboard("{Control>}4{/Control}");
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    await userEvent.keyboard("{Alt>}{ArrowLeft}{/Alt}");
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    await userEvent.keyboard("{Alt>}{ArrowRight}{/Alt}");
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1/estimate"));
  });

  it("keeps every shortcut on an Arabic keyboard layout", async () => {
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1");
    await overview();

    fireEvent.keyDown(document, { key: "ت", code: "KeyJ", ctrlKey: true });
    expect(await screen.findByRole("complementary", { name: "Team" })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "لا", code: "KeyB", ctrlKey: true });
    expect(isFolded()).toBe(true);
    fireEvent.keyDown(document, { key: "٤", code: "Digit4", ctrlKey: true });
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    fireEvent.keyDown(document, { key: "ظ", code: "Slash", ctrlKey: true });
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("leaves every other key to the page, so Ctrl+C still copies", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await overview();

    // fireEvent says whether the page may still do what the key does
    expect(fireEvent.keyDown(document, { key: "c", code: "KeyC", ctrlKey: true })).toBe(true);
    expect(fireEvent.keyDown(document, { key: "K", code: "KeyK", ctrlKey: true, shiftKey: true })).toBe(true);
    expect(fireEvent.keyDown(document, { key: "8", code: "Digit8", ctrlKey: true })).toBe(true); // seven screens
    expect(fireEvent.keyDown(document, { key: "ArrowLeft", code: "ArrowLeft", altKey: true, ctrlKey: true })).toBe(true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    expect(fireEvent.keyDown(document, { key: "k", code: "KeyK", ctrlKey: true })).toBe(false);
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument();
  });

  it("opens no tender's screen by number on a firm screen before any tender was opened", async () => {
    fakeService({ tenders: [school] });
    const router = openApp("/library");
    await screen.findByRole("heading", { name: "Company library" });

    expect(fireEvent.keyDown(document, { key: "2", code: "Digit2", ctrlKey: true })).toBe(true);
    expect(router.state.location.pathname).toBe("/library");
  });

  it("follows the last tender opened on a firm screen", async () => {
    localStorage.setItem("quantix.place", JSON.stringify({ last: "t9", screens: { t9: "/tenders/t9/estimate" } }));
    fakeService({ tenders: [school, warehouse] });
    const router = openApp("/library");

    expect(await within(sidebar()).findByRole("button", { name: /^Riyadh Warehouse, no due date/ })).toBeInTheDocument();
    await userEvent.keyboard("{Control>}2{/Control}");
    expect(router.state.location.pathname).toBe("/tenders/t9/documents");
  });
});

describe("The sidebar and the team", () => {
  it("reopens as the engineer left them: the sidebar folded and the team open", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await overview();
    await userEvent.keyboard("{Control>}b{/Control}");
    await userEvent.keyboard("{Control>}j{/Control}");
    await screen.findByRole("complementary", { name: "Team" });

    cleanup();
    openApp("/tenders/t1/estimate");
    expect(await screen.findByRole("complementary", { name: "Team" })).toBeInTheDocument();
    expect(isFolded()).toBe(true);
  });

  it("drags the sidebar's edge within its bounds and keeps the width", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    const edge = await screen.findByRole("separator", { name: "Sidebar width" });
    expect(sidebar()).toHaveStyle({ width: "224px" });

    fireEvent.pointerDown(edge, { button: 0, clientX: 224 });
    fireEvent.pointerMove(window, { clientX: 600 });
    expect(sidebar()).toHaveStyle({ width: "360px" });
    fireEvent.pointerMove(window, { clientX: 160 });
    expect(sidebar()).toHaveStyle({ width: "184px" });
    fireEvent.pointerMove(window, { clientX: 290 });
    fireEvent.pointerUp(window);
    expect(sidebar()).toHaveStyle({ width: "290px" });
    expect(localStorage.getItem("quantix.sidebarWidth")).toBe("290");

    fireEvent.doubleClick(edge);
    expect(sidebar()).toHaveStyle({ width: "224px" });
  });

  it("folds the sidebar when its edge is dragged well past its narrowest, and unfolds it at the width it had", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");

    drag(await screen.findByRole("separator", { name: "Sidebar width" }), 224, 300);
    drag(screen.getByRole("separator", { name: "Sidebar width" }), 300, 100);
    expect(isFolded()).toBe(true);
    expect(sidebar()).toHaveStyle({ width: "52px" });
    expect(screen.queryByRole("separator", { name: "Sidebar width" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}b{/Control}");
    expect(sidebar()).toHaveStyle({ width: "300px" });
  });

  it("drags the team's edge within its bounds, and a double-click brings back the usual width", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await overview();
    await userEvent.keyboard("{Control>}j{/Control}");
    const panel = await screen.findByRole("complementary", { name: "Team" });
    const edge = within(panel).getByRole("separator", { name: "Team panel width" });
    expect(panel).toHaveStyle({ width: "400px" });

    fireEvent.pointerDown(edge, { button: 0, clientX: 1000 });
    fireEvent.pointerMove(window, { clientX: 400 });
    expect(panel).toHaveStyle({ width: "720px" });
    fireEvent.pointerMove(window, { clientX: 1300 });
    expect(panel).toHaveStyle({ width: "320px" });
    fireEvent.pointerMove(window, { clientX: 950 });
    fireEvent.pointerUp(window);
    expect(panel).toHaveStyle({ width: "450px" });
    expect(localStorage.getItem("quantix.teamWidth")).toBe("450");

    fireEvent.doubleClick(edge);
    expect(panel).toHaveStyle({ width: "400px" });
  });

  it("opens at the widths the engineer left", async () => {
    localStorage.setItem("quantix.sidebarWidth", "300");
    localStorage.setItem("quantix.teamWidth", "520");
    localStorage.setItem("quantix.team", "true");
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");

    expect(await screen.findByRole("complementary", { name: "Team" })).toHaveStyle({ width: "520px" });
    expect(sidebar()).toHaveStyle({ width: "300px" });
  });

  it("gives the drawing the room on Takeoff, and Ctrl+B and Ctrl+J open the sidebar and the team there", async () => {
    localStorage.setItem("quantix.team", "true");
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1");
    await screen.findByRole("complementary", { name: "Team" });

    await userEvent.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("heading", { name: "Takeoff" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Team" })).not.toBeInTheDocument();
    expect(isFolded()).toBe(true);

    await userEvent.keyboard("{Control>}b{/Control}");
    expect(isFolded()).toBe(false);
    drag(screen.getByRole("separator", { name: "Sidebar width" }), 224, 60); // dragged shut, it folds again
    expect(isFolded()).toBe(true);
    await userEvent.keyboard("{Control>}j{/Control}");
    expect(await screen.findByRole("complementary", { name: "Team" })).toBeInTheDocument();

    await userEvent.keyboard("{Control>}4{/Control}");
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    expect(isFolded()).toBe(false); // off the drawing, the sidebar is as the engineer left it
  });
});

describe("A narrower window", () => {
  it("shows the sidebar's icons in a narrow window and opens it over the screen", async () => {
    windowOf(900);
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1");
    await overview();
    expect(isFolded()).toBe(true);
    expect(within(sidebar()).getByRole("link", { name: "Estimate" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Sidebar (Ctrl+B)" }));
    expect(isFolded()).toBe(false);
    drag(within(sidebar()).getByRole("separator", { name: "Sidebar width" }), 224, 60);
    expect(sidebar()).toHaveStyle({ width: "184px" }); // dragged narrow, the drawer stays open
    await userEvent.click(within(sidebar()).getByRole("link", { name: "Estimate" }));
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    expect(isFolded()).toBe(true); // it closes once it has taken the engineer somewhere

    await userEvent.keyboard("{Control>}b{/Control}");
    expect(isFolded()).toBe(false);
    await userEvent.click(sidebar().previousElementSibling!); // a click on the screen beside it
    expect(isFolded()).toBe(true);
  });

  it("makes the sidebar a drawer in a small window, and gives the team the whole window", async () => {
    const view = windowOf(500);
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await overview();
    expect(screen.queryByRole("navigation", { name: "Quantix" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}b{/Control}");
    expect(isFolded()).toBe(false);
    view.resize(900); // the drawer closes when the window changes
    expect(isFolded()).toBe(true);
    view.resize(500);
    expect(screen.queryByRole("navigation", { name: "Quantix" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}j{/Control}");
    const panel = await screen.findByRole("complementary", { name: "Team" });
    expect(within(panel).queryByRole("separator")).not.toBeInTheDocument();
  });
});
