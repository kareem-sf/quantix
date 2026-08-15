import { readFileSync, writeFileSync } from "node:fs";

function readNormalized(path) {
  return readFileSync(path, "utf8").replace(/\r\n/g, "\n");
}

function replaceOnce(content, before, after, label) {
  const count = content.split(before).length - 1;
  if (count !== 1) {
    throw new Error(`Expected one ${label} match, found ${count}.`);
  }
  return content.replace(before, after);
}

const settingsPath = "src/ApplicationSettings.tsx";
let settings = readNormalized(settingsPath);
settings = replaceOnce(
  settings,
  'connection.provider !== "codex" && canDisconnect',
  "canDisconnect",
  "provider disconnect narrowing",
);
writeFileSync(settingsPath, settings, "utf8");

const managerPath = "src/ManagerWorkspace.tsx";
let manager = readNormalized(managerPath);
const replaceAllToken = '.replaceAll("_", " ")';
const replaceAllCount = manager.split(replaceAllToken).length - 1;
if (replaceAllCount !== 5) {
  throw new Error(
    `Expected five replaceAll compatibility calls, found ${replaceAllCount}.`,
  );
}
manager = manager.split(replaceAllToken).join('.replace(/_/g, " ")');

manager = replaceOnce(
  manager,
  `    const result = await run(() => {
      if (retentionAction === "archive") {
        return archiveTender(tenderId, rationale);
      }
      if (retentionAction === "trash") {
        return trashTender(tenderId, rationale);
      }
      return restoreArchivedTender(tenderId, rationale);
    });`,
  `    const result = await run(async () => {
      if (retentionAction === "archive") {
        return await archiveTender(tenderId, rationale);
      }
      if (retentionAction === "trash") {
        return await trashTender(tenderId, rationale);
      }
      return await restoreArchivedTender(tenderId, rationale);
    });`,
  "retention operation",
);

manager = replaceOnce(
  manager,
  `    const result = await run(() =>
      kind === "restore"
        ? restoreTrashedTender(record.deletion_id, rationale)
        : purgeTrashedTender(
            record.deletion_id,
            rationale,
            permanentDeleteConfirmation,
          ),
    );`,
  `    const result = await run(async () => {
      if (kind === "restore") {
        return await restoreTrashedTender(record.deletion_id, rationale);
      }
      return await purgeTrashedTender(
        record.deletion_id,
        rationale,
        permanentDeleteConfirmation,
      );
    });`,
  "Trash operation",
);
writeFileSync(managerPath, manager, "utf8");

const testPath = "src/ManagerWorkspace.test.tsx";
let test = readNormalized(testPath);
test = replaceOnce(
  test,
  `    expect(
      screen.getByRole("button", { name: "Permanent Delete" }),
    ).toBeDisabled();`,
  `    expect(
      (
        screen.getByRole("button", {
          name: "Permanent Delete",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);`,
  "disabled-button assertion",
);
writeFileSync(testPath, test, "utf8");
