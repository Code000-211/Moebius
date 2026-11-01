/**
 * The script a column runs for a repository, read-only: where the repository keeps it, whether it is the repository's
 * own or the built-in one, and its text. The board's dock links here; the editor comes with the Rules and scripts page.
 */
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import { script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Loader } from "../../ui/components/Loader";
import { boardPath } from "../pages";
import { Header } from "../shell/Page";

export function ScriptPage() {
  const { id, column } = useParams();
  const repository = Number(id);
  const name = column ?? "";
  const source = useQuery({
    queryKey: keys.script(repository, name),
    queryFn: () => script(repository, name),
  });
  return (
    <>
      <Header breadcrumb="Projects" title={`Script of ${name}`} />
      <p className="mb-[12px] text-sm">
        <Link to={boardPath(repository)} className="text-accent hover:underline">
          Back to the board
        </Link>
      </p>
      {source.isPending && <Loader message="Reading the script…" className="py-double" />}
      {source.error && (
        <p role="alert" className="text-sm text-error">
          {isApiError(source.error) ? source.error.message : "Could not read the script."}
        </p>
      )}
      {source.data && (
        <>
          <p className="mb-base font-mono text-xs text-low">
            {source.data.path} ·{" "}
            {source.data.source === "repository" ? "this repository's own" : "built in"} · read-only
          </p>
          <pre
            data-testid="script"
            className="m-0 overflow-auto rounded-xl border bg-recessed p-double font-mono text-xs text-normal"
          >
            {source.data.text}
          </pre>
        </>
      )}
    </>
  );
}
