import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("About", () => {
  it("is the last tab of Settings: Quantix, its place in QS Mind, and its founder", async () => {
    fakeService();
    openApp("/settings");

    await userEvent.click(await screen.findByRole("tab", { name: "About" }));
    expect(screen.getByRole("img", { name: "Quantix" })).toBeInTheDocument();
    const house = screen.getByRole("region", { name: "A QS Mind product" });
    expect(within(house).getByText(/Quantix is a product of/)).toHaveTextContent(
      "QS Mind, the commercial mind of construction",
    );
    expect(within(house).getByText("Stage 2 · Tender")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Kareem Safwat" })).toHaveAttribute("href", "https://kareemsafwat.com");
    expect(screen.getByText(/QS Mind\. All rights reserved\./)).toBeInTheDocument();
  });
});
