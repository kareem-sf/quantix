import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import { folderGroup } from "./Documents";
import type { TenderDocument } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

function doc(id: string, path: string, extra: Partial<TenderDocument> = {}): TenderDocument {
  return {
    id,
    path,
    name: path.split("/").pop()!,
    kind: "word",
    size: 100,
    status: "read",
    note: null,
    page_count: 1,
    group_name: null,
    description: null,
    ...extra,
  };
}

describe("folder groups", () => {
  it("uses the folders inside the package, not the package folder itself", () => {
    const all = [doc("a", "Pkg/ITT.docx"), doc("b", "Pkg/Drawings/A-101.pdf")];
    expect(folderGroup("Pkg/ITT.docx", all)).toBe("Documents");
    expect(folderGroup("Pkg/Drawings/A-101.pdf", all)).toBe("Drawings");
    const loose = [doc("c", "Conditions/ITT.docx"), doc("d", "BOQ.xlsx")];
    expect(folderGroup("Conditions/ITT.docx", loose)).toBe("Conditions");
  });
});

describe("Documents", () => {
  it("adds a folder from the overview, keeping its folder paths", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    expect(await screen.findByText("Add the tender package")).toBeInTheDocument();
    const folder = new File(["%PDF"], "Conditions.pdf");
    Object.defineProperty(folder, "webkitRelativePath", { value: "Package/Conditions.pdf" });
    await userEvent.upload(screen.getByLabelText("Tender folder"), folder);

    expect(await screen.findByText("1 of 1 read")).toBeInTheDocument();
    expect(service.state.documents[0].path).toBe("Package/Conditions.pdf");
  });

  it("groups documents, flags problems and shows a page's text", async () => {
    fakeService({
      tenders: [tender],
      documents: [
        doc("d1", "Conditions/ITT.docx"),
        doc("d2", "Drawings/A-101.dwg", { status: "unreadable", note: "CAD drawings can't be read yet." }),
      ],
      pages: { d1: "The bid bond shall be one percent." },
    });
    openApp("/tenders/t1/documents");

    const list = await screen.findByRole("region", { name: "Documents" });
    expect(await within(list).findByText("2 files · 1 can’t be read")).toBeInTheDocument();
    expect(within(list).getByText("Conditions")).toBeInTheDocument();
    expect(within(list).getByText("CAD drawings can't be read yet.")).toBeInTheDocument();

    await userEvent.click(within(list).getByText("ITT.docx"));
    expect(await screen.findByText("The bid bond shall be one percent.")).toBeInTheDocument();
  });

  it("searches every page and opens the hit", async () => {
    fakeService({
      tenders: [tender],
      documents: [doc("d1", "Conditions.pdf", { kind: "pdf", page_count: 46 })],
      hits: [{ document_id: "d1", name: "Conditions.pdf", page: 12, snippet: "the [tender] [security] of one percent" }],
    });
    const router = openApp("/tenders/t1/documents");

    await userEvent.type(await screen.findByLabelText("Search the documents"), "tender security{Enter}");
    const hit = await screen.findByRole("button", { name: /Conditions.pdf · page 12/ });
    expect(within(hit).getByText("security").tagName).toBe("MARK");

    await userEvent.click(hit);
    await waitFor(() => expect(router.state.location.search).toContain("page=12"));
    expect(screen.getByRole("img", { name: "Conditions.pdf, page 12" })).toHaveAttribute(
      "src",
      expect.stringContaining("/api/documents/d1/pages/12/image"),
    );
  });
});
