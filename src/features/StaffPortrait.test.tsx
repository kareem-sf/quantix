import { createHash } from "node:crypto";
import { fireEvent, render, screen } from "@testing-library/react";
import {
  NOTIONISTS_RECIPE,
  PORTRAIT_RENDER_SIZE,
  StaffPortrait,
  renderPortraitSvg,
} from "./StaffPortrait";

const validPortrait = {
  style: "notionists-v1",
  seed: "staff-001",
};

function decodeSvg(dataUri: string) {
  const comma = dataUri.indexOf(",");
  return decodeURIComponent(dataUri.slice(comma + 1));
}

it("renders a local illustrated image at the pinned internal size", () => {
  render(
    <StaffPortrait portrait={validPortrait} name="Lina Haddad" size={72} />,
  );

  const image = screen.getByRole("img", {
    name: "Illustrated portrait of Lina Haddad",
  });
  const source = image.getAttribute("src");

  expect(source).toMatch(/^data:image\/svg\+xml/);
  expect(source).not.toMatch(/^https?:/);
  expect(source).toBeTruthy();
  expect(decodeSvg(source ?? "")).toMatch(
    new RegExp(`<svg[^>]+width="${PORTRAIT_RENDER_SIZE}"`),
  );
  expect(image).toHaveAttribute("width", String(PORTRAIT_RENDER_SIZE));
  expect(image).toHaveAttribute("height", String(PORTRAIT_RENDER_SIZE));
  expect(image).toHaveAttribute("data-portrait-style", "notionists-v1");
  const frame = document.querySelector<HTMLElement>(".staff-portrait__frame");
  expect(frame).toBeTruthy();
  if (!frame) throw new Error("portrait frame was not rendered");
  expect(frame.style.getPropertyValue("--staff-portrait-size")).toBe("72px");
});

it("keeps the local portrait stable when the display name and theme change", () => {
  const { rerender } = render(
    <StaffPortrait portrait={validPortrait} name="Lina Haddad" />,
  );
  const firstImage = screen.getByRole("img", {
    name: "Illustrated portrait of Lina Haddad",
  });
  const firstSource = firstImage.getAttribute("src");

  document.documentElement.dataset.theme = "dark";
  rerender(<StaffPortrait portrait={validPortrait} name="Lina Renamed" />);

  const secondImage = screen.getByRole("img", {
    name: "Illustrated portrait of Lina Renamed",
  });
  expect(secondImage.getAttribute("src")).toBe(firstSource);
  expect(secondImage).toBe(firstImage);
});

it("gives different local server seeds different illustrations", () => {
  const firstSvg = renderPortraitSvg(validPortrait);
  const secondSvg = renderPortraitSvg({ ...validPortrait, seed: "staff-002" });

  expect(firstSvg).toBeTruthy();
  expect(secondSvg).toBeTruthy();
  expect(secondSvg).not.toBe(firstSvg);
});

it("pins a deterministic recipe hash for notionists-v1", () => {
  const svg = renderPortraitSvg({
    style: NOTIONISTS_RECIPE.styleId,
    seed: "quantix-manager-24",
  });

  expect(svg).toBeTruthy();
  expect(
    createHash("sha256")
      .update(svg ?? "")
      .digest("hex"),
  ).toBe("8a5311ee65907e892beee75b831f51abe5928812fdc7c8a30c88170b4fb2fc55");
});

it("uses a truthful accessible fallback for unknown styles", () => {
  render(
    <StaffPortrait
      portrait={{ style: "remote-style", seed: "staff-003" }}
      name="Samir Fares"
    />,
  );

  expect(
    screen.getByRole("img", { name: "Portrait unavailable for Samir Fares" }),
  ).toHaveTextContent("SF");
  expect(screen.getByRole("status")).toHaveTextContent(
    "Portrait style is unavailable locally. This does not affect staff work.",
  );
  expect(
    screen.queryByRole("button", { name: "Try portrait again" }),
  ).toBeNull();
  expect(
    screen.queryByRole("img", { name: /Illustrated portrait/ }),
  ).toBeNull();
});

it("falls back for an invalid server seed without accepting a URL", () => {
  render(
    <StaffPortrait
      portrait={{ style: "notionists-v1", seed: "" }}
      name="نور سليم"
    />,
  );

  expect(
    screen.getByRole("img", { name: "Portrait unavailable for نور سليم" }),
  ).toHaveTextContent("نس");
  expect(screen.getByRole("status")).toHaveTextContent(
    "Portrait seed is invalid. This does not affect staff work.",
  );
  expect(document.querySelector("img")).toBeNull();
});

it("handles an image load failure and retries the local image", () => {
  render(<StaffPortrait portrait={validPortrait} name="Nour Selim" />);

  const image = screen.getByRole("img", {
    name: "Illustrated portrait of Nour Selim",
  });
  fireEvent.error(image);

  expect(
    screen.getByRole("img", {
      name: "Portrait unavailable for Nour Selim",
    }),
  ).toHaveTextContent("NS");
  expect(screen.getByRole("status")).toHaveTextContent(
    "Portrait failed to load locally. This does not affect staff work.",
  );

  fireEvent.click(screen.getByRole("button", { name: "Try portrait again" }));
  expect(
    screen.getByRole("img", {
      name: "Illustrated portrait of Nour Selim",
    }),
  ).toHaveAttribute("src", expect.stringMatching(/^data:image\/svg\+xml/));
});

it("supports decorative portraits and empty names without leaking an image label", () => {
  render(
    <StaffPortrait
      portrait={{ style: "notionists-v1", seed: "" }}
      name=""
      decorative
    />,
  );

  const fallback = document.querySelector(".staff-portrait__fallback");
  expect(fallback).toBeTruthy();
  if (!fallback) throw new Error("portrait fallback was not rendered");
  expect(fallback).toHaveAttribute("aria-hidden", "true");
  expect(fallback).toHaveTextContent("?");
  expect(screen.queryByRole("img")).toBeNull();
});
