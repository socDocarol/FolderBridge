// Attach to an already-running Windows build launched with WebView2's CDP port.
// Uses new, isolated fixture folders only. Records test history in that app profile.
import { chromium, expect } from "@playwright/test";
import { mkdtemp, mkdir, writeFile, readFile, stat } from "node:fs/promises";
import { resolve, join } from "node:path";
import assert from "node:assert/strict";

await mkdir("qa", { recursive: true });
const fixtures = await mkdtemp(resolve("qa/native-"));
await writeFile("qa/native-latest.json", JSON.stringify({ fixtures }, null, 2));
const source = join(fixtures, "source");
const destination = join(fixtures, "destination");
await Promise.all([mkdir(source), mkdir(destination)]);
await Promise.all([
  writeFile(join(source, "missing, café & plan.txt"), "copy this safely"),
  writeFile(join(source, "duplicate.txt"), "identical content"),
  writeFile(join(destination, "duplicate.txt"), "identical content"),
  writeFile(join(source, "conflict.txt"), "AAAA"),
  writeFile(join(destination, "conflict.txt"), "BBBB"),
  writeFile(join(destination, "recover.txt"), "recover this safely"),
]);
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
try {
  const page = browser.contexts()[0].pages()[0];
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const invoke = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  await page.getByLabel("Source folder", { exact: true }).fill(source);
  await page
    .getByLabel("Destination folder", { exact: true })
    .fill(destination);
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .last()
    .click();
  await expect(page.locator("tbody tr").first()).toBeVisible();
  await expect(page.locator(".table-footer")).toContainText("of 4 files");
  const scans = await invoke("list_scans");
  const scanId = scans[0].id;
  assert.equal(scans[0].state, "complete");
  const filter = {
    scanId,
    search: "",
    status: "",
    extension: "",
    minSize: 0,
    offset: 0,
    limit: 50,
  };
  const { entries } = await invoke("list_entries", { filter });
  assert.deepEqual(
    new Set(entries.map((e) => e.status)),
    new Set(["source_only", "destination_only", "different", "identical"]),
  );
  assert.equal(
    (await invoke("get_analysis", { scanId })).sizeStatistics.length,
    1,
  );
  assert.equal((await invoke("matching_entry_ids", { filter })).length, 4);

  const missingEntry = entries.find(
    (e) => e.relativePath === "missing, café & plan.txt",
  );
  const selected = await invoke("reveal_entry", {
    scanId,
    entryId: missingEntry.id,
    side: "source",
  });
  assert.equal(selected.selectFile, true);
  const missingFolder = await invoke("reveal_entry", {
    scanId,
    entryId: missingEntry.id,
    side: "destination",
  });
  assert.equal(missingFolder.selectFile, false);
  assert.ok(missingFolder.path.endsWith("destination"));
  console.log(
    "Explorer file selection and missing-side folder open succeeded.",
  );

  // Copy through the actual rendered interface and its confirmation dialog.
  await page.getByLabel("Search files", { exact: true }).fill("missing");
  await page.getByRole("button", { name: "Select all 1", exact: true }).click();
  await page.getByRole("button", { name: "Review", exact: true }).click();
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await page.getByRole("button", { name: "Copy files", exact: true }).click();
  await expect(
    page.getByText("completed", { exact: false }).first(),
  ).toBeVisible();
  assert.equal(
    await readFile(join(destination, "missing, café & plan.txt"), "utf8"),
    "copy this safely",
  );

  // Separate reviewed original cleanup through the packaged UI, using only this fixture.
  const copied = (await invoke("list_operations"))[0];
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page
    .getByRole("button", { name: `View operation ${copied.id}` })
    .click();
  await page
    .getByRole("button", { name: "Review cleanup", exact: true })
    .click();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByLabel("Network share", { exact: false }).check();
  await page.getByLabel("Type REMOVE 1 to confirm").fill("REMOVE 1");
  await page
    .getByRole("button", { name: "Move to _ToDelete", exact: true })
    .click();
  await expect(
    page.getByRole("dialog").getByText("held", { exact: true }),
  ).toBeVisible();
  const [heldItem] = await invoke("get_operation_items", {
    operationId: copied.id,
  });
  await assert.rejects(stat(heldItem.originalPath));
  assert.equal(
    await readFile(heldItem.cleanup.holdingPath, "utf8"),
    "copy this safely",
  );
  await page.screenshot({ path: "qa/native-cleanup-held.png" });
  await page
    .getByRole("button", { name: "Review restore", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Restore selected", exact: true })
    .click();
  await expect(
    page.getByRole("dialog").getByText("restored", { exact: true }),
  ).toBeVisible();
  assert.equal(
    await readFile(heldItem.originalPath, "utf8"),
    "copy this safely",
  );
  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .first()
    .click();

  // Exercise the same production IPC used by the remaining reviewed operations.
  const execute = async (name, action) => {
    const request = {
      scanId,
      entryIds: [entries.find((e) => e.relativePath === name).id],
      action,
    };
    assert.equal((await invoke("preview_operation", { request })).eligible, 1);
    return invoke("execute_operation", { request });
  };
  const recovered = await execute("recover.txt", "copy_to_source");
  assert.equal(
    await readFile(join(source, "recover.txt"), "utf8"),
    "recover this safely",
  );
  const conflict = await execute("conflict.txt", "keep_both");
  const conflictItems = await invoke("get_operation_items", {
    operationId: conflict,
  });
  assert.equal(await readFile(conflictItems[0].targetPath, "utf8"), "AAAA");
  assert.equal(
    await readFile(join(destination, "conflict.txt"), "utf8"),
    "BBBB",
  );
  for (const operationId of [recovered, conflict]) {
    const [{ id }] = await invoke("get_operation_items", { operationId });
    const outcome = await invoke("cleanup_originals", {
      request: {
        operationId,
        itemIds: [id],
        handling: "network",
        confirmation: "REMOVE 1",
        sharepointConfirmed: false,
      },
    });
    assert.equal(outcome.completed, 1, JSON.stringify(outcome));
    assert.equal(
      (await invoke("restore_originals", { operationId, itemIds: [id] }))
        .completed,
      1,
    );
  }
  const quarantined = await execute("duplicate.txt", "quarantine_destination");
  await assert.rejects(stat(join(destination, "duplicate.txt")));
  await invoke("restore_operation", { operationId: quarantined });
  assert.equal(
    await readFile(join(destination, "duplicate.txt"), "utf8"),
    "identical content",
  );
  const report = join(fixtures, "inventory.csv");
  assert.equal(await invoke("export_scan", { filter, path: report }), 4);
  assert.match(await readFile(report, "utf8"), /Source SHA-256/);
  await invoke("export_statistics", {
    scanId,
    path: join(fixtures, "sizes.csv"),
  });
  await invoke("export_operation", {
    operationId: quarantined,
    path: join(fixtures, "operation.csv"),
  });
  await page.getByLabel("Search files", { exact: true }).fill("");
  await expect(page.locator(".table-footer")).toContainText("of 4 files");
  assert.ok(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= innerHeight &&
        document.documentElement.scrollWidth <= innerWidth,
    ),
  );
  await page.screenshot({ path: "qa/native-windows.png", fullPage: true });
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify(
      {
        passed: true,
        fixtures,
        checks: [
          "native UI scan and confirmed copy",
          "native UI original cleanup to _ToDelete and restore",
          "recovery and keep-both original cleanup and restore through IPC",
          "production IPC comparison/statistics/bulk selection",
          "Explorer selects existing file and opens missing-side parent",
          "compact native layout without page overflow",
          "recovery and keep both",
          "quarantine and restore",
          "three CSV reports",
          "no JavaScript errors",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
