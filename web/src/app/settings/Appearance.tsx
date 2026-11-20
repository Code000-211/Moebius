/**
 * The appearance of the app, on the Settings page: follow the system, or pick one (`docs/design-web.md`). The choice
 * belongs to this browser rather than to `moebius.toml` — `src/theme.ts` keeps it in local storage and applies it at
 * once — so it is saved as it is picked and has no Save button. The row is a radiogroup of buttons, the chosen one
 * drawn as the selected fill; an arrow key moves the choice, as it does across a row of `Tabs`.
 */
import { type Icon, MonitorIcon, MoonIcon, SunIcon } from "@phosphor-icons/react";
import { useRef } from "react";
import { useShowThinking } from "../../chat/thinking";
import { useReduceMotion } from "../../motion";
import { type ThemePreference, useTheme } from "../../theme";
import { cn } from "../../ui/lib/cn";
import { useBlur } from "../../ui/lib/effects";
import { FieldRow, Rows } from "../../ui/page/Field";
import { Switch } from "../../ui/page/Switch";

/** The three appearances the standard offers. */
const CHOICES: Array<{ value: ThemePreference; label: string; icon: Icon }> = [
  { value: "system", label: "System", icon: MonitorIcon },
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
];

export function Appearance() {
  const { preference, setPreference } = useTheme();
  const motion = useReduceMotion();
  const blur = useBlur();
  const thinking = useShowThinking();
  const row = useRef<HTMLDivElement>(null);
  return (
    <Rows>
      <FieldRow label="Theme">
        <div
          role="radiogroup"
          aria-label="Theme"
          ref={row}
          className="inline-flex items-center gap-0.5 rounded-card bg-sunken p-0.5"
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
                  "inline-flex min-h-7 cursor-pointer items-center gap-2 rounded-control px-3 text-base font-medium",
                  "transition-colors duration-(--motion-routine) ease-out",
                  chosen
                    ? "bg-surface text-text ring-1 ring-inset ring-hairline"
                    : "text-secondary hover:text-text",
                )}
              >
                <each.icon className="size-4 shrink-0" aria-hidden="true" />
                {each.label}
              </button>
            );
          })}
        </div>
      </FieldRow>
      <FieldRow
        label="Show thinking expanded"
        description="While an answer streams it is always open"
      >
        <Switch label="Show thinking expanded" checked={thinking.on} onChange={thinking.set} />
      </FieldRow>
      <FieldRow label="Reduce motion" description={motion.system ? "Set by the system" : undefined}>
        <Switch
          label="Reduce motion"
          checked={motion.on || motion.system}
          disabled={motion.system}
          onChange={motion.setOn}
        />
      </FieldRow>
      <FieldRow
        label="Blur on menus"
        description={blur.gpu ? undefined : "Off automatically without GPU"}
      >
        <Switch
          label="Blur on menus"
          checked={blur.on}
          disabled={!blur.gpu}
          onChange={blur.setOn}
        />
      </FieldRow>
    </Rows>
  );
}
