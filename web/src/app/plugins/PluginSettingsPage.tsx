/**
 * The Plugin settings page (`/plugin-settings`, only in the web app), one of the pages Settings holds: what each
 * running plugin asks of the whole server (`app_settings` of its manifest), one box per plugin, read from
 * `GET /v1/plugins/settings` and saved per plugin with `PUT /v1/plugins/{plugin}/settings`. What a plugin asks of one
 * repository is on that repository's page instead.
 */
import { appSettings, saveAppSettings } from "../../api/calls/models";
import { keys } from "../../api/keys";
import { PageHeader } from "../../ui/page/PageHeader";
import { PluginSettingsBoxes, type SettingsScope } from "./PluginSettingsBoxes";

const APP_SCOPE: SettingsScope = {
  queryKey: keys.appSettings,
  read: appSettings,
  save: saveAppSettings,
  empty:
    "No plugin on this server asks anything of it. One that does, such as a sandbox with a base image for every repository, shows its settings here.",
};

export function PluginSettingsPage() {
  return (
    <>
      <PageHeader
        title="Plugin settings"
        context="What the plugins ask of the whole server. What a plugin asks of one repository is on that repository's page, under Projects."
      />
      <div className="flex flex-col gap-6">
        <PluginSettingsBoxes scope={APP_SCOPE} />
      </div>
    </>
  );
}
