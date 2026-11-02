/**
 * The appearance of the app, on the Settings page: follow the system, or pick one (`docs/design-web.md`). The choice
 * belongs to this browser rather than to `moebius.toml` — `src/theme.ts` keeps it in local storage and applies it at
 * once — so it is saved as it is picked and has no Save button. The row is a radiogroup of buttons, the chosen one
 * drawn as the selected fill; an arrow key moves the choice, as it does across a row of `Tabs`.
 */
import { type Icon, MonitorIcon, MoonIcon, SunIcon } from "@phosphor-icons/react";
import { useRef } from "react";
import { type ThemePreference, useTheme } from "../../theme";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";

/** The three appearances the standard offers. */
const CHOICES: Array<{ value: ThemePreference; label: string; icon: Icon }> = [
  { value: "system", label: "Follow the system", icon: MonitorIcon },
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
];

export function Appearance() {
  const { preference, theme, setPreference } = useTheme();
  const row = useRef<HTMLDivElement>(null);
  return (
    <Box
      title="Appearance"
      aside={preference === "system" ? `Following the system: ${theme}` : "Saved in this browser"}
      className="min-w-0"
    >
      <div
        role="radiogroup"
        aria-label="Appearance"
        ref={row}
        className="flex flex-wrap items-center gap-2"
        onKeyDown={(event) => {
          if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
          const at = CHOICES.findIndex((each) => each.value === preference);
          const next =
            event.key === "ArrowRight"
              ? (at + 1) % CHOICES.length
              : (at - 1 + CHOICES.length) % CHOICES.length;
          event.preventDefault();
          setPreference((CHOICES[next] as { value: ThemePreference }).value);
          row.current?.querySelectorAll<HTMLButtonElement>("[role='radio']")[next]?.focus();
        }}
      >
        {CHOICES.map((each) => {
          const chosen = each.value === preference;
          return (
            // biome-ignore lint/a11y/useSemanticElements: a radio input cannot carry its icon or the selected fill
            <button
              key={each.value}
              type="button"
              role="radio"
              aria-checked={chosen}
              tabIndex={chosen ? 0 : -1}
              onClick={() => setPreference(each.value)}
              className={cn(
                "inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-control px-3 text-base",
                "transition-colors duration-(--motion-routine) ease-out",
                chosen
                  ? "bg-selected font-semibold text-text"
                  : "bg-sunken text-secondary hover:bg-hover hover:text-text",
              )}
            >
              <each.icon className="size-4 shrink-0" aria-hidden="true" />
              {each.label}
            </button>
          );
        })}
      </div>
    </Box>
  );
}
