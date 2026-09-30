import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import { layout } from "../test/layout";
import type { TenderDocument } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const photo: TenderDocument = {
  id: "d1",
  path: "Photos/Site.jpg",
  name: "Site.jpg",
  kind: "image",
  size: 100,
  status: "read",
  note: null,
  page_count: 1,
  group_name: null,
  description: null,
  scans_to_read: 0,
  opened: 0,
  cited: 0,
};

/** jsdom loads no pictures: the photo arrives 2000 × 1000 pixels. */
function loaded(picture: HTMLElement) {
  Object.defineProperties(picture, { naturalWidth: { value: 2000 }, naturalHeight: { value: 1000 } });
  fireEvent.load(picture);
}

describe("A picture in the viewer", () => {
  it("fits the picture to the window without enlarging it, and zooms with the controls and Ctrl + wheel", async () => {
    const { resize } = layout();
    fakeService({ tenders: [tender], documents: [photo] });
    openApp("/tenders/t1/documents?doc=d1&page=1");

    const picture = await screen.findByRole("img", { name: "Site.jpg" });
    expect(screen.getByText("Image")).toBeInTheDocument();
    expect(picture).toHaveStyle({ opacity: "0" }); // not shown until it can be fitted
    loaded(picture);
    const scroller = picture.closest("[tabindex]")!;
    resize(scroller, 1032, 832); // room for 1000 × 800 inside the margin
    const zoom = screen.getByLabelText("Zoom");
    expect(zoom).toHaveTextContent("50%");
    expect(picture).toHaveStyle({ width: "1000px", height: "500px" });

    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(zoom).toHaveTextContent("67%");
    expect(picture).toHaveStyle({ width: "1340px" });
    await userEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(zoom).toHaveTextContent("50%");
    fireEvent.wheel(scroller, { deltaY: -400, ctrlKey: true });
    expect(zoom).toHaveTextContent("136%");
    await userEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(zoom).toHaveTextContent("50%");

    resize(scroller, 3032, 2032);
    expect(zoom).toHaveTextContent("100%"); // a large window shows it at its own size, no larger
  });

  it("moves the picture by dragging it", async () => {
    fakeService({ tenders: [tender], documents: [photo] });
    openApp("/tenders/t1/documents?doc=d1&page=1");
    const picture = await screen.findByRole("img", { name: "Site.jpg" });
    loaded(picture);
    const scroller = picture.closest<HTMLElement>("[tabindex]")!;

    fireEvent.pointerDown(scroller, { button: 2, clientX: 300, clientY: 200 }); // the right button doesn't drag
    fireEvent.pointerMove(scroller, { clientX: 100, clientY: 100 });
    expect([scroller.scrollLeft, scroller.scrollTop]).toEqual([0, 0]);

    fireEvent.pointerDown(scroller, { button: 0, clientX: 300, clientY: 200 });
    fireEvent.pointerMove(scroller, { clientX: 260, clientY: 150 });
    expect([scroller.scrollLeft, scroller.scrollTop]).toEqual([40, 50]);
    fireEvent.pointerUp(scroller);
    fireEvent.pointerMove(scroller, { clientX: 0, clientY: 0 });
    expect([scroller.scrollLeft, scroller.scrollTop]).toEqual([40, 50]);
  });

  it("says when the picture can't be shown", async () => {
    fakeService({ tenders: [tender], documents: [photo] });
    openApp("/tenders/t1/documents?doc=d1&page=1");

    fireEvent.error(await screen.findByRole("img", { name: "Site.jpg" }));
    expect(await screen.findByText("Quantix couldn’t show this image. Open the original.")).toBeInTheDocument();
  });
});
