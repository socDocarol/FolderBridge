import { test, expect } from "@playwright/test";

test("filter, inspect, review and run an operation with explicit confirmation", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("main", { name: "Compare", exact: true }),
  ).toBeVisible();
  await expect(
    page.locator(".file-name").filter({ hasText: "Team directory.xlsx" }),
  ).toBeVisible();
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page
    .locator(".file-name")
    .filter({ hasText: "Q3 forecast.xlsx" })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "Select file", exact: true }).click();
  await page.getByRole("button", { name: "Review", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Copy files", exact: true }),
  ).toBeDisabled();
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await page.getByRole("button", { name: "Copy files", exact: true }).click();
  await expect(
    page
      .getByRole("region", { name: "Job status" })
      .or(page.getByRole("status", { name: "Job status" })),
  ).toBeVisible();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "View operation 1" }).click();
  await expect(
    page.getByText("Simulated operation. No real files changed."),
  ).toBeVisible();
});

test("folder pairs, quick scan, and keyboard dialog dismissal work", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Save pair", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Name", exact: true })
    .fill("Review pair");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save pair", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Saved folders", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Review pair", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Review pair", exact: true }).click();
  await page.getByRole("button", { name: "Rules" }).click();
  await page.getByRole("tab", { name: "Scan", exact: true }).click();
  await page.getByLabel("Verify contents", { exact: false }).uncheck();
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .last()
    .click();
  await expect(page.getByText("Quick comparison")).toBeVisible();
  await page
    .locator(".file-name")
    .filter({ hasText: "Team directory.xlsx" })
    .click();
  await expect(
    page.getByRole("dialog").getByText("Not verified", { exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("desktop, mobile and reduced-motion layouts stay usable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 960, height: 680 });
  await page.goto("/");
  await expect(page.locator("tbody tr").first()).toBeVisible();
  await page.screenshot({ path: "qa/desktop.png", fullPage: true });
  await page.getByRole("button", { name: "Storage", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Storage by file type" }),
  ).toBeVisible();
  await page.screenshot({ path: "qa/insights.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .first()
    .click();
  await expect(page.getByLabel("Source folder", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Browse destination folder" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBeTruthy();
  await page.screenshot({ path: "qa/mobile.png", fullPage: true });
});

test("visual libraries are actively exercised in the development-only trial page", async ({
  page,
}) => {
  await page.goto("/experiments/");
  await expect(
    page.getByRole("heading", { name: "Interaction trials" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Toggle selection" }).click();
  await expect(page.locator("canvas")).toHaveCount(2);
  await page.locator("canvas").first().hover();
  await page.screenshot({ path: "qa/interaction-trials.png", fullPage: true });
});
