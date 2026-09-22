import { textDirection, rtlDir } from "./text-direction";

it("reads mostly-Arabic text right to left, even when it names an English file", () => {
  expect(
    textDirection("سأعيد قراءة جدول الأنظمة Systems.pdf ثم أحدّث ملخص العمل."),
  ).toBe("rtl");
  expect(
    textDirection(
      "I'm tracking the tender إنشاء وبناء محطة (Fire-Fighting Services Station).",
    ),
  ).toBe("ltr");
  expect(textDirection("12.50 m3")).toBe("ltr");
  expect(rtlDir("Pad footings")).toBeUndefined();
  expect(rtlDir("الخطوة الجاية")).toBe("rtl");
});
