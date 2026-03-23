/**
 * Storage Setup
 *
 * Initializes IndexedDB-backed stores for settings, API keys,
 * sessions, and custom providers. Provides the AppStorage instance
 * that Pi SDK components depend on.
 */

import {
  AppStorage,
  CustomProvidersStore,
  IndexedDBStorageBackend,
  ProviderKeysStore,
  SessionsStore,
  SettingsStore,
  setAppStorage,
} from "@mariozechner/pi-web-ui";

export interface StorageLayer {
  settings: SettingsStore;
  providerKeys: ProviderKeysStore;
  sessions: SessionsStore;
  customProviders: CustomProvidersStore;
  storage: AppStorage;
}

export function createStorageLayer(): StorageLayer {
  const settings = new SettingsStore();
  const providerKeys = new ProviderKeysStore();
  const sessions = new SessionsStore();
  const customProviders = new CustomProvidersStore();

  const backend = new IndexedDBStorageBackend({
    dbName: "canopy",
    version: 1,
    stores: [
      settings.getConfig(),
      SessionsStore.getMetadataConfig(),
      providerKeys.getConfig(),
      customProviders.getConfig(),
      sessions.getConfig(),
    ],
  });

  settings.setBackend(backend);
  providerKeys.setBackend(backend);
  customProviders.setBackend(backend);
  sessions.setBackend(backend);

  const storage = new AppStorage(settings, providerKeys, sessions, customProviders, backend);
  setAppStorage(storage);

  return { settings, providerKeys, sessions, customProviders, storage };
}

/**
 * Enable CORS proxy for dev server.
 * Pi SDK routes API calls through `<proxyUrl>/?url=<target>`.
 * Vite's corsProxy plugin handles this server-side.
 */
export async function ensureProxySettings(settings: SettingsStore): Promise<void> {
  try {
    const proxyEnabled = await settings.get("proxy.enabled");
    if (!proxyEnabled) {
      await settings.set("proxy.enabled", true);
      await settings.set("proxy.url", `${window.location.origin}/cors-proxy`);
    }
  } catch {
    // Storage not ready yet — retry once
    setTimeout(() => ensureProxySettings(settings), 500);
  }
}
