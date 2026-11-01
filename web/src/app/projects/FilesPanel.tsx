/**
 * The Files box of a repository: the tree of its default branch on the left and, on the right, the files that were
 * opened as tabs, the one shown in an editor. The two columns are inset panels inside the card and are separated by
 * space alone. A tab with unsaved changes is marked and asks before it closes. "Go to file" (Ctrl+P) and "Search in
 * files" (Ctrl+Shift+F) open over the editor and open a file, at a line when a hit was chosen. The open files and their
 * unsaved text are kept while the page is shown.
 */
import { MagnifyingGlassIcon, XIcon } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { FileEditor } from "./FileEditor";
import { FileTree } from "./FileTree";
import { GoToFile, SearchInFiles } from "./FindPanels";
import { parents } from "./model";

type Panel = "go" | "search" | null;

export function FilesPanel({ repository }: { repository: number }) {
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const [tabs, setTabs] = useState<string[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [panel, setPanel] = useState<Panel>(null);
  const [line, setLine] = useState<{ path: string; line: number } | null>(null);
  /** The tab whose closing waits for the answer to "drop its changes?". */
  const [closing, setClosing] = useState<string | null>(null);

  // Ctrl+P and Ctrl+Shift+F, as in an editor; the browser's own print and find give way while this page is shown.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === "p" && !e.shiftKey) {
        e.preventDefault();
        setPanel("go");
      } else if (key === "f" && e.shiftKey) {
        e.preventDefault();
        setPanel("search");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const show = (path: string, at?: number) => {
    setTabs((list) => (list.includes(path) ? list : [...list, path]));
    setCurrent(path);
    setClosing(null);
    setPanel(null);
    // The tree opens the directories above the file, so it is found there.
    setOpen((dirs) => new Set([...dirs, ...parents(path)]));
    setLine(at === undefined ? null : { path, line: at });
  };
  const close = (path: string) => {
    setTabs((list) => {
      const at = list.indexOf(path);
      const rest = list.filter((each) => each !== path);
      if (current === path) setCurrent(rest[Math.max(0, at - 1)] ?? null);
      return rest;
    });
    setDrafts((all) => {
      const { [path]: _dropped, ...rest } = all;
      return rest;
    });
    setClosing(null);
  };
  const toggle = (directory: string) =>
    setOpen((dirs) => {
      const next = new Set(dirs);
      if (!next.delete(directory)) next.add(directory);
      return next;
    });
  const unsaved = (path: string) => drafts[path] !== undefined;

  return (
    <Box title="Files" aside="Default branch" className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setPanel(panel === "go" ? null : "go")}
        >
          <MagnifyingGlassIcon className="size-4" aria-hidden="true" />
          Go to file
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => setPanel(panel === "search" ? null : "search")}
        >
          Search in files
        </Button>
      </div>
      {panel === "go" && (
        <GoToFile
          repository={repository}
          onOpen={(path) => show(path)}
          onClose={() => setPanel(null)}
        />
      )}
      {panel === "search" && (
        <SearchInFiles
          repository={repository}
          onOpen={(path, at) => show(path, at)}
          onClose={() => setPanel(null)}
        />
      )}
      {/* The tree stops where its last row stops: a box held to the height of the column beside it leaves a band of
          nothing under the files, which reads as a box that failed to fill. */}
      <div className="flex flex-col gap-4 md:flex-row md:items-start">
        <div className="max-h-96 w-full shrink-0 overflow-y-auto rounded-card bg-sunken p-2 md:w-64">
          <FileTree
            repository={repository}
            open={open}
            current={current}
            onToggle={toggle}
            onOpen={(path) => show(path)}
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-4">
          {tabs.length === 0 ? (
            <Empty
              plain
              title="No file open"
              body="Pick a file on the left to read or edit it. Go to file finds one by what you type, and Search in files finds the text inside them."
            />
          ) : (
            <div role="tablist" aria-label="Open files" className="flex flex-wrap gap-2">
              {tabs.map((path) => (
                <div
                  key={path}
                  className={cn(
                    "flex min-h-10 items-center rounded-control",
                    path === current ? "bg-selected" : "bg-sunken hover:bg-hover",
                  )}
                >
                  <button
                    type="button"
                    role="tab"
                    aria-selected={path === current}
                    title={path}
                    onClick={() => setCurrent(path)}
                    className="min-h-10 min-w-0 max-w-64 break-words px-3 text-sm text-text"
                  >
                    {path.split("/").pop()}
                    {unsaved(path) && (
                      <span className="ml-2 text-xs font-semibold text-attention-text">
                        Unsaved
                      </span>
                    )}
                  </button>
                  <IconButton
                    icon={XIcon}
                    variant="ghost"
                    aria-label={`Close ${path}`}
                    onClick={() => (unsaved(path) ? setClosing(path) : close(path))}
                  />
                </div>
              ))}
            </div>
          )}
          {closing !== null && (
            <div className="flex flex-wrap items-center gap-4 rounded-control bg-attention-fill p-3 text-attention-text">
              <p className="m-0 min-w-0 flex-1 break-words text-sm">
                {`${closing} has unsaved changes.`}
              </p>
              <Button size="sm" variant="secondary" onClick={() => setClosing(null)}>
                Keep
              </Button>
              <Button size="sm" variant="danger" onClick={() => close(closing)}>
                Discard
              </Button>
            </div>
          )}
          {current !== null && (
            <FileEditor
              key={current}
              repository={repository}
              path={current}
              draft={drafts[current]}
              onDraft={(text) =>
                setDrafts((all) => {
                  if (text === undefined) {
                    const { [current]: _dropped, ...rest } = all;
                    return rest;
                  }
                  return { ...all, [current]: text };
                })
              }
              line={line?.path === current ? line.line : null}
              onLineShown={() => setLine(null)}
            />
          )}
        </div>
      </div>
    </Box>
  );
}
