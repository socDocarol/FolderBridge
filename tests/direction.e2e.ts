import { test, expect } from "@playwright/test";

test("one-way default ignores destination extras and switches without rescanning", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await page.goto("/");
  const toggle = page.getByLabel("Compare both ways", { exact: true });
  await expect(toggle).not.toBeChecked();
  await expect(page.locator(".table-footer")).toContainText("of 15 files");
  await expect(page.locator(".comparison-summary")).not.toContainText(
    "Destination only",
  );
  await page
    .getByRole("button", { name: "Select all 15", exact: true })
    .click();
  await expect(
    page
      .getByLabel("Action for selected files")
      .locator("option[value=copy_to_source]"),
  ).toHaveCount(0);
  await toggle.check();
  await expect(
    page.getByRole("button", { name: "Clear selection", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".table-footer")).toContainText("of 18 files");
  await page.getByLabel("Filter by status").selectOption("destination_only");
  await expect(page.locator(".table-footer")).toContainText("of 3 files");
  await page.getByRole("button", { name: "Select all 3", exact: true }).click();
  await expect(
    page
      .getByLabel("Action for selected files")
      .locator("option[value=copy_to_source]"),
  ).toHaveCount(1);
  await toggle.uncheck();
  await expect(page.getByLabel("Filter by status")).toHaveValue("");
  await expect(page.locator(".table-footer")).toContainText("of 15 files");
  await page
    .getByLabel("Search files", { exact: true })
    .fill("Asset inventory");
  await expect(page.locator("tbody tr")).toHaveCount(0);
  await toggle.check();
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "Comparisons", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /^Open comparison / }),
  ).toHaveCount(1);
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= innerHeight &&
        document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test("saved folder pairs retain comparison direction", async ({ page }) => {
  await page.goto("/");
  const toggle = page.getByLabel("Compare both ways", { exact: true });
  await toggle.check();
  await page.getByRole("button", { name: "Save pair", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Both folders");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save pair", exact: true })
    .click();
  await toggle.uncheck();
  await page
    .getByRole("button", { name: "Saved folders", exact: true })
    .click();
  await page.getByRole("button", { name: "Both folders", exact: true }).click();
  await expect(toggle).toBeChecked();
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .last()
    .click();
  await expect(page.locator(".table-footer")).toContainText("of 18 files");
  await toggle.uncheck();
  await expect(toggle).not.toBeChecked();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "Comparisons", exact: true }).click();
  await page
    .getByRole("button", { name: "Open comparison 2", exact: true })
    .click();
  await expect(toggle).toBeChecked();
  await expect(page.locator(".table-footer")).toContainText("of 18 files");
});
