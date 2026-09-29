import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Prose, tidy } from "./Prose";

describe("Prose", () => {
  it("lays out points run together in one line, and drops the signature", () => {
    const text = "Status: (1) BOQ — 26 lines approved. (2) Takeoff — nothing measured yet. — Salem";
    expect(tidy(text, "Salem")).toBe("Status:\n\n1. BOQ — 26 lines approved.\n2. Takeoff — nothing measured yet.");
  });

  it("drops the office's record references", () => {
    expect(tidy("Quantix has now sent markups 99d1acc5 back twice.")).toBe("Quantix has now sent markups back twice.");
    expect(tidy("Quantix's open record is markups e8d6e257.")).toBe("Quantix's open record is markups.");
    expect(tidy("Item 12345678 and the cafe decade stay.")).toBe("Item 12345678 and the cafe decade stay.");
  });

  it("leaves ordinary text alone", () => {
    expect(tidy("Priced at 18.50 (see the build-up).", "Salem")).toBe("Priced at 18.50 (see the build-up).");
  });

  it("renders lists and bold, but never raw HTML", () => {
    render(<Prose text={"**Next:**\n\n- Rashid's figures\n- Nora's limits\n\n<img src=x onerror=alert(1)>"} />);
    expect(screen.getByText("Next:").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Rashid's figures", "Nora's limits"]);
    expect(document.querySelector("img")).toBeNull();
  });
});
