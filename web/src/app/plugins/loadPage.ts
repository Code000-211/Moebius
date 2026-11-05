/**
 * Loads a plugin's own page for its settings (see `PluginPage`): the module's text from
 * `GET /v1/plugins/{plugin}/settings-ui`, imported from a blob URL, kept by plugin and version. Apart from `PluginPage`
 * so that tests can replace it, since a test's DOM imports no blob.
 */
import { settingsUi } from "../../api/calls/models";
import type { SettingsHost } from "./PluginPage";

/** The default export of a plugin's page: draws it in `element`, and may return what takes it down. */
export type Mount = (element: HTMLElement, host: SettingsHost) => undefined | (() => void);

/** The modules already loaded, by plugin and version, so a page drawn again does not load it again. */
const loaded = new Map<string, Promise<Mount>>();

async function load(plugin: string, version: string): Promise<Mount> {
  const source = await settingsUi(plugin);
  // The read carries the header every call does, which `import()` of the route could not; the module is imported
  // from the text it answered.
  const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  try {
    const module = (await import(/* @vite-ignore */ url)) as { default?: unknown };
    if (typeof module.default !== "function") {
      throw new Error(`the settings page of ${plugin} ${version} exports no function`);
    }
    return module.default as Mount;
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** The default export of the plugin's page, loaded once per version. */
export function loadPage(plugin: string, version: string): Promise<Mount> {
  const key = `${plugin}@${version}`;
  let page = loaded.get(key);
  if (!page) {
    page = load(plugin, version);
    loaded.set(key, page);
    page.catch(() => loaded.delete(key));
  }
  return page;
}
