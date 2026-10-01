import { describe, expect, it } from "vitest";
import { bytes, isEligible, readablePath } from "./format";
import type { Entry } from "./types";

describe("file action eligibility", () => {
  it("never permits cleanup for unverified matches", () => {
    const entry = {
      status: "unverified",
      sourceHash: null,
      destinationHash: null,
    } as Entry;
    expect(isEligible(entry, "quarantine_destination")).toBe(false);
    expect(
      isEligible({ ...entry, status: "identical" }, "quarantine_destination"),
    ).toBe(false);
    expect(
      isEligible(
        {
          ...entry,
          status: "identical",
          sourceHash: "abc",
          destinationHash: "abc",
        },
        "quarantine_destination",
      ),
    ).toBe(true);
  });
  it("formats absent sizes separately from empty files", () => {
    expect(bytes(null)).toBe("—");
    expect(bytes(0)).toBe("0 B");
  });
  it("shows readable Windows UNC paths", () => {
    expect(readablePath("\\\\?\\UNC\\server\\share")).toBe("\\\\server\\share");
  });
});
