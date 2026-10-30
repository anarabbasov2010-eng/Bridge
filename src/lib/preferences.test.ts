import { describe, expect, it } from "vitest";
import {
  DEFAULT_PREFERENCES,
  loadPreferences,
  savePreferences,
  type BridgePreferences,
} from "./preferences";

function memoryStorage(initial?: string) {
  let value = initial ?? null;
  return {
    getItem: (_key: string) => value,
    setItem: (_key: string, next: string) => { value = next; },
    inspect: () => value,
  };
}

describe("Bridge preferences", () => {
  it("returns safe defaults when storage is empty", () => {
    expect(loadPreferences(memoryStorage())).toEqual(DEFAULT_PREFERENCES);
  });

  it("round-trips the supported settings", () => {
    const storage = memoryStorage();
    const expected: BridgePreferences = {
      theme: "dark",
      reducedMotion: true,
      clipboardSync: false,
      sidebarCollapsed: true,
    };
    expect(savePreferences(expected, storage)).toBe(true);
    expect(loadPreferences(storage)).toEqual(expected);
  });

  it("falls back safely when the JSON is corrupt", () => {
    expect(loadPreferences(memoryStorage("{broken"))).toEqual(DEFAULT_PREFERENCES);
  });

  it("does not trust unknown schema versions or invalid fields", () => {
    const storage = memoryStorage(JSON.stringify({
      schemaVersion: 99,
      theme: "neon",
      reducedMotion: "yes",
      clipboardSync: true,
      sidebarCollapsed: false,
    }));
    expect(loadPreferences(storage)).toEqual(DEFAULT_PREFERENCES);
  });

  it("sanitizes individual fields from a current schema", () => {
    const storage = memoryStorage(JSON.stringify({
      schemaVersion: 1,
      theme: "neon",
      reducedMotion: true,
      clipboardSync: "yes",
      sidebarCollapsed: false,
    }));
    expect(loadPreferences(storage)).toEqual({
      theme: "system",
      reducedMotion: true,
      clipboardSync: false,
      sidebarCollapsed: false,
    });
  });

  it("does not throw when storage writes fail", () => {
    const storage = {
      getItem: () => null,
      setItem: () => { throw new Error("quota exceeded"); },
    };
    expect(savePreferences(DEFAULT_PREFERENCES, storage)).toBe(false);
  });
});
