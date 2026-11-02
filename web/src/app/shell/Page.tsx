/** The pages the sidebar lists that the web app does not build yet, and the page for an address that matches nothing.
 * Both are a `PageHeader` and an empty state, like every other page. */
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import type { PageDef } from "../pages";
import { Placeholder } from "./Placeholder";

/** A page that is not built yet. */
export function PlaceholderPage({ page }: { page: PageDef }) {
  return (
    <>
      <PageHeader title={page.title} />
      <Placeholder title={page.title} />
    </>
  );
}

/** An address that is not one of the app's pages. */
export function NotFoundPage() {
  return (
    <>
      <PageHeader title="Not found" />
      <Empty title="There is no such page" body="Pick one from the sidebar." />
    </>
  );
}
