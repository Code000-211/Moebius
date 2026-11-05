/**
 * The settings plugins ask of a repository (`GET /v1/repositories/{id}/settings`), on the repository's page: the boxes
 * of `plugins/PluginSettingsBoxes`, read and saved for this repository.
 */
import { pluginProjectSettings, savePluginProjectSettings } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { PluginSettingsBoxes } from "../plugins/PluginSettingsBoxes";

export function PluginSettings({ repository }: { repository: number }) {
  return (
    <PluginSettingsBoxes
      scope={{
        queryKey: keys.pluginProjectSettings(repository),
        read: () => pluginProjectSettings(repository),
        save: (plugin, values) => savePluginProjectSettings(repository, plugin, values),
        empty:
          "No plugin on this server asks anything of a repository. One that does, such as a sandbox that builds the image of the runs, shows its settings here.",
      }}
    />
  );
}
