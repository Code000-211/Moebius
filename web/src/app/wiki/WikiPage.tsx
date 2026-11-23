/**
 * The Wiki page: the organisation's pages and a group for each project, in the three columns of `WikiWorkspace`
 * (the tree, the page, its outline), under a header with "Templates" and "New page". A viewer reads; a member writes.
 * The project's own pages are also its Wiki tab (`projects/WikiTab.tsx`).
 */
import { PlusIcon } from "@phosphor-icons/react";
import { Button } from "../../ui/components/Button";
import { PageHeader } from "../../ui/page/PageHeader";
import { WikiWorkspace } from "./WikiWorkspace";

export function WikiPage() {
  return (
    <WikiWorkspace
      scope={{ kind: "all" }}
      header={({ canWrite, ready, onNew, onTemplates }) => (
        <PageHeader
          title="Wiki"
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
