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

  it("shows the firm's logo, replaces it and removes it", async () => {
    const service = fakeService({ company: { name: "Gulf Builders Co.", address: "", cr_number: "", vat_number: "", has_logo: true } });
    openApp("/company");

    expect(await screen.findByLabelText("Company name")).toHaveValue("Gulf Builders Co.");
    expect(screen.getByRole("img", { name: "Company logo" })).toHaveAttribute("src", "/api/company/logo?v=0");
    await userEvent.upload(screen.getByLabelText("Logo file"), new File(["png"], "logo.png", { type: "image/png" }));
    await waitFor(() => expect(screen.getByRole("img", { name: "Company logo" })).toHaveAttribute("src", "/api/company/logo?v=1"));

    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(await screen.findByText("No logo yet. It goes at the top of every document.")).toBeInTheDocument();
    expect(service.state.company.has_logo).toBe(false);
    expect(screen.getByText("Choose a logo")).toBeInTheDocument();
  });

  it("says why the logo or the details couldn't be saved", async () => {
    fakeService({ fail: { "/company/logo": "Choose a PNG or JPEG image.", "/company": "The address is too long for the title block." } });
    openApp("/company");

    await userEvent.upload(await screen.findByLabelText("Logo file"), new File(["png"], "logo.png", { type: "image/png" }));
    expect(await screen.findByText("Choose a PNG or JPEG image.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("The address is too long for the title block.")).toBeInTheDocument();
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
  });
});
