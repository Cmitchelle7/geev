import { describe, it, expect } from "vitest";

const ALLOWED_FOLDERS = new Set(["uploads", "avatars", "posts"]);

function validateFolder(folder: string): boolean {
  return ALLOWED_FOLDERS.has(folder);
}

describe("upload folder validation", () => {
  it("accepts allowed folders", () => {
    expect(validateFolder("uploads")).toBe(true);
    expect(validateFolder("avatars")).toBe(true);
    expect(validateFolder("posts")).toBe(true);
  });

  it("rejects path traversal", () => {
    expect(validateFolder("../../avatars")).toBe(false);
    expect(validateFolder("../uploads")).toBe(false);
  });

  it("rejects invalid characters", () => {
    expect(validateFolder("folder/name")).toBe(false);
    expect(validateFolder("folder name")).toBe(false);
  });
});