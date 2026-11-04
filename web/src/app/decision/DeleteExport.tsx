/**
 * Deleting a training-set export, from its row of the Exports table: "Delete" asks once, then `DELETE /v1/exports/{id}`
 * removes its files from the server. The judgements it was made of stay; a new export can be made from them.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteExport } from "../../api/calls/improve";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";

export function DeleteExport({ id }: { id: number }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        size="sm"
        variant="danger"
        aria-label={`Delete export ${id}`}
        onClick={() => setOpen(true)}
      >
        Delete
      </Button>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title={`Delete export #${id}?`}
        onConfirm={async () => {
          await deleteExport(id);
          await queryClient.invalidateQueries({ queryKey: ["decision-model"] });
        }}
      >
        Its files are removed from the server. The decisions it was made of stay.
      </ConfirmDelete>
    </>
  );
}
