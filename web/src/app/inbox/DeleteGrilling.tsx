/**
 * The grilling's way out of the inbox that is not an answer: "Delete" asks once, then deletes the grilling with its
 * chat (`DELETE /v1/grillings/{id}`). The issues it already filed stay on the forge and as cards.
 */
import { useState } from "react";
import { deleteGrilling } from "../../api/calls/inbox";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";

export function DeleteGrilling({ grilling }: { grilling: number }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="sm" variant="danger" onClick={() => setOpen(true)}>
        Delete
      </Button>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title="Delete this grilling?"
        onConfirm={() => deleteGrilling(grilling)}
      >
        The grilling and its chat are removed. Issues it already filed stay on the forge and as
        cards.
      </ConfirmDelete>
    </>
  );
}
