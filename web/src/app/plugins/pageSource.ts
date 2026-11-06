/**
 * The source of a plugin's own page for its settings (see `PluginPage`): the module's text from
 * `GET /v1/plugins/{plugin}/settings-ui`, kept by plugin and version so a page drawn again does not read it again. The
 * text is handed to the sandboxed frame, never run in the app.
 */
import { settingsUi } from "../../api/calls/models";

/** The sources already read, by plugin and version. */
const read = new Map<string, Promise<string>>();

/** The module's text, read once per version. */
export function pageSource(plugin: string, version: string): Promise<string> {
  const key = `${plugin}@${version}`;
  let source = read.get(key);
  if (!source) {
    source = settingsUi(plugin);
    read.set(key, source);
    source.catch(() => read.delete(key));
  }
  return source;
}
