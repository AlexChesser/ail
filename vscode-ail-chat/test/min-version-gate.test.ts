import { describe, it, expect } from "vitest";
import { isAcceptable, MinVersionState } from "../src/min-version-gate";

const baseState: MinVersionState = {
  minVersion: "0.4.0",
  resolvedVersion: "0.4.0",
  meetsMin: true,
  useAnywayActive: false,
};

describe("isAcceptable", () => {
  it("returns true when binary meets the minimum", () => {
    expect(isAcceptable({ ...baseState, meetsMin: true, useAnywayActive: false })).toBe(true);
  });

  it("returns true when override is active even if binary is below minimum", () => {
    expect(
      isAcceptable({
        ...baseState,
        resolvedVersion: "0.3.0",
        meetsMin: false,
        useAnywayActive: true,
      })
    ).toBe(true);
  });

  it("returns false when binary is below minimum and override is not active", () => {
    expect(
      isAcceptable({
        ...baseState,
        resolvedVersion: "0.3.0",
        meetsMin: false,
        useAnywayActive: false,
      })
    ).toBe(false);
  });

  it("returns true when both meetsMin and useAnywayActive are true", () => {
    expect(isAcceptable({ ...baseState, meetsMin: true, useAnywayActive: true })).toBe(true);
  });
});
