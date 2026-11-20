/**
 * The sub-navigation of Settings: a search field and the entries in their groups. Typing narrows the entries across
 * the groups; a search that matches nothing says so in one line. Entries come from `settingsEntries.ts`.
 */
import { useState } from "react";
import { NavLink } from "react-router";
import { SearchIcon } from "../../ui/icons";
import { cn } from "../../ui/lib/cn";
import { type SettingsEntry, searchEntries } from "./settingsEntries";

export function SettingsNav({ entries }: { entries: readonly SettingsEntry[] }) {
  const [query, setQuery] = useState("");
  const groups = searchEntries(entries, query);
  return (
    <nav
      aria-label="Settings"
      className="flex w-50 shrink-0 flex-col gap-4 overflow-y-auto border-r border-hairline p-3"
    >
      <label className="flex h-8 items-center gap-2 rounded-control border border-hairline px-2 text-secondary focus-within:border-accent-border">
        <SearchIcon className="size-4 shrink-0" />
        <input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search settings"
          aria-label="Search settings"
          className="min-w-0 flex-1 bg-transparent text-base text-text outline-none placeholder:text-secondary"
        />
      </label>
      {groups.length === 0 && (
        <p className="m-0 px-2 text-base text-secondary">No settings match.</p>
      )}
      {groups.map(({ group, entries: inGroup }) => (
        <div key={group} className="flex flex-col gap-0.5">
          <span className="px-2 pb-1 text-sm text-secondary">{group}</span>
          {inGroup.map((entry) => (
            <NavLink
              key={entry.path}
              to={`/${entry.path}`}
              className={({ isActive }) =>
                cn(
                  "flex min-h-8 items-center rounded-control px-2 text-base",
                  isActive
                    ? "bg-selected font-medium text-accent-text"
                    : "text-text hover:bg-hover",
                )
              }
            >
              {entry.label}
            </NavLink>
          ))}
        </div>
      ))}
    </nav>
  );
}
