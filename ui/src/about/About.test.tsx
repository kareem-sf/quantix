import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("About Quantix", () => {
  it("says Quantix is a QS Mind product, where it sits in the house, and credits the founder", async () => {
    fakeService();
    openApp("/about");

    expect(await screen.findByRole("img", { name: "Quantix by QS Mind" })).toBeInTheDocument();
    const house = screen.getByRole("region", { name: "A QS Mind product" });
    expect(within(house).getByText(/Quantix is a product of/)).toHaveTextContent(
      "QS Mind, the commercial mind of construction",
    );
    expect(within(house).getByText("Stage 2 · Tender")).toBeInTheDocument();
    expect(within(house).getByText("Tawreed")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Founded & developed by Kareem Safwat/ })).toHaveAttribute(
      "href",
      "https://kareemsafwat.com",
    );
    expect(screen.getByText(/QS Mind\. All rights reserved\./)).toBeInTheDocument();
  });
});
