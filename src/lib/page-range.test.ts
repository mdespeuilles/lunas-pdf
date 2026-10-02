import { describe, expect, it } from "vitest";
import { formatSize, parseRange, passwordStrength } from "./page-range";

describe("export : plages, mots de passe, tailles", () => {
  it("plages comme côté Rust", () => {
    expect(parseRange("1-3, 5", 10)).toEqual([0, 1, 2, 4]);
    expect(parseRange("8-", 10)).toEqual([7, 8, 9]);
    expect(parseRange("-2", 10)).toEqual([0, 1]);
    expect(parseRange("3,3,1", 10)).toEqual([2, 0]);
    expect(parseRange("0", 10)).toBeNull();
    expect(parseRange("4-2", 10)).toBeNull();
    expect(parseRange("11", 10)).toBeNull();
    expect(parseRange("a", 10)).toBeNull();
    expect(parseRange(" ", 10)).toBeNull();
  });
  it("solidité et tailles", () => {
    expect(passwordStrength("abc")).toBe(0);
    expect(passwordStrength("abcdefgh")).toBe(1);
    expect(passwordStrength("Abcdefgh12!")).toBe(4);
    expect(formatSize(1_887_437, "fr")).toBe("1,8 Mo");
    expect(formatSize(512, "fr")).toBe("512 o");
  });
});
