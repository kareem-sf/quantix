import { render, screen } from "@testing-library/react";
import { isStructured, plainText, RichText } from "./rich-text";

const answer = `**Answer first:** the tender names the two systems in one place only.

| Where | What it states |
|---|---|
| Systems schedule, p.1 | **Fire Alarm — Edwards EST4**; BMS — Johnson Controls |
| BOQ item 6.3.1 (p.12) | 1 no. fire alarm control panel |`;

it("renders AI Markdown as formatted text with a real table, never raw symbols", () => {
  const { container } = render(<RichText text={answer} />);
  expect(screen.getByText("Answer first:").tagName).toBe("STRONG");
  const table = screen.getByRole("table");
  expect(table.closest(".typeset-scroll")).not.toBeNull();
  expect(
    screen.getByRole("columnheader", { name: "Where" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("cell", { name: /Fire Alarm — Edwards EST4/ }),
  ).toBeInTheDocument();
  expect(container.textContent).not.toContain("**");
  expect(container.textContent).not.toContain("|---|");
});

it("keeps inline text on one line", () => {
  const { container } = render(
    <RichText inline text="I'll read **Systems.pdf** next." />,
  );
  expect(container.querySelector("p")).toBeNull();
  expect(screen.getByText("Systems.pdf").tagName).toBe("STRONG");
});

it("turns Markdown into one clean line where formatting cannot show", () => {
  expect(
    plainText(
      "**Fire Alarm** = `Edwards EST4`, see [the schedule](https://x).",
    ),
  ).toBe("Fire Alarm = Edwards EST4, see the schedule.");
  expect(plainText("| Where | What |\n|---|---|\n| p.1 | EST4 |")).toBe(
    "Where · What · p.1 · EST4",
  );
  expect(plainText("## Heading\n- one\n- two")).toBe("Heading one two");
  expect(
    plainText("First sentence here. Second sentence goes on for a while.", 30),
  ).toBe("First sentence here.…");
});

it("tells short notes from structured text", () => {
  expect(isStructured("I'll read the visit schedule next.")).toBe(false);
  expect(isStructured(answer)).toBe(true);
  expect(isStructured("- one\n- two")).toBe(true);
});

it("shows cited evidence IDs as small source links that open the passage", async () => {
  const onSource = vi.fn();
  const { container } = render(
    <RichText
      text="Edwards EST4 [619cd37489a5445d885fd34c4ee30d3f] and the BOQ [881006207b9f479fb39ce9e3f895ab75, 0d40c766cbfc4fbda3a33682f5bd32db]."
      onSource={onSource}
    />,
  );
  expect(container.textContent).not.toMatch(/[0-9a-f]{32}/);
  screen.getByRole("button", { name: "source" }).click();
  expect(onSource).toHaveBeenCalledWith("619cd37489a5445d885fd34c4ee30d3f");
  expect(screen.getByRole("button", { name: "2 sources" })).toBeInTheDocument();
  expect(plainText("Edwards EST4 [619cd37489a5445d885fd34c4ee30d3f].")).toBe(
    "Edwards EST4.",
  );
});

it("drops bare record IDs from sentences but keeps citation links working", () => {
  const onSource = vi.fn();
  const { container } = render(
    <RichText
      text="The table is saved as work product 315e7a0854f44955a8c2b9b44f2f593f for your review [0d40c766cbfc4fbda3a33682f5bd32db, 881006207b9f479fb39ce9e3f895ab75]."
      onSource={onSource}
    />,
  );
  expect(container.textContent).toContain(
    "saved as work product for your review",
  );
  expect(
    plainText("Saved: evidence table `dac27499ac11476bb10d29c140cdb9b5` v3."),
  ).toBe("Saved: evidence table v3.");
  expect(container.textContent).not.toMatch(/[0-9a-f]{32}/);
  screen.getByRole("button", { name: "2 sources" }).click();
  expect(onSource).toHaveBeenCalledWith("0d40c766cbfc4fbda3a33682f5bd32db");
});

it("never shows a bold marker left open by a shortened note", () => {
  const { container } = render(
    <RichText text={"Both systems are named. **Fire alarm** and **BM"} />,
  );
  expect(container.textContent).not.toContain("**");
  expect(screen.getByText("Fire alarm").tagName).toBe("STRONG");
});

it("lays out each Arabic paragraph right to left and each English one left to right", () => {
  const { container } = render(
    <RichText
      text={
        "سأتحقق أولاً من التنبيه في ملخص العمل Systems.pdf.\n\nThen I will check the BOQ."
      }
    />,
  );
  const [arabic, english] = container.querySelectorAll("p");
  expect(arabic).toHaveAttribute("dir", "rtl");
  expect(english).toHaveAttribute("dir", "ltr");
  expect(container.firstElementChild).toHaveAttribute("dir", "rtl");
});
