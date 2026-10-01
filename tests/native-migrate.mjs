// Run against an isolated Windows app profile with CDP on port 9227.
// Creates and changes only disposable files inside the project's ignored qa folder.
import { chromium, expect } from "@playwright/test";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";

await mkdir("qa", { recursive: true });
const fixtures = await mkdtemp(resolve("qa/migrate-"));
const source = join(fixtures, "source");
const destination = join(fixtures, "destination");
await mkdir(join(source, "Archive"), { recursive: true });
await mkdir(destination);
await Promise.all([
  writeFile(join(source, "ready.txt"), "ready"),
  writeFile(join(source, "skipped.bak"), "backup"),
  writeFile(join(source, "reviewed.bin"), Buffer.alloc(2 * 1024 * 1024, 65)),
  writeFile(join(source, "conflict.txt"), "source version"),
  writeFile(join(destination, "conflict.txt"), "target version"),
  writeFile(join(source, "Archive", "held.txt"), "kept here"),
]);
const browser = await chromium.connectOverCDP("http://127.0.0.1:9227");
try {
  const page = browser.contexts()[0].pages()[0];
  const invoke = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.getByRole("button", { name: "Migrate", exact: true }).click();
  await page.getByLabel("Source folder", { exact: true }).fill(source);
  await page
    .getByLabel("Destination folder", { exact: true })
    .fill(destination);
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await page.getByLabel("Skip backup files", { exact: true }).check();
  await page.getByRole("tab", { name: "Files", exact: true }).click();
  await page.getByLabel("Skipped paths").fill("Archive/");
  await page.getByRole("tab", { name: "Sizes", exact: true }).click();
  await page.getByLabel("Review above (MiB)", { exact: true }).fill("1");
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await page.getByRole("button", { name: "Prepare", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Ready 1", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Review 2", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Skipped 2", exact: true }),
  ).toBeVisible();
  const scan = (await invoke("list_scans"))[0];
  assert.equal(scan.options.rules.reviewAbove, 1024 * 1024);
  await page
    .getByRole("button", { name: "Select eligible", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Review migration", exact: true })
    .click();
  const start = page.getByRole("button", {
    name: "Start migration",
    exact: true,
  });
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await expect(start).toBeDisabled();
  await page.getByLabel("Approve 2 flagged files", { exact: true }).check();
  await start.click();
  await expect(page.getByText(/^Migration finished\./)).toBeVisible();
  const operations = (await invoke("list_operations")).filter(
    (o) => o.scanId === scan.id,
  );
  assert.equal(operations.length, 2);
  assert.ok(operations.every((operation) => operation.state === "complete"));
  assert.ok(operations.every((operation) => operation.skipped === 0));
  assert.equal(
    operations.reduce((n, o) => n + o.completed, 0),
    3,
  );
  const copy = operations.find((o) => o.action === "copy_to_destination");
  const keep = operations.find((o) => o.action === "keep_both");
  assert.ok(copy && keep);
  const [kept] = await invoke("get_operation_items", { operationId: keep.id });
  assert.equal(await readFile(kept.targetPath, "utf8"), "source version");
  assert.equal(
    await readFile(join(destination, "conflict.txt"), "utf8"),
    "target version",
  );
  assert.equal(await readFile(join(destination, "ready.txt"), "utf8"), "ready");
  assert.equal(
    (await stat(join(destination, "reviewed.bin"))).size,
    2 * 1024 * 1024,
  );
  await assert.rejects(stat(join(destination, "skipped.bak")));
  await assert.rejects(stat(join(destination, "Archive", "held.txt")));
  assert.equal(
    await readFile(join(source, "conflict.txt"), "utf8"),
    "source version",
  );

  // A mixed migration's Keep both record must still support verified cleanup and restore.
  const preview = await invoke("preview_cleanup", { operationId: keep.id });
  assert.equal(preview.items[0].eligible, true);
  assert.equal(
    (
      await invoke("cleanup_originals", {
        request: {
          operationId: keep.id,
          itemIds: [kept.id],
          handling: "network",
          confirmation: "REMOVE 1",
          sharepointConfirmed: false,
        },
      })
    ).completed,
    1,
  );
  assert.equal(
    (
      await invoke("restore_originals", {
        operationId: keep.id,
        itemIds: [kept.id],
      })
    ).completed,
    1,
  );
  assert.equal(
    await readFile(join(source, "conflict.txt"), "utf8"),
    "source version",
  );
  const report = join(fixtures, "skipped.csv");
  assert.equal(
    await invoke("export_scan", {
      filter: {
        scanId: scan.id,
        search: "",
        status: "",
        migrationState: "skipped",
        extension: "",
        minSize: 0,
        offset: 0,
        limit: 50,
      },
      path: report,
    }),
    2,
  );
  assert.match(await readFile(report, "utf8"), /Migration reason/);
  assert.ok(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= innerHeight &&
        document.documentElement.scrollWidth <= innerWidth,
    ),
  );
  assert.deepEqual(errors, []);
  console.log(
    JSON.stringify(
      {
        passed: true,
        fixtures,
        checks: [
          "native shared Rules and prepared counts",
          "explicit flag approval",
          "mixed copy and Keep both",
          "skipped files untouched",
          "originals preserved",
          "Keep both cleanup and restore",
          "filtered CSV reasons",
          "compact layout",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
