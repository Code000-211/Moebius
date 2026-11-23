/**
 * A project's Wiki tab (`/projects/:id/wiki`): the three columns of the Wiki page, but only the project's own pages
 * (`projects/<KEY>/`), its categories and its labels. The header is the project's, with Templates among its controls and
 * "New page" as its primary button, which makes the page in the project. The sidebar's Wiki keeps showing everything.
 */
import { PlusIcon } from "@phosphor-icons/react";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { ProjectHeader } from "../projects/ProjectHeader";
import { WikiWorkspace } from "./WikiWorkspace";

export function WikiTab({ project, onTerminal }: { project: Project; onTerminal: () => void }) {
  return (
    <WikiWorkspace
      scope={{ kind: "project", key: project.key, name: project.name }}
      header={({ canWrite, ready, onNew, onTemplates }) => (
        <ProjectHeader
          project={project}
          tab="wiki"
          onTerminal={onTerminal}
          actions={
            canWrite && ready ? (
              <Button size="sm" variant="ghost" onClick={onTemplates}>
                Templates
              </Button>
            ) : undefined
          }
          primary={
            canWrite && ready ? (
              <Button size="sm" onClick={onNew}>
                <PlusIcon className="size-4" aria-hidden />
                New page
              </Button>
            ) : undefined
          }
        />
      )}
    />
  );
}
