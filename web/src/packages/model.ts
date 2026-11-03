/**
 * The logic of the package sheet, apart from React: the two catalogs and the kinds each can be filtered by, what a
 * result says about itself (installed, an update waiting), the sentences an install, update or removal is told in,
 * which installs happen at once and which are a change a person merges, and what a refused request says.
 */
import { type ApiError, isApiError } from "../api/http";
import type { InboxItem } from "../api/types/board";
import type {
  InstalledPackage,
  PackageKind,
  PackageOutcome,
  PackageSummary,
  PiHit,
  PiOffered,
  PiUpdates,
} from "../api/types/packages";
import { short } from "../app/usage/figures";
import type { Tone } from "../ui/page/Status";

/** Which package manager the sheet is pointed at: the registries of Moebius, or the pi.dev catalog. */
export type Catalog = "moebius" | "pi";

export interface KindChoice {
  value: string;
  label: string;
}

/** What the Moebius registries hold, in the order the filter lists them. */
export const MOEBIUS_KINDS: KindChoice[] = [
  { value: "plugin", label: "Plugin" },
  { value: "workflow", label: "Workflow" },
  { value: "rules", label: "Rules" },
  { value: "skills", label: "Skills" },
];

/** What the pi.dev catalog can be narrowed to. */
export const PI_KINDS: KindChoice[] = [
  { value: "extension", label: "Extension" },
  { value: "skill", label: "Skill" },
  { value: "theme", label: "Theme" },
  { value: "prompt", label: "Prompt" },
];

export const kindsOf = (catalog: Catalog): KindChoice[] =>
  catalog === "moebius" ? MOEBIUS_KINDS : PI_KINDS;

/** The words for a kind, as the filter has them; a kind the app does not know is said as it came. */
export function kindLabel(kind: string): string {
  const all = [...MOEBIUS_KINDS, ...PI_KINDS];
  return all.find((each) => each.value === kind)?.label ?? kind;
}

/** The source of a package of the pi.dev catalog: it is an npm package. */
export const piSource = (name: string) => `npm:${name}`;

/** Where a change a person merges is: the Inbox, narrowed to that change. */
export const changeLink = (change: number) => `/inbox?change=${change}`;

/** Moebius packages the server installs at once; the others are files proposed as a change. */
export const installsAtOnce = (kind: PackageKind): boolean => kind === "plugin";

/** Moebius packages that go to a repository, which one is named when there are several. */
export const needsRepository = (kind: PackageKind): boolean =>
  kind === "workflow" || kind === "rules";

/** The Moebius packages of one kind (all of them for none). */
export function inKind(packages: PackageSummary[], kind: string | null): PackageSummary[] {
  return kind === null ? packages : packages.filter((each) => each.kind === kind);
}

/** The packages of the pi.dev catalog of one kind (all of them for none); the server filters too, this is for a cache. */
export function piInKind(hits: PiHit[], kind: string | null): PiHit[] {
  return kind === null ? hits : hits.filter((each) => each.kinds.includes(kind));
}

export interface Standing {
  label: string;
  tone: Tone;
}

/**
 * What a result says about its package besides its name: that it is installed, and that a newer version waits. Neither
 * for a package that is not installed.
 */
export function standing(installed: string | null, newest: string | null): Standing[] {
  if (installed === null) return [];
  const here: Standing[] = [{ label: `Installed ${installed}`, tone: "complete" }];
  if (newest !== null && newest !== installed) {
    here.push({ label: `Update to ${newest}`, tone: "attention" });
  }
  return here;
}

/** A standing as the labels the tags of a row carry. */
export const standingTags = (
  all: Standing[],
): Array<{ label: string; tone: "attention" | "complete" }> =>
  all.map((each) => ({
    label: each.label,
    tone: each.tone === "attention" ? "attention" : "complete",
  }));

/** The standing of a result of the Moebius registries. */
export const summaryStanding = (each: PackageSummary): Standing[] =>
  standing(each.installed, each.compatible ? each.version : null);

/** The standing of an installed Moebius package: the nightly check's newer version, if it found one. */
export const installedStanding = (each: InstalledPackage): Standing[] =>
  standing(each.version, each.update_available);

/** The updates the nightly check of the Pi packages left in the inbox, by package: the version they go to. */
export function waitingPiUpdates(inbox: InboxItem[]): Map<string, { from: string; to: string }> {
  const waiting = new Map<string, { from: string; to: string }>();
  for (const item of inbox) {
    if (item.kind !== "extension_update") continue;
    const { package: name, from, to } = item.data;
    if (typeof name === "string" && typeof to === "string") {
      waiting.set(name, { from: typeof from === "string" ? from : "", to });
    }
  }
  return waiting;
}

