/**
 * The Plugins tab of a project's settings: the settings plugins ask of a repository (a sandbox's image, say), one group
 * for each repository of the project, in the same boxes as on a repository's page. A project with no repository has
 * nothing a plugin could ask of.
 */
import type { Project } from "../../api/types/projects";
import { PluginSettings } from "./PluginSettings";

export function ProjectPluginsTab({ project }: { project: Project }) {
  if (project.repositories.length === 0)
    return (
      <p className="m-0 max-w-prose text-base text-secondary">
        Plugins ask their settings of a repository, and this project has none yet.
      </p>
    );
  return (
    <div className="flex flex-col gap-8">
      {project.repositories.map((repository) => (
        <section key={repository.id} aria-label={`Plugins of ${repository.name}`}>
          <h2 className="m-0 mb-3 font-mono text-base font-semibold text-text">
            {repository.name}
          </h2>
          <PluginSettings repository={repository.id} />
        </section>
      ))}
    </div>
  );
}
