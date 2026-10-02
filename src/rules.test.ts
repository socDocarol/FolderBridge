import { expect, test } from "vitest";
import { emptyRules, withRules } from "./rules";
import type { ScanOptions } from "./types";

test("legacy saved extensions are normalized so Include can remove an exclusion", () => {
  const options = withRules({
    source: "source",
    destination: "target",
    compareBothWays: false,
    verifyContents: true,
    collectOwners: false,
    excludedExtensions: ["BAK", " .bak ", "PDF"],
    rules: { ...emptyRules, reviewExtensions: ["ACCDB", ".accdb"] },
  });
  expect(options.excludedExtensions).toEqual([".bak", ".pdf"]);
  expect(options.excludedExtensions.filter((type) => type !== ".bak")).toEqual([
    ".pdf",
  ]);
  expect(options.rules.reviewExtensions).toEqual([".accdb"]);
});

test("old saved pairs gain empty rules without enabling silent exclusions", () => {
  const options = withRules({
    source: "source",
    destination: "target",
    verifyContents: true,
    collectOwners: false,
    excludedExtensions: [],
  } as unknown as ScanOptions);
  expect(options.rules.reviewAbove).toBeNull();
  expect(options.rules.excludedPaths).toEqual([]);
  expect(options.compareBothWays).toBe(false);
});
