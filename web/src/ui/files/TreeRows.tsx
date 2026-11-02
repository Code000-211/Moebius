/**
 * The rows every list of files in the app wears, so a folder of a repository and a folder of a change's diff are read
 * the same way: a name at body size with the mark of what it is before it, the whole path in the row's tooltip rather
 * than repeated down the column, and the row standing further in for every folder above it. The list owns the `<ul>`
 * and the `<li>`; a row is only the thing inside one, and takes whatever that list wants at its right end — a diff
 * puts what each file costs there, and the repository's tree puts nothing.
 */
import { CaretDownIcon, CaretRightIcon, FileIcon, FolderIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

/**
 * How far a row of depth `depth` stands in, on the 8pt grid. A list that draws a row of its own — a folder that is
 * still being read, a folder that holds nothing — indents it the same way, so the column does not step.
 */
export const indent = (depth: number) => ({ paddingLeft: `${8 + depth * 16}px` });

const ROW =
  "flex min-h-10 w-full items-center gap-2 rounded-control pr-2 text-left transition-colors duration-(--motion-routine) ease-out";

interface RowProps {
  /** The last part of the path, which is what the row prints. */
  name: string;
  /** The whole path, which is what the row stands for. */
  path: string;
  depth: number;
  /** Whatever the list adds at the right end of the row, such as what a file costs. */
  trailing?: ReactNode;
}

/** A file: the row that opens it, marked as the one the page is showing. */
export function FileRow({
  name,
  path,
  depth,
  chosen,
  onOpen,
  trailing,
}: RowProps & { chosen: boolean; onOpen: (path: string) => void }) {
  return (
    <button
      type="button"
      style={indent(depth)}
      title={path}
      aria-current={chosen ? "true" : undefined}
      onClick={() => onOpen(path)}
      className={cn(ROW, "hover:bg-hover", chosen ? "bg-selected text-text" : "text-secondary")}
    >
      <FileIcon className="size-4 shrink-0" aria-hidden="true" />
      <span className="min-w-0 flex-1 break-words">{name}</span>
      {trailing}
    </button>
  );
}

/** A folder: the row that opens or closes what is under it. */
export function FolderRow({
  name,
  path,
  depth,
  open,
  onToggle,
  trailing,
}: RowProps & { open: boolean; onToggle: (path: string) => void }) {
  return (
    <button
      type="button"
      style={indent(depth)}
      title={path}
      aria-expanded={open}
      onClick={() => onToggle(path)}
      className={cn(ROW, "text-secondary hover:bg-hover")}
    >
      {open ? (
        <CaretDownIcon className="size-4 shrink-0" aria-hidden="true" />
      ) : (
        <CaretRightIcon className="size-4 shrink-0" aria-hidden="true" />
      )}
      <FolderIcon className="size-4 shrink-0" aria-hidden="true" />
      <span className="min-w-0 flex-1 break-words">{name}</span>
      {trailing}
    </button>
  );
}
