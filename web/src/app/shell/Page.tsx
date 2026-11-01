/** A page of the sidebar: its header (the group as breadcrumb in the code font, then the title) and its body. */

import type { PageDef } from "../pages";
import { Placeholder } from "./Placeholder";

export function Header({ breadcrumb, title }: { breadcrumb?: string; title: string }) {
  return (
    <header className="mb-[12px]">
      {breadcrumb && <div className="font-mono text-[12px] text-low">{breadcrumb}</div>}
      <h1 className="text-[19px] font-bold text-high">{title}</h1>
    </header>
  );
}

/** A page that is not built yet. */
export function PlaceholderPage({ page }: { page: PageDef }) {
  return (
    <>
      <Header breadcrumb={page.group || undefined} title={page.title} />
      <Placeholder title={page.title} />
    </>
  );
}

export function NotFoundPage() {
  return (
    <>
      <Header title="Not found" />
      <p className="text-sm text-normal">There is no such page. Pick one from the sidebar.</p>
    </>
  );
}
