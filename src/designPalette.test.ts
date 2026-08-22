import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
// @ts-expect-error Vitest runs this test in Node; renderer types intentionally exclude Node declarations.
import { readFileSync } from "node:fs";
// @ts-expect-error Vitest runs this test in Node; renderer types intentionally exclude Node declarations.
import { resolve } from "node:path";

const designSystemCss = readFileSync(
  resolve("src/quantixDesignSystem.css"),
  "utf8",
);
const workspaceCss = readFileSync(resolve("src/ManagerWorkspace.css"), "utf8");
const contextCss = readFileSync(
  resolve("src/WorkspaceContextPanel.css"),
  "utf8",
);
const titleBarCss = readFileSync(resolve("src/WindowTitleBar.css"), "utf8");

const root = document.documentElement;
let style: HTMLStyleElement;

function cssRule(source: string, selector: string) {
  const start = source.indexOf(`${selector} {`);
  const openingBrace = source.indexOf("{", start);
  if (start < 0 || openingBrace < 0) {
    throw new Error(`Missing palette rule for ${selector}`);
  }

  let depth = 0;
  for (let index = openingBrace; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }

  throw new Error(`Unclosed palette rule for ${selector}`);
}

function token(name: string) {
  return getComputedStyle(root).getPropertyValue(name).trim();
}

function ruleToken(source: string, selector: string, name: string) {
  const rule = cssRule(source, selector);
  const declaration = rule
    .slice(rule.indexOf("{") + 1, -1)
    .split(";")
    .map((candidate) => candidate.trim())
    .find((candidate) => candidate.startsWith(`${name}:`));

  if (!declaration) {
    throw new Error(`Missing ${name} in palette rule for ${selector}`);
  }

  return declaration.slice(declaration.indexOf(":") + 1).trim();
}

function ruleStyle(selector: string) {
  const sheet = style.sheet;
  if (!sheet) throw new Error("Palette stylesheet was not attached");
  const rule = Array.from(sheet.cssRules).find(
    (candidate): candidate is CSSStyleRule =>
      candidate instanceof CSSStyleRule && candidate.selectorText === selector,
  );
  if (!rule) {
    throw new Error(
      `Missing rendered rule for ${selector}; found ${Array.from(sheet.cssRules)
        .filter(
          (candidate): candidate is CSSStyleRule =>
            candidate instanceof CSSStyleRule,
        )
        .map((candidate) => candidate.selectorText)
        .join(", ")}`,
    );
  }
  return rule.style;
}

