import { locatorLabel } from "./locator";

it("names a passage's place in words", () => {
  expect(locatorLabel("page:12")).toBe("page 12");
  expect(locatorLabel("sheet:BOQ/row:14")).toBe("BOQ, row 14");
  expect(locatorLabel("sheet:BOQ/cell:E12")).toBe("BOQ, cell E12");
  expect(locatorLabel("sheet:BOQ/range:A1:E40")).toBe("BOQ, A1:E40");
  expect(locatorLabel("paragraph:3")).toBe("paragraph 3");
  expect(locatorLabel("reply:abc/characters:1-6000")).toBe("supplier reply");
  expect(locatorLabel("row 3")).toBe("row 3");
  expect(locatorLabel(null)).toBe("");
});
