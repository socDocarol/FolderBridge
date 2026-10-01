import { test, expect } from "@playwright/test";

test("copy history reviews paths, gates cleanup, and restores a held original", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await page.goto("/");
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await page
    .locator(".file-name")
    .filter({ hasText: "Q3 forecast.xlsx" })
    .click();
  await page.getByRole("button", { name: "Select file", exact: true }).click();
  await page.getByRole("button", { name: "Review", exact: true }).click();
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await page.getByRole("button", { name: "Copy files", exact: true }).click();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "View operation 1" }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByText("Example source/Budgets/Q3 forecast.xlsx"),
  ).toBeVisible();
  await expect(
    dialog.getByText("Example destination/Budgets/Q3 forecast.xlsx"),
  ).toBeVisible();
  await expect(
    dialog.getByRole("button", {
      name: "Show copy Budgets/Q3 forecast.xlsx in Explorer",
    }),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "Review cleanup" }).click();
  await expect(
    dialog.getByText("1 eligible copied originals selected."),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "Continue" }).click();
  expect(
    await dialog.evaluate((el) => el.scrollHeight <= el.clientHeight + 1),
  ).toBe(true);
  await expect(
    dialog.getByRole("button", { name: "Move to _ToDelete" }),
  ).toBeDisabled();
  await expect(dialog.getByText("Network folder detected.")).toBeVisible();
  await dialog.getByLabel("Type REMOVE 1 to confirm").fill("REMOVE 1");
  await dialog.getByRole("button", { name: "Move to _ToDelete" }).click();
  await expect(dialog.getByText("held", { exact: true })).toBeVisible();
  await expect(
    dialog.getByText("_ToDelete/1/Budgets/Q3 forecast.xlsx"),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "Review restore" }).click();
  await dialog.getByRole("button", { name: "Restore selected" }).click();
  await expect(dialog.getByText("restored", { exact: true })).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollHeight <= innerHeight,
    ),
  ).toBe(true);
});

test("SharePoint cleanup requires the sync acknowledgement and fits a small window", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await page.goto("/");
  await page
    .getByLabel("Search files", { exact: true })
    .fill("Asset inventory");
  await page
    .getByLabel("Select Data/Asset inventory.accdb", { exact: true })
    .check();
  await page
    .getByLabel("Action for selected files")
    .selectOption("copy_to_source");
  await page.getByRole("button", { name: "Review", exact: true }).click();
  await page.getByLabel("I reviewed the folders and selected action.").check();
  await page
    .getByRole("button", { name: "Recover files", exact: true })
    .click();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "View operation 1" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Review cleanup" }).click();
  await dialog.getByRole("button", { name: "Continue" }).click();
  await dialog.getByLabel("Synced SharePoint", { exact: false }).check();
  await dialog.getByLabel("Type REMOVE 1 to confirm").fill("REMOVE 1");
  const remove = dialog.getByRole("button", {
    name: "Remove synced originals",
  });
  await expect(remove).toBeDisabled();
  await dialog.getByLabel("I checked OneDrive sync.", { exact: false }).check();
  await expect(remove).toBeEnabled();
  expect(
    await dialog.evaluate((el) => el.scrollHeight <= el.clientHeight + 1),
  ).toBe(true);
  await page.screenshot({ path: "qa/cleanup-confirm-small.png" });
  await dialog.getByRole("button", { name: "Back", exact: true }).click();
  await dialog.getByRole("button", { name: "Clear", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Continue" })).toBeDisabled();
});
