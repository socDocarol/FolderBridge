import type { Action, Entry, Status } from "./types";

export function bytes(value: number | null | undefined): string {
  if (value == null) return "—";
  if (value === 0) return "0 B";
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), 5);
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: index < 2 ? 0 : 1 }).format(value / 1024 ** index)} ${["B", "KB", "MB", "GB", "TB", "PB"][index]}`;
}
export const count = (value: number) => new Intl.NumberFormat().format(value);
export const date = (value: number) =>
  new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
export const basename = (value: string) =>
  value
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() || value;
export const readablePath = (value: string) =>
  value.startsWith("\\\\?\\UNC\\")
    ? `\\\\${value.slice(8)}`
    : value.replace(/^\\\\\?\\/, "");
export const statuses: Record<Status, string> = {
  source_only: "Source only",
  destination_only: "Destination only",
  identical: "Identical",
  different: "Different",
  unverified: "Not verified",
  excluded: "Excluded",
  error: "Issue",
  inventory: "Inventoried",
  pending: "Pending",
};
export const actions: Record<
  Action,
  { label: string; verb: string; status: Status }
> = {
  copy_to_destination: {
    label: "Copy to destination",
    verb: "Copy files",
    status: "source_only",
  },
  copy_to_source: {
    label: "Recover to source",
    verb: "Recover files",
    status: "destination_only",
  },
  keep_both: {
    label: "Keep both",
    verb: "Copy with new names",
    status: "different",
  },
  quarantine_destination: {
    label: "Quarantine duplicates",
    verb: "Move to quarantine",
    status: "identical",
  },
};
export function isEligible(entry: Entry, action: Action) {
  return (
    entry.status === actions[action].status &&
    (action !== "quarantine_destination" ||
      Boolean(entry.sourceHash && entry.destinationHash))
  );
}
