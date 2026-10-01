import { test, expect } from "@playwright/test";

test("shared rules prepare a mixed migration and require approval for flagged files", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await page.goto("/");
  await page.getByRole("button", { name: "Migrate", exact: true }).click();
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await page.getByLabel("Find or add file type").fill(".pdf");
  await page.getByLabel("Rule for .pdf", { exact: true }).selectOption("skip");
  await page.getByRole("tab", { name: "Files", exact: true }).click();
  await page.getByLabel("Skipped paths").fill("Budgets/");
  await page.getByRole("tab", { name: "Sizes", exact: true }).click();
  await page.getByLabel("Review above (MiB)", { exact: true }).fill("1");
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await expect(page.locator("tbody tr")).toHaveCount(0);
  await page.getByRole("button", { name: "Prepare", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Ready 3", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Review 2", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Skipped 13", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Review 2", exact: true }).click();
  await expect(page.locator("tbody tr")).toHaveCount(2);
  await page.getByRole("button", { name: "Select all 2", exact: true }).click();
  await page
    .getByRole("button", { name: "Review migration", exact: true })
    .click();
  const start = page.getByRole("button", {
    name: "Start migration",
    exact: true,
  });
  await expect(start).toBeDisabled();
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await expect(start).toBeDisabled();
  await page.getByLabel("Approve 2 flagged files", { exact: true }).check();
  await start.click();
  await expect(
    page.getByRole("button", { name: "Cancel", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "History", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "View operation 1", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "View operation 2", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .first()
    .click();
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await expect(page.locator("tbody tr").first()).toContainText("Excluded");
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= innerHeight &&
        document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test("saved pairs and historical comparisons retain their own rules", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await page.getByLabel("Skip backup files", { exact: true }).check();
  await page.getByRole("tab", { name: "Sizes", exact: true }).click();
  await page.getByLabel("Review above (MiB)", { exact: true }).fill("10");
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await page.getByRole("button", { name: "Save pair", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Migration rules");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save pair", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Compare", exact: true })
    .last()
    .click();
  await expect(
    page.getByRole("button", { name: "Rules", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await page.getByRole("button", { name: "Reset rules", exact: true }).click();
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await page
    .getByRole("button", { name: "Saved folders", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Migration rules", exact: true })
    .click();
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await expect(
    page.getByLabel("Skip backup files", { exact: true }),
  ).toBeChecked();
  await page.getByRole("tab", { name: "Sizes", exact: true }).click();
  await expect(
    page.getByLabel("Review above (MiB)", { exact: true }),
  ).toHaveValue("10");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "Comparisons", exact: true }).click();
  await page
    .getByRole("button", { name: "Open comparison 1", exact: true })
    .click();
  await page.getByRole("button", { name: "Rules", exact: true }).click();
  await expect(
    page.getByLabel("Skip backup files", { exact: true }),
  ).not.toBeChecked();
});

test("individual file exclusions invalidate the plan and skipped files cannot be selected for migration", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await page.goto("/");
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await page.locator(".file-name").first().click();
  await page
    .getByRole("button", { name: "Skip this file", exact: true })
    .click();
  await expect(page.getByLabel("Skipped paths")).toContainText(
    "Budgets/Q3 forecast.xlsx",
  );
  await page.getByRole("button", { name: "Apply rules", exact: true }).click();
  await page.getByRole("button", { name: "Migrate", exact: true }).click();
  await page.getByRole("button", { name: "Prepare", exact: true }).click();
  await page.getByRole("button", { name: /Skipped \d+/, exact: true }).click();
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await expect(
    page.getByLabel("Select Budgets/Q3 forecast.xlsx", { exact: true }),
  ).toBeDisabled();
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= innerHeight &&
        document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});
