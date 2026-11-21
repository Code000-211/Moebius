/**
 * The one primary button of a list page's header: "New issue", which opens the form of a new issue over the page
 * (`useNewIssue`). The Inbox, My issues and a project's list carry it in `PageHeader`'s actions.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { Button } from "../../ui/components/Button";
import { useNewIssue } from "../board/NewIssueHost";

export function NewIssueButton() {
  const open = useNewIssue();
  return (
    <Button size="sm" className="px-3" onClick={() => open()}>
      <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
      New issue
    </Button>
  );
}
