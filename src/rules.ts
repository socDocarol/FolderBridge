import type { FileRules, ScanOptions } from "./types";

export const emptyRules: FileRules = {
  excludedPaths: [],
  minSize: null,
  maxSize: null,
  reviewAbove: null,
  reviewAccessAbove: null,
  reviewExtensions: [],
};
export const backupTypes = [".bak", ".bac", ".bacpac", ".dmp"];
export const temporaryTypes = [".tmp", ".temp", ".log", ".trc"];
export const mib = 1024 * 1024;

export function withRules(options: ScanOptions): ScanOptions {
  const extensions = (values: string[]) => [
    ...new Set(
      values
        .map((value) => value.trim().replace(/^\.+/, "").toLowerCase())
        .filter(Boolean)
        .map((value) => "." + value),
    ),
  ];
  return {
    ...options,
    excludedExtensions: extensions(options.excludedExtensions),
    rules: {
      ...emptyRules,
      ...options.rules,
      reviewExtensions: extensions(options.rules?.reviewExtensions || []),
    },
  };
}

export function rulesSummary(options: ScanOptions): string {
  const rules = { ...emptyRules, ...options.rules };
  const parts: string[] = [];
  if (options.excludedExtensions.length)
    parts.push(`Skip ${options.excludedExtensions.length} types`);
  if (rules.excludedPaths.length)
    parts.push(`Skip ${rules.excludedPaths.length} paths`);
  if (rules.minSize != null) parts.push(`Size ≥ ${rules.minSize / mib} MiB`);
  if (rules.maxSize != null) parts.push(`Size ≤ ${rules.maxSize / mib} MiB`);
  if (rules.reviewAbove != null)
    parts.push(`Review > ${rules.reviewAbove / mib} MiB`);
  if (rules.reviewAccessAbove != null)
    parts.push(`Access > ${rules.reviewAccessAbove / mib} MiB`);
  if (rules.reviewExtensions.length)
    parts.push(`Review ${rules.reviewExtensions.length} types`);
  return parts.length
    ? parts.join(" · ")
    : "No exclusions · Conflicts need review";
}
