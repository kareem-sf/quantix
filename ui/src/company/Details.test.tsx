import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("Company details", () => {
  it("keeps the letterhead every document carries", async () => {
    const service = fakeService();
    openApp("/company");

    await userEvent.type(await screen.findByLabelText("Company name"), "Gulf Builders Co.");
    await userEvent.type(screen.getByLabelText("VAT registration"), "300123");
    await userEvent.type(screen.getByLabelText("Address"), "King Fahd Road, Riyadh");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.company).toMatchObject({
        name: "Gulf Builders Co.",
        vat_number: "300123",
        address: "King Fahd Road, Riyadh",
      }),
    );
    expect(screen.getByText("Saved")).toBeInTheDocument();
    expect(screen.getByText("No logo yet. It goes at the top of every document.")).toBeInTheDocument();
  });
});
