/**
 * The More page: the workspace pages that have neither a slot in the sidebar nor an entry in Settings, as one plain
 * list of rows. The list is written out below rather than computed as what the sidebar and Settings leave over, so a
 * page that moves out of either is added here on purpose. A row is shown to the people the page itself is shown to.
 */
import { NavLink } from "react-router";
import { cn } from "../../ui/lib/cn";
import { PageHeader } from "../../ui/page/PageHeader";
import { type PageDef, pageByPath, shownTo } from "../pages";
import { useRole } from "../shell/session";

/** The paths of the pages More lists, in the order it lists them. */
export const MORE_PATHS = [
  "milestones",
  "intake",
  "chat",
  "terminal",
  "usage",
  "automations",
  "experiments",
  "decision-model",
  "kinds",
  "tokens",
  "ports",
  "plugin-settings",
  "start",
] as const;

/** The pages More lists for a person of `role`. */
export function morePages(role: string | undefined): PageDef[] {
  return MORE_PATHS.flatMap((path) => {
    const page = pageByPath(path);
    return page && shownTo(page, role) ? [page] : [];
  });
}

export function MorePage() {
  const role = useRole();
  return (
    <div data-testid="more-page">
      <PageHeader title="More" />
      <nav aria-label="More pages" className="max-w-sm">
        {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops the list from the accessibility tree */}
        <ul role="list" className="m-0 flex list-none flex-col gap-0.5 p-0">
          {morePages(role).map((page) => (
            <li key={page.path}>
              <NavLink
                to={`/${page.path}`}
                className={cn(
                  "flex h-9 items-center gap-2 rounded-control px-2 text-base text-text hover:bg-hover",
                )}
              >
                <page.icon className="size-5 shrink-0 text-secondary" weight="regular" />
                {page.title}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
