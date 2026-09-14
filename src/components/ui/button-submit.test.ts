import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

// Base UI's Button renders type="button" unless told otherwise, so a shadcn
// <Button> that is meant to submit its form silently does nothing. Any Button
// in a file with a form must declare its type or handle its own click.
const root = join(__dirname, "..", "..");

function sources(folder: string): string[] {
  return readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    if (statSync(path).isDirectory())
      return name === "ui" || name === "bindings" ? [] : sources(path);
    return /\.tsx$/.test(name) && !/\.test\.tsx$/.test(name) ? [path] : [];
  });
}

const buttonTag =
  /<Button\b(?:[^>"'{]|"[^"]*"|'[^']*'|\{(?:[^{}]|\{[^{}]*\})*\})*>/g;

it("never leaves a form's Button without a type or click handler", () => {
  const offenders: string[] = [];
  for (const file of sources(root)) {
    const text = readFileSync(file, "utf8");
    if (!text.includes("<form")) continue;
    for (const match of text.matchAll(buttonTag)) {
      const tag = match[0];
      if (!/\btype=|\bonClick=|\brender=/.test(tag)) {
        const line = text.slice(0, match.index).split("\n").length;
        offenders.push(`${relative(root, file)}:${line}`);
      }
    }
  }
  expect(offenders).toEqual([]);
});
