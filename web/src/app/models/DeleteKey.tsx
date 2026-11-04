/**
 * Deleting the API key stored on the server under a name, beside the field that replaces it: "Delete stored key" asks
 * once, then `DELETE /v1/secrets/{name}`. A provider that still names the key cannot start until a key is stored under
 * that name again, which the question says.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteSecret } from "../../api/calls/models";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";

export function DeleteKey({ name }: { name: string }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="sm" variant="danger" onClick={() => setOpen(true)}>
        Delete stored key
      </Button>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title={`Delete the stored key ${name}?`}
        onConfirm={async () => {
          await deleteSecret(name);
          await queryClient.invalidateQueries({ queryKey: keys.settings });
        }}
      >
        The key is removed from the server. A provider that still uses it cannot be reached until
        you store a key for it again.
      </ConfirmDelete>
    </>
  );
}
