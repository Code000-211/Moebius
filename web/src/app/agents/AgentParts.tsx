/**
 * Two pieces of the agent editor's boxes: `Item`, a row of a list (what names it, and a line under it that is red
 * when something is wrong), and `Paths`, a box of file paths that are added by typing them and removed one by one.
 */
import { useState } from "react";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { addPath } from "./model";

/** A row of a list: what names it, and a line under it that is red when something is wrong. */
export function Item({ name, line, red }: { name: React.ReactNode; line?: string; red?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      {name}
      {line && (
        <span
          className={`min-w-0 break-words text-sm ${red ? "text-error-text" : "text-secondary"}`}
        >
          {line}
        </span>
      )}
    </div>
  );
}

export function Paths({
  heading,
  aside,
  empty,
  placeholder,
  addLabel,
  paths,
  onChange,
  browse,
}: {
  heading: string;
  aside: string;
  empty: string;
  placeholder: string;
  addLabel: string;
  paths: string[];
  onChange: (paths: string[]) => void;
  /** A way to find the files in a catalog instead of typing their path. */
  browse?: { label: string; onClick: () => void };
}) {
  const [typed, setTyped] = useState("");
  const add = () => {
    onChange(addPath(paths, typed));
    setTyped("");
  };
  return (
    <Box title={heading} aside={aside} className="flex flex-col gap-4 [&>header]:mb-0">
      {paths.length === 0 && <p className="m-0 text-base text-secondary">{empty}</p>}
      {paths.map((path) => (
        <div key={path} className="flex min-h-8 flex-wrap items-center justify-between gap-3">
          <span className="min-w-0 flex-1 break-all font-mono text-sm text-text">{path}</span>
          <Button
            variant="danger"
            size="sm"
            aria-label={`Remove ${path}`}
            onClick={() => onChange(paths.filter((held) => held !== path))}
          >
            Remove
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap items-end gap-3">
        <Input
          mono
          className="min-w-60 flex-1"
          aria-label={`${addLabel.replace("Add ", "")} path`}
          value={typed}
          placeholder={placeholder}
          onChange={(event) => setTyped(event.target.value)}
          onCommandEnter={add}
        />
        <Button
          variant="secondary"
          size="sm"
          disabled={typed.trim() === ""}
          title={typed.trim() === "" ? "Type the path first" : undefined}
          onClick={add}
        >
          {addLabel}
        </Button>
        {browse && (
          <Button variant="secondary" size="sm" onClick={browse.onClick}>
            {browse.label}
          </Button>
        )}
      </div>
    </Box>
  );
}
