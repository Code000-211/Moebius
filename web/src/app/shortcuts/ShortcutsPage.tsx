/**
 * The Keyboard shortcuts page: every shortcut of the web app, grouped by where it works, from the same list
 * (`shell/palette/shortcuts.ts`) the palette shows its keys from. It opens on `?` from any page.
 */
import { PageHeader } from "../../ui/page/PageHeader";
import { keyName, SHORTCUT_GROUPS } from "../shell/palette/shortcuts";

export function ShortcutsPage() {
  return (
    <div data-testid="shortcuts-page">
      <PageHeader title="Keyboard shortcuts" />
      <div className="flex max-w-3xl flex-col gap-8">
        {SHORTCUT_GROUPS.map((group) => (
          <section key={group.title} aria-label={group.title}>
            <h2 className="text-base font-medium text-text">{group.title}</h2>
            <p className="mb-3 text-sm text-secondary">{group.note}</p>
            <ul className="divide-y divide-hairline rounded-control border border-hairline">
              {group.shortcuts.map((shortcut) => (
                <li
                  key={shortcut.label}
                  className="flex items-center justify-between gap-4 px-3 py-2 text-sm"
                >
                  <span>{shortcut.label}</span>
                  <span className="flex shrink-0 gap-1">
                    {shortcut.keys.map((key) => (
                      <kbd
                        key={key}
                        className="rounded-small bg-hover px-1.5 font-sans text-xs text-secondary"
                      >
                        {keyName(key)}
                      </kbd>
                    ))}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
