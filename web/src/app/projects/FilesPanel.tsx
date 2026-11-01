/**
 * The Files box of a repository: the tree of its default branch on the left and, on the right, the files that were
 * opened as tabs, the one shown in an editor. A tab with unsaved changes is marked and asks before it closes. "Go to
 * file" (Ctrl+P) and "Search in files" (Ctrl+Shift+F) open over the editor and open a file, at a line when a hit was
 * chosen. The open files and their unsaved text are kept while the page is shown.
 */
import { MagnifyingGlassIcon, XIcon } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
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
    <Box title="Files" aside="the default branch" className="flex flex-col gap-base">
      <div className="mb-base flex flex-wrap gap-base">
        <Button size="sm" variant="outline" onClick={() => setPanel(panel === "go" ? null : "go")}>
          <MagnifyingGlassIcon className="size-icon-sm" /> Go to file
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => setPanel(panel === "search" ? null : "search")}
        >
          Search in files
        </Button>
      </div>
      {panel === "go" && (
        <div className="mb-base">
          <GoToFile
            repository={repository}
            onOpen={(path) => show(path)}
            onClose={() => setPanel(null)}
          />
        </div>
      )}
      {panel === "search" && (
        <div className="mb-base">
          <SearchInFiles
            repository={repository}
            onOpen={(path, at) => show(path, at)}
            onClose={() => setPanel(null)}
          />
        </div>
      )}
      <div className="flex min-h-[200px] flex-col gap-base md:flex-row">
        <div className="max-h-[480px] w-full shrink-0 overflow-y-auto rounded-md border bg-lane p-half md:w-[240px]">
          <FileTree
            repository={repository}
            open={open}
            current={current}
            onToggle={toggle}
            onOpen={(path) => show(path)}
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-base">
          {tabs.length === 0 ? (
            <p className="m-0 text-[13px] text-low">Pick a file to read or edit it.</p>
          ) : (
            <div role="tablist" aria-label="Open files" className="flex flex-wrap gap-half">
              {tabs.map((path) => (
                <div
                  key={path}
                  className={cn(
                    "flex items-center rounded-md border",
                    path === current ? "border-accent-border bg-accent-fill" : "bg-surface",
                  )}
                >
                  <button
                    type="button"
                    role="tab"
                    aria-selected={path === current}
                    onClick={() => setCurrent(path)}
                    className="max-w-[200px] truncate px-base py-half font-mono text-[12px] text-high"
                  >
                    {path.split("/").pop()}
                    {unsaved(path) && <span title="Unsaved changes"> ●</span>}
                  </button>
                  <button
                    type="button"
                    aria-label={`Close ${path}`}
                    onClick={() => (unsaved(path) ? setClosing(path) : close(path))}
                    className="px-half text-low hover:text-high"
                  >
                    <XIcon className="size-icon-xs" />
                  </button>
                </div>
              ))}
            </div>
          )}
          {closing !== null && (
            <div className="flex flex-wrap items-center gap-base text-[12.5px] text-normal">
              <span>{`${closing} has unsaved changes.`}</span>
              <Button size="xs" variant="outline" onClick={() => setClosing(null)}>
                Keep
              </Button>
              <Button size="xs" variant="destructive" onClick={() => close(closing)}>
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
