import { expect, type Page } from "@playwright/test";

/** Starts a tender as the engineer does, and waits until Quantix shows it. Its address is returned. */
export async function startTender(page: Page, name: string) {
  await page.goto("/new");
  await page.getByLabel("Tender name").fill(name);
  await page.getByRole("button", { name: "Start tender" }).click();
  await expect(page.getByRole("main").getByText(name, { exact: true })).toBeVisible();
  await expect(page.getByRole("banner").getByText(name, { exact: true })).toBeVisible();
  return page.url();
}

/** A synthetic text PDF for the tests, one page per list of lines, in Helvetica: no tender document is committed. */
export function pdf(name: string, pages: string[][]) {
  const escape = (text: string) => text.replace(/[\\()]/g, (c) => `\\${c}`);
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    `<< /Type /Pages /Kids [${pages.map((_, i) => `${4 + i * 2} 0 R`).join(" ")}] /Count ${pages.length} >>`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  for (const [i, lines] of pages.entries()) {
    const text = `BT /F1 12 Tf 16 TL 72 770 Td ${lines.map((l) => `(${escape(l)}) Tj T*`).join(" ")} ET`;
    objects.push(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R >> >> /Contents ${5 + i * 2} 0 R >>`,
      `<< /Length ${text.length} >>\nstream\n${text}\nendstream`,
    );
  }
  let body = "%PDF-1.4\n";
  const offsets = objects.map((object, i) => {
    const offset = body.length;
    body += `${i + 1} 0 obj\n${object}\nendobj\n`;
    return offset;
  });
  const xref = body.length;
  body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  body += offsets.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("");
  body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return { name, mimeType: "application/pdf", buffer: Buffer.from(body, "latin1") };
}
