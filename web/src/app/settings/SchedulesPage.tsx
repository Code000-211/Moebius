/**
 * The Schedules entry of Settings: what the server starts by the clock. The nightly schedule is the times `moebius.toml`
 * sets (`retro_at`, the trial window), read from `GET /v1/settings`; the automations are those of `GET /v1/automations`,
 * each leading to the Automations page where it is edited. Nothing is changed here.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { automations as readAutomations } from "../../api/calls/automations";
import { settings as readSettings } from "../../api/calls/settings";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { scheduleName, until } from "../automations/model";
import { AreaColumn, AreaTitle, Section, ValueRow } from "./parts";

export function SchedulesPage() {
  const now = useNow();
  const config = useQuery({ queryKey: keys.settings, queryFn: readSettings });
  const list = useQuery({ queryKey: keys.automations, queryFn: readAutomations });
  const value = (key: string) => {
    const found = config.data?.values[key]?.value;
    return typeof found === "string" ? found : "—";
  };
  return (
    <AreaColumn>
      <AreaTitle
        title="Schedules"
        actions={
          <Button asChild size="sm" variant="secondary">
            <Link to="/automations">Open Automations</Link>
          </Button>
        }
      />
      <Section title="Nightly">
        {config.isError ? (
          <ReadFailed
            error={config.error}
            fallback="Could not read the settings."
            onRetry={() => void config.refetch()}
          />
        ) : config.isPending ? (
          <SkeletonList rows={2} />
        ) : (
          <Rows>
            <ValueRow label="Retro" value={value("retro_at")} />
            <ValueRow
              label="Trial window"
              value={`${value("trial_window.start")} – ${value("trial_window.end")}`}
            />
          </Rows>
        )}
      </Section>
      <Section title="Automations">
        {list.isError ? (
          <ReadFailed
            error={list.error}
            fallback="Could not read the automations."
            onRetry={() => void list.refetch()}
          />
        ) : list.isPending ? (
          <SkeletonList rows={2} />
        ) : list.data.length === 0 ? (
          <p className="m-0 text-base text-secondary">No automation yet.</p>
        ) : (
          <Rows>
            {list.data.map((each) => (
              <FieldRow
                key={each.id}
                label={each.name}
                description={
                  each.next_firing !== null
                    ? `${scheduleName(each.schedule)} · next ${until(now, each.next_firing)}`
                    : `${scheduleName(each.schedule)} · off${
                        each.last_firing ? ` · last ${ago(now, each.last_firing.at)}` : ""
                      }`
                }
              >
                <Button asChild size="sm" variant="secondary">
                  <Link to={`/automations?automation=${each.id}`} aria-label={`Open ${each.name}`}>
                    Open
                  </Link>
                </Button>
              </FieldRow>
            ))}
          </Rows>
        )}
      </Section>
    </AreaColumn>
  );
}
