export type PreferenceTheme = "light" | "dark" | "system";

export type BridgePreferences = {
  theme: PreferenceTheme;
  reducedMotion: boolean;
  clipboardSync: boolean;
  sidebarCollapsed: boolean;
};

type StoredPreferences = BridgePreferences & { schemaVersion: 1 };
type StorageLike = Pick<Storage, "getItem" | "setItem">;

const STORAGE_KEY = "bridge.preferences.v1";

export const DEFAULT_PREFERENCES: BridgePreferences = {
  theme: "system",
  reducedMotion: false,
  clipboardSync: false,
  sidebarCollapsed: false,
};

function isTheme(value: unknown): value is PreferenceTheme {
  return value === "light" || value === "dark" || value === "system";
}

function browserStorage(): StorageLike | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}

export function loadPreferences(storage: StorageLike | undefined = browserStorage()): BridgePreferences {
  if (!storage) return { ...DEFAULT_PREFERENCES };

  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_PREFERENCES };

    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { ...DEFAULT_PREFERENCES };
    }

    const record = value as Record<string, unknown>;
    if (record.schemaVersion !== 1) return { ...DEFAULT_PREFERENCES };

    return {
      theme: isTheme(record.theme) ? record.theme : DEFAULT_PREFERENCES.theme,
      reducedMotion: typeof record.reducedMotion === "boolean" ? record.reducedMotion : DEFAULT_PREFERENCES.reducedMotion,
      clipboardSync: typeof record.clipboardSync === "boolean" ? record.clipboardSync : DEFAULT_PREFERENCES.clipboardSync,
      sidebarCollapsed: typeof record.sidebarCollapsed === "boolean" ? record.sidebarCollapsed : DEFAULT_PREFERENCES.sidebarCollapsed,
    };
  } catch {
    // Corrupt or inaccessible settings must never prevent the app from opening.
    return { ...DEFAULT_PREFERENCES };
  }
}

export function savePreferences(
  preferences: BridgePreferences,
  storage: StorageLike | undefined = browserStorage(),
): boolean {
  if (!storage) return false;

  const stored: StoredPreferences = {
    schemaVersion: 1,
    theme: isTheme(preferences.theme) ? preferences.theme : DEFAULT_PREFERENCES.theme,
    reducedMotion: Boolean(preferences.reducedMotion),
    clipboardSync: Boolean(preferences.clipboardSync),
    sidebarCollapsed: Boolean(preferences.sidebarCollapsed),
  };

  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(stored));
    return true;
  } catch {
    // Quota/private-mode errors are non-fatal. The current session still works.
    return false;
  }
}
