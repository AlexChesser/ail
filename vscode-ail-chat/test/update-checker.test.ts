import { describe, it, expect } from "vitest";
import { compareVersions } from "../src/update-checker";

describe("compareVersions", () => {
  it("returns 0 for equal versions", () => {
    expect(compareVersions("1.2.3", "1.2.3")).toBe(0);
    expect(compareVersions("0.0.0", "0.0.0")).toBe(0);
  });

  it("orders by major first", () => {
    expect(compareVersions("2.0.0", "1.99.99")).toBeGreaterThan(0);
    expect(compareVersions("1.0.0", "2.0.0")).toBeLessThan(0);
  });

  it("orders by minor when majors are equal", () => {
    expect(compareVersions("1.2.0", "1.1.99")).toBeGreaterThan(0);
    expect(compareVersions("1.1.0", "1.2.0")).toBeLessThan(0);
  });

  it("orders by patch when major+minor are equal", () => {
    expect(compareVersions("0.4.10", "0.4.9")).toBeGreaterThan(0);
    expect(compareVersions("0.4.0", "0.4.1")).toBeLessThan(0);
  });

  it("treats missing components as 0", () => {
    expect(compareVersions("1", "1.0.0")).toBe(0);
    expect(compareVersions("1.2", "1.2.0")).toBe(0);
  });

  it("handles non-numeric components as 0", () => {
    expect(compareVersions("1.x.0", "1.0.0")).toBe(0);
  });
});
