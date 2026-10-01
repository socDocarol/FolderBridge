import { test, expect } from "@playwright/test";

test("small desktop fits the workspace and pages through results without scrolling", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await page.goto("/");
  await expect(page.getByLabel("Source folder", { exact: true })).toBeVisible();
  await expect(page.locator("tbody tr").first()).toBeVisible();
  const overflow = await page.evaluate(() => ({
    vertical: document.documentElement.scrollHeight > innerHeight,
    horizontal: document.documentElement.scrollWidth > innerWidth,
  }));
  expect(overflow).toEqual({ vertical: false, horizontal: false });
  await expect(
    page.getByRole("button", { name: "Next page" }),
  ).toBeInViewport();
  const first = await page.locator(".file-name").first().textContent();
  await page.getByRole("button", { name: "Next page" }).click();
  await expect(page.locator(".file-name").first()).not.toHaveText(first!);
  const pageStart = await page.locator(".file-name").first().textContent();
  await page.getByLabel("Select all files on this page").check();
  await expect(
    page.getByRole("button", { name: "Review", exact: true }),
  ).toBeInViewport();
  await expect
    .poll(() =>
      page
        .locator(".file-table")
        .evaluate((el) => el.scrollHeight <= el.clientHeight + 1),
    )
    .toBe(true);
  await expect(page.locator(".file-name").first()).toHaveText(pageStart!);
  await page.screenshot({ path: "qa/compact-800.png" });
  await page.getByRole("button", { name: "Scan options", exact: true }).click();
  await expect(page.getByLabel("Include owners")).toBeVisible();
  await expect(page.getByLabel("Exclude types")).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Storage", exact: true }).click();
  await page.getByRole("button", { name: "Statistics", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Export statistics" }),
  ).toBeInViewport();
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("button", { name: "Comparisons", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Open comparison 1", exact: true }),
  ).toBeInViewport();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollHeight <= innerHeight,
    ),
  ).toBe(true);
});

test("result rows offer Explorer and details expose both folder locations", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByLabel("Search files", { exact: true }).fill("Q3 forecast");
  await page
    .getByRole("button", { name: "Show Budgets/Q3 forecast.xlsx in Explorer" })
    .click();
  await expect(
    page.getByText("Explorer is available in the desktop app."),
  ).toBeVisible();
  await page.locator(".file-name").first().click();
  await expect(
    page
      .getByRole("dialog")
      .getByRole("button", { name: "Show source in Explorer" }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog")
      .getByRole("button", { name: "Open destination folder" }),
  ).toBeVisible();
});
