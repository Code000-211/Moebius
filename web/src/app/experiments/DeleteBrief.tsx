/**
 * Deleting a research brief, from the brief itself: "Delete" asks once, then `DELETE /v1/research/{id}` takes the
 * brief, its experiments, their runs and their lever branches. A brief that is running is stopped first, which the
 * question says. The page goes on to the next brief of the list.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteResearch } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import type { Research } from "../../api/types/improve";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";

export function DeleteBrief({
  research,
  onDeleted,
}: {
  research: Research;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="sm" variant="danger" onClick={() => setOpen(true)}>
        Delete
      </Button>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title="Delete this brief?"
        onConfirm={async () => {
          await deleteResearch(research.id);
          await queryClient.invalidateQueries({ queryKey: keys.research });
          onDeleted();
        }}
      >
        {"The brief is removed with its experiments, their runs and the lever branches they made."}
        {research.status === "running" && " It is running now, so it is stopped first."}
      </ConfirmDelete>
    </>
  );
}