describe("Quantix design palette", () => {
  beforeAll(() => {
    style = document.createElement("style");
    style.textContent = [
      cssRule(designSystemCss, ":root"),
      cssRule(designSystemCss, 'html[data-quantix-appearance="dark"]'),
      cssRule(workspaceCss, ".manager-workspace__sidebar"),
      cssRule(workspaceCss, ".manager-workspace__main"),
      cssRule(contextCss, ".workspace-context"),
      cssRule(titleBarCss, ".window-title-bar"),
    ].join("\n");
    document.head.append(style);
  });

  afterAll(() => {
    style.remove();
  });

  afterEach(() => {
    root.removeAttribute("data-quantix-appearance");
    document.body.replaceChildren();
  });

  it("uses a very-light baby-blue workspace field in light mode", () => {
    root.dataset.quantixAppearance = "light";

    const canvas = token("--qx-canvas");
    const surface = token("--qx-surface");

    expect(canvas).toMatch(/^#[0-9a-f]{6}$/i);
    expect(surface).toMatch(/^#[0-9a-f]{6}$/i);
    expect(canvas.toLowerCase()).not.toBe("#ffffff");
    expect(surface.toLowerCase()).not.toBe("#ffffff");
    expect(Number.parseInt(canvas.slice(5, 7), 16)).toBeGreaterThan(
      Number.parseInt(canvas.slice(3, 5), 16),
    );
    expect(Number.parseInt(surface.slice(5, 7), 16)).toBeGreaterThan(
      Number.parseInt(surface.slice(3, 5), 16),
    );
    expect(token("--qx-side-panel")).toBe("#eaf6fc");
    expect(token("--qx-side-panel-strong")).toBe("#ddeff8");
  });

  it("uses charcoal workspace surfaces and steel-blue side planes in dark mode", () => {
    root.dataset.quantixAppearance = "dark";

    expect(token("--qx-canvas")).toBe("#273137");
    expect(token("--qx-surface")).toBe("#2b363d");
    expect(token("--qx-side-panel")).toBe("#304b59");
    expect(token("--qx-side-panel-strong")).toBe("#3a5d6d");
    expect(token("--qx-success")).toBe("#9de5bf");
    expect(token("--qx-warning")).toBe("#f3ca72");
    expect(token("--qx-danger")).toBe("#ffaaaa");
    expect(token("--qx-danger-label")).toBe("var(--qx-brand-graphite)");
  });

  it("defines the grayscale identity and accessible blue focus colors", () => {
    root.dataset.quantixAppearance = "light";

    expect(token("--qx-brand-graphite")).toBe("#3f464d");
    expect(token("--qx-brand-silver")).toBe("#9aa3ab");
    expect(token("--qx-brand-white")).toBe("#ffffff");
    expect(token("--qx-focus")).toBe("#397c9d");
  });

  it("keeps system-dark and forced-colors palettes aligned", () => {
    const systemDarkCss = designSystemCss.slice(
      designSystemCss.indexOf("@media (prefers-color-scheme: dark)"),
    );
    const forcedColorsCss = designSystemCss.slice(
      designSystemCss.indexOf("@media (forced-colors: active)"),
    );
    const systemSelector = 'html[data-quantix-appearance="system"]';

    expect(ruleToken(systemDarkCss, systemSelector, "--qx-canvas")).toBe(
      "#273137",
    );
    expect(ruleToken(systemDarkCss, systemSelector, "--qx-side-panel")).toBe(
      "#304b59",
    );
    expect(ruleToken(systemDarkCss, systemSelector, "--qx-danger")).toBe(
      "#ffaaaa",
    );
    expect(ruleToken(forcedColorsCss, systemSelector, "--qx-canvas")).toBe(
      "Canvas",
    );
    expect(ruleToken(forcedColorsCss, systemSelector, "--qx-side-panel")).toBe(
      "Canvas",
    );
    expect(ruleToken(forcedColorsCss, systemSelector, "--qx-danger")).toBe(
      "CanvasText",
    );
  });

  it("does not retain obsolete palette aliases", () => {
    expect(designSystemCss).not.toContain("--qx-canvas-gradient");
    expect(designSystemCss).not.toContain("--qx-title-bar-gradient");
    expect(designSystemCss).not.toContain("--qx-shadow-panel");
  });

  it("uses blended washes for the title bar and workspace planes without borders", () => {
    root.dataset.quantixAppearance = "light";
    document.body.innerHTML = `
      <aside class="manager-workspace__sidebar"></aside>
      <main class="manager-workspace__main"></main>
      <aside class="workspace-context"></aside>
    `;

    const sidebar = document.querySelector<HTMLElement>(
      ".manager-workspace__sidebar",
    );
    const workspace = document.querySelector<HTMLElement>(
      ".manager-workspace__main",
    );
    const context = document.querySelector<HTMLElement>(".workspace-context");

    expect(sidebar).not.toBeNull();
    expect(workspace).not.toBeNull();
    expect(context).not.toBeNull();
    expect(ruleStyle(".manager-workspace__sidebar").background).toMatch(
      /gradient|--qx-wash-sidebar|--workspace-sidebar-wash/,
    );
    expect(ruleStyle(".manager-workspace__main").background).toMatch(
      /gradient|--qx-wash-main|--workspace-main-wash/,
    );
    expect(ruleStyle(".workspace-context").background).toMatch(
      /gradient|--qx-wash-context|--workspace-context-wash/,
    );
    expect(ruleStyle(".window-title-bar").background).toMatch(
      /gradient|--qx-wash-titlebar|--window-title-bar-background/,
    );
    expect(["", "0px"]).toContain(
      ruleStyle(".manager-workspace__sidebar").borderRight,
    );
    expect(["", "0px"]).toContain(ruleStyle(".workspace-context").border);
  });

  it("keeps the overlapping mobile sidebar legible with a dense gradient wash", () => {
    const mobileWorkspaceCss = workspaceCss.slice(
      workspaceCss.indexOf("@media (max-width: 819px)"),
    );
    const mobileSidebarRule = cssRule(
      mobileWorkspaceCss,
      ".manager-workspace__sidebar",
    );

    expect(mobileSidebarRule).toContain("background:");
    expect(mobileSidebarRule).toContain("linear-gradient");
    expect(mobileSidebarRule).not.toContain("var(--qx-surface)");

    const mobileSidebarOffset = workspaceCss.indexOf(
      mobileSidebarRule,
      workspaceCss.indexOf("@media (max-width: 819px)"),
    );
    const cssAfterMobileSidebar = workspaceCss.slice(
      mobileSidebarOffset + mobileSidebarRule.length,
    );
    expect(cssAfterMobileSidebar).not.toMatch(
      /\.manager-workspace__sidebar\s*\{\s*background:\s*var\(--workspace-sidebar-wash\);\s*\}/,
    );

    const contextDrawerRule = cssRule(contextCss, ".workspace-context--drawer");
    expect(contextDrawerRule).toContain("background:");
    expect(contextDrawerRule).toContain("linear-gradient");
  });
});