/** A weekly download count as a result says it. */
export const downloads = (weekly: number): string => `${short(weekly)} a week`;

/** What a request told: the sentences, whether they are good news, and the change it proposed, if it proposed one. */
export interface Told {
  tone: "ok" | "error";
  lines: string[];
  change: number | null;
}

/**
 * The answer of an install, update or removal of a Moebius package in words: `installed <name> <version>`, the
 * plugin's own error when it does not run, and the restart a launcher needs.
 */
export function tellOutcome(outcome: PackageOutcome): Told {
  const { name, version } = outcome;
  const lines: string[] = [];
  let tone: Told["tone"] = "ok";
  switch (outcome.action) {
    case "installed":
      lines.push(`Installed ${name} ${version}.`);
      break;
    case "updated":
      lines.push(`Updated ${name} to ${version}.`);
      break;
    case "unchanged":
      lines.push(`${name} ${version} is the newest version that works with this Moebius.`);
      break;
    case "removed":
      // A package whose files go to a repository is only proposed for removal; a person merges the change.
      lines.push(outcome.change === undefined ? `Removed ${name}.` : `Proposed removing ${name}.`);
      if (outcome.leftover) {
        lines.push(`Its folder could not be deleted: ${outcome.leftover}. Delete it by hand.`);
      }
      break;
  }
  if (outcome.plugin?.error) {
    tone = "error";
    lines.push(`${name} does not run: ${outcome.plugin.error}`);
  }
  if (outcome.restart_needed) lines.push("Restart the server for it to take effect.");
  // An unchanged package proposes nothing, whatever the outcome carries.
  if (outcome.change !== undefined && outcome.action !== "unchanged") {
    lines.push(`Its files are proposed as change ${outcome.change}, which a person merges.`);
  }
  return {
    tone,
    lines,
    change: outcome.action === "unchanged" ? null : (outcome.change ?? null),
  };
}

/** The answer of a proposal to add, update or remove a Pi package in words. */
export function tellOffered(offered: PiOffered): Told {
  const { package: name, from, to } = offered;
  const what =
    offered.action === "add"
      ? `Proposed installing ${name} ${to ?? ""}`.trim()
      : offered.action === "update"
        ? `Proposed updating ${name} from ${from ?? "?"} to ${to ?? "?"}`
        : `Proposed removing ${name} ${from ?? ""}`.trim();
  const lines = [`${what}. Nothing changes until a person merges it.`];
  if (offered.agents.length > 0 && offered.action !== "remove") {
    lines.push(`It is named for ${offered.agents.join(", ")}.`);
  }
  if (offered.action === "remove" && offered.agents.length > 0) {
    lines.push(
      `${offered.agents.join(", ")} lose${offered.agents.length === 1 ? "s" : ""} its names.`,
    );
  }
  return { tone: "ok", lines, change: offered.change.id };
}

/** What an update of one Pi package or of all of them did: a proposal for each newer version, and why one failed. */
export function tellUpdates(updates: PiUpdates): Told[] {
  const told = updates.offered.map(tellOffered);
  for (const name of updates.up_to_date) {
    told.push({ tone: "ok", lines: [`${name} is up to date.`], change: null });
  }
  for (const failed of updates.failed) {
    told.push({
      tone: "error",
      lines: [`${failed.package} could not be updated. ${failed.error}`],
      change: null,
    });
  }
  return told;
}

/** The changelog of each proposal that has one, under the package it is for. */
export function changelogs(updates: PiUpdates): Array<{ name: string; text: string }> {
  return updates.offered
    .filter((each) => each.changelog.trim() !== "")
    .map((each) => ({ name: each.package, text: each.changelog.trim() }));
}

/** What a refused request says: the server's sentence, and for a session that is not a person's, whose it is not. */
export function refusal(error: unknown, fallback: string): string {
  if (!isApiError(error)) return fallback;
  const sentence: ApiError = error;
  if (sentence.status === 403) {
    // Said once: a server sentence that already says it is a person's to do is not added to ours.
    const whose = "Only a person can do this, and this session is not a person's.";
    return /person/i.test(sentence.message) ? whose : `${whose} ${sentence.message}`.trim();
  }
  return sentence.message === "" ? fallback : sentence.message;
}

/** A registry that could not be read, as a sentence. */
export const problemLine = (registry: string, message: string): string =>
  `The registry ${registry} could not be read. ${message}`;
