/**
 * Importing skills or MCP servers (the desktop app's import panel): the person gives a zip file, which is sent to the
 * server with `POST /v1/uploads`, or a link to a repository or a page, and "Submit" starts a chat with the
 * Moebius agent that reads the source and writes what it finds with the same routes the pages use. The chat is the
 * Chat page's, which the panel links to once the agent has the request.
 */
import { useMutation } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { upload } from "../../api/calls/harness";
import { isApiError } from "../../api/http";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { type ImportKind, importRequest } from "./importRequest";

/** The largest file the server takes (`UPLOAD_LIMIT` of the desktop API): a larger one is not sent. */
export const UPLOAD_LIMIT = 20 * 1024 * 1024;

/** The sentence for a zip the server did not take, or null for a file it takes. */
export function uploadProblem(error: unknown): string {
  if (isApiError(error) && error.status === 413) return "The file is larger than the server takes.";
  return error instanceof Error ? error.message : "cannot send the file";
}

export function ImportPanel({ kind, onClose }: { kind: ImportKind; onClose: () => void }) {
  const fileId = useId();
  const input = useRef<HTMLInputElement>(null);
  const [fromZip, setFromZip] = useState(true);
  const [file, setFile] = useState<string | null>(null);
  const [uploaded, setUploaded] = useState<string | null>(null);
  const [tooBig, setTooBig] = useState(false);
  const [link, setLink] = useState("");
  const [note, setNote] = useState("");
  const sending = useMutation({
    mutationFn: upload,
    onSuccess: (sent) => setUploaded(sent.path),
  });
  const ask = useMutation({
    mutationFn: (message: string) => startChat({ agent: "Moebius", message }),
  });

  const source = fromZip
    ? uploaded !== null
      ? { zip: uploaded }
      : null
    : link.trim() !== ""
      ? { link: link.trim() }
      : null;
  const what = kind === "skills" ? "skills" : "MCP servers";

  function choose(chosen: File | undefined) {
    if (!chosen) return;
    setFile(chosen.name);
    setUploaded(null);
    setTooBig(chosen.size > UPLOAD_LIMIT);
    sending.reset();
    if (chosen.size <= UPLOAD_LIMIT) sending.mutate(chosen);
  }

  if (ask.isSuccess) {
    return (
      <Box title={`Import ${what}`} className="flex flex-col gap-3">
        <Notice tone="ok">The Moebius agent has the request.</Notice>
        <div className="flex flex-wrap items-center gap-3">
          <Button asChild size="sm">
            <Link to={`/chat/${ask.data.id}`}>Open the chat</Link>
          </Button>
          <Button variant="secondary" size="sm" onClick={onClose}>
            Close
          </Button>
        </div>
      </Box>
    );
  }
  const problem = tooBig
    ? "The file is larger than 20 MB, the most the server takes."
    : sending.isError
      ? uploadProblem(sending.error)
      : ask.isError
        ? ask.error.message
        : null;
  return (
    <Box title={`Import ${what}`} className="flex flex-col gap-3">
      <p className="m-0 max-w-prose text-base text-secondary">
        The Moebius agent reads the {what} from a zip file or a repository and saves them here; the
        chat opens.
      </p>
      <div className="flex flex-wrap gap-3">
        <Chip selected={fromZip} onClick={() => setFromZip(true)}>
          From a zip
        </Chip>
        <Chip selected={!fromZip} onClick={() => setFromZip(false)}>
          From a link
        </Chip>
      </div>
      {fromZip ? (
        <div className="flex flex-wrap items-center gap-3">
          <input
            ref={input}
            id={fileId}
            type="file"
            accept=".zip,application/zip"
            aria-label="Zip file"
            className="sr-only"
            onChange={(event) => choose(event.target.files?.[0])}
          />
          <Button
            variant="secondary"
            size="sm"
            disabled={sending.isPending}
            onClick={() => input.current?.click()}
          >
            Choose file
          </Button>
          <span className="min-w-0 break-words text-sm text-secondary">
            {file === null
              ? "No file chosen."
              : sending.isPending
                ? `Sending ${file}`
                : uploaded !== null
                  ? `${file} is on the server.`
                  : file}
          </span>
        </div>
      ) : (
        <Field label="A git repository or a page">
          {(id) => (
            <Input
              id={id}
              value={link}
              placeholder="https://github.com/acme/skills"
              onChange={(event) => setLink(event.target.value)}
            />
          )}
        </Field>
      )}
      <Field label="Note (optional)">
        {(id) => (
          <Input
            id={id}
            value={note}
            placeholder="What to import, which key it needs"
            onChange={(event) => setNote(event.target.value)}
          />
        )}
      </Field>
      {problem && <Notice tone="error">{problem}</Notice>}
      <div className="flex flex-wrap items-center gap-3">
        {source === null && !ask.isPending && !sending.isPending && (
          // Why the button is quiet, printed where a keyboard and a screen reader reach it.
          <span className="text-sm text-secondary">Choose a zip file or paste a link first</span>
        )}
        <Button
          size="sm"
          disabled={source === null || ask.isPending || sending.isPending}
          onClick={() => source && ask.mutate(importRequest(kind, source, note.trim()))}
        >
          {ask.isPending ? "Sending" : "Submit"}
        </Button>
        <Button variant="secondary" size="sm" onClick={onClose}>
          Cancel
        </Button>
      </div>
    </Box>
  );
}
