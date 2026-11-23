/**
 * The form of a new page: the category it goes in (a folder of the group it is made in, none, or a new one), what a
 * template could make of it, and the editor with the name. The page is made at `<category>/<name>` inside its group,
 * the organisation's pages or one project's `projects/<KEY>/`.
 */
import { useState } from "react";
import type { WikiPageView } from "../../api/types/wiki";
import { CategoryPicker } from "./CategoryPicker";
import { categoriesOf, joinPath, pathProblem, splitFolder } from "./model";
import { PageEditor } from "./PageEditor";
import { FromTemplate } from "./WikiTemplates";

export function NewPage({
  folder,
  pages,
  keys,
  onMade,
  onCancel,
}: {
  /** Where the page was asked for: a group, and a category in it. */
  folder: string;
  pages: readonly { path: string }[];
  keys: readonly string[];
  onMade: (page: WikiPageView) => void;
  onCancel: () => void;
}) {
  const start = splitFolder(folder);
  const [category, setCategory] = useState(start.category);
  const where = joinPath(start.group, category.trim());
  const problem = category.trim() === "" ? null : pathProblem(joinPath(where, "x"), keys);
  return (
    <section aria-label="New page" className="flex min-w-0 flex-col gap-4">
      <h2 className="m-0 text-xl font-semibold tracking-tight text-text">New page</h2>
      <CategoryPicker
        categories={categoriesOf(pages, start.group)}
        value={category}
        onChange={setCategory}
        problem={problem}
      />
      <FromTemplate folder={where} keys={keys} onMade={onMade} />
      <PageEditor
        path={null}
        folder={where}
        text=""
        version={null}
        keys={keys}
        onSaved={onMade}
        onCancel={onCancel}
      />
    </section>
  );
}
