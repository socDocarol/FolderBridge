import type {
  Analysis,
  CleanupPreview,
  CleanupRequest,
  CleanupSummary,
  Entry,
  EntryFilter,
  Operation,
  OperationItem,
  OperationRequest,
  Progress,
  SavedPair,
  Scan,
  ScanOptions,
  Status,
} from "./types";
import { actions, isEligible } from "./format";

// Development-only fixture transport. This module is eliminated from production.
const folders = {
  source: "Example network / Shared documents",
  destination: "Example SharePoint / Team documents",
};
let scans: Scan[] = [
  {
    id: 1,
    ...folders,
    startedAt: Date.now() - 120000,
    state: "complete",
    verified: true,
    files: 18,
    errors: 0,
  },
];
const samples: [string, Status, number][] = [
  ["Administration/Team directory.xlsx", "identical", 384000],
  ["Budgets/2026 operating budget.xlsx", "different", 1240000],
  ["Budgets/Q3 forecast.xlsx", "source_only", 768000],
  ["Contracts/Vendor agreement.pdf", "identical", 2400000],
  ["Data/Asset inventory.accdb", "destination_only", 28400000],
  ["Data/Service requests.csv", "source_only", 4520000],
  ["Guides/New starter checklist.docx", "identical", 244000],
  ["Guides/Remote access.pdf", "different", 1850000],
  ["Meetings/September review.docx", "source_only", 186000],
  ["Projects/Atlas/Project brief.pdf", "identical", 1560000],
  ["Projects/Atlas/Timeline.xlsx", "source_only", 562000],
  ["Projects/Beacon/Research notes.md", "destination_only", 12000],
  ["Reports/Annual performance.pdf", "identical", 7820000],
  ["Reports/Quarterly dashboard.pbix", "different", 48300000],
  ["Scripts/Export inventory.ps1", "identical", 8200],
  ["Templates/Meeting notes.docx", "source_only", 46000],
  ["Training/Reference guide.pdf", "identical", 4280000],
  ["Working files/Readme.txt", "destination_only", 2400],
];
let entries: Entry[] = samples.map(([path, status, size], i) => ({
  id: i + 1,
  scanId: 1,
  relativePath: path,
  sourceRelative: status === "destination_only" ? null : path,
  destinationRelative: status === "source_only" ? null : path,
  extension: `.${path.split(".").pop()}`,
  status,
  sourceSize: status === "destination_only" ? null : size,
  destinationSize:
    status === "source_only"
      ? null
      : status === "different"
        ? size + 1000
        : size,
  sourceModified: 1788000000000000000,
  destinationModified: 1788000000000000000,
  sourceHash: status === "identical" ? "a".repeat(64) : null,
  destinationHash: status === "identical" ? "a".repeat(64) : null,
  owner: null,
  issue: null,
}));
let progress: Progress = {
  running: false,
  kind: "",
  phase: "",
  currentPath: "",
  processed: 0,
  total: 0,
  bytes: 0,
  errors: 0,
  scanId: null,
  operationId: null,
  message: "",
};
const operations: Operation[] = [];
const items: OperationItem[] = [];
const pairs: SavedPair[] = [];
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
function filtered(filter: EntryFilter) {
  return entries.filter(
    (e) =>
      (!filter.status || e.status === filter.status) &&
      (!filter.extension || e.extension === filter.extension) &&
      e.relativePath.toLowerCase().includes(filter.search.toLowerCase()) &&
      Math.max(e.sourceSize || 0, e.destinationSize || 0) >= filter.minSize,
  );
}
function analysis(): Analysis {
  const group = (key: (entry: Entry) => string) => {
    const map = new Map<
      string,
      { label: string; count: number; bytes: number }
    >();
    entries.forEach((e) => {
      const label = key(e);
      const row = map.get(label) || { label, count: 0, bytes: 0 };
      row.count++;
      row.bytes += e.sourceSize ?? e.destinationSize ?? 0;
      map.set(label, row);
    });
    return [...map.values()].sort((a, b) => b.bytes - a.bytes);
  };
  const sizeStatistics = [...new Set(entries.map((e) => e.extension))].map(
    (extension) => {
      const values = entries
        .filter((e) => e.extension === extension)
        .map((e) => e.sourceSize ?? e.destinationSize ?? 0)
        .sort((a, b) => a - b);
      const quantile = (p: number) => {
        const index = (values.length - 1) * p;
        const lower = Math.floor(index);
        return (
          values[lower] +
          (values[Math.min(values.length - 1, lower + 1)] - values[lower]) *
            (index - lower)
        );
      };
      return {
        extension,
        count: values.length,
        mean: values.reduce((a, b) => a + b, 0) / values.length,
        median: quantile(0.5),
        lowerQuartile: quantile(0.25),
        upperQuartile: quantile(0.75),
      };
    },
  );
  return {
    sizeStatistics,
    statuses: group((e) => e.status),
    extensions: group((e) => e.extension),
    folders: group((e) => e.relativePath.split("/")[0]),
    sourceBytes: entries.reduce((n, e) => n + (e.sourceSize || 0), 0),
    destinationBytes: entries.reduce((n, e) => n + (e.destinationSize || 0), 0),
    duplicateBytes: entries
      .filter((e) => e.status === "identical")
      .reduce((n, e) => n + (e.destinationSize || 0), 0),
  };
}
export async function demoCall<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  let result: unknown;
  switch (command) {
    case "list_scans":
      result = scans;
      break;
    case "list_pairs":
      result = [...pairs];
      break;
    case "list_operations":
      result = [...operations];
      break;
    case "get_progress":
      result = { ...progress };
      break;
    case "list_entries": {
      const filter = args.filter as EntryFilter;
      const rows = filtered(filter);
      result = {
        entries: rows.slice(filter.offset, filter.offset + filter.limit),
        total: rows.length,
      };
      break;
    }
    case "matching_entry_ids":
      result = filtered(args.filter as EntryFilter).map((e) => e.id);
      break;
    case "get_analysis":
      result = analysis();
      break;
    case "scan_folders": {
      const options = args.options as ScanOptions;
      if (!options.source.trim()) throw new Error("Choose a source folder.");
      progress = {
        ...progress,
        running: true,
        phase: "inventory",
        processed: 0,
        total: 18,
        currentPath: "Example documents",
        message: "",
      };
      await delay(650);
      progress = { ...progress, phase: "verifying", processed: 8 };
      await delay(650);
      if (!progress.running) {
        result = scans[0].id;
        break;
      }
      const id = scans[0].id + 1;
      scans = [
        {
          id,
          source: options.source,
          destination: options.destination,
          startedAt: Date.now(),
          state: "complete",
          verified: options.verifyContents,
          files: entries.length,
          errors: 0,
        },
        ...scans,
      ];
      entries = entries.map((e) => ({
        ...e,
        scanId: id,
        status: !options.destination
          ? "inventory"
          : !options.verifyContents && e.status === "identical"
            ? "unverified"
            : e.status,
      }));
      progress = {
        ...progress,
        running: false,
        phase: "complete",
        processed: 18,
        scanId: id,
        message: "Example comparison ready. No real files were accessed.",
      };
      result = id;
      break;
    }
    case "cancel_job":
      progress = { ...progress, running: false, phase: "cancelled" };
      break;
    case "preview_operation": {
      const request = args.request as OperationRequest;
      const selected = entries.filter((e) => request.entryIds.includes(e.id));
      const valid = selected.filter((e) => isEligible(e, request.action));
      result = {
        action: request.action,
        eligible: valid.length,
        skipped: selected.length - valid.length,
        bytes: valid.reduce(
          (n, e) => n + (e.sourceSize ?? e.destinationSize ?? 0),
          0,
        ),
        description:
          request.action === "quarantine_destination"
            ? "Move verified destination duplicates into quarantine. Restore is available in History. Disk space is not freed."
            : "Copy selected eligible files. Existing files and source versions are preserved.",
      };
      break;
    }
    case "execute_operation": {
      const request = args.request as OperationRequest;
      const selected = entries.filter(
        (e) => request.entryIds.includes(e.id) && isEligible(e, request.action),
      );
      const id = operations.length + 1;
      progress = {
        ...progress,
        running: true,
        phase: "applying",
        processed: 0,
        total: selected.length,
      };
      await delay(800);
      operations.unshift({
        id,
        scanId: request.scanId,
        action: request.action,
        startedAt: Date.now(),
        state: "complete",
        completed: selected.length,
        skipped: 0,
        errors: 0,
      });
      items.push(
        ...selected.map((e, i) => ({
          id: id * 100 + i,
          operationId: id,
          relativePath: e.relativePath,
          originalPath: `Example ${request.action === "copy_to_source" ? "destination" : "source"}/${e.relativePath}`,
          targetPath: `Example ${request.action === "copy_to_source" ? "source" : "destination"}/${e.relativePath}`,
          state:
            request.action === "quarantine_destination"
              ? "quarantined"
              : "copied",
          message: "Simulated operation. No real files changed.",
          contentHash: "a".repeat(64),
          cleanup: null,
        })),
      );
      progress = {
        ...progress,
        running: false,
        phase: "complete",
        message: `Preview: ${actions[request.action].label} simulated.`,
        operationId: id,
        processed: selected.length,
      };
      result = id;
      break;
    }
    case "get_operation_items":
      result = items.filter((i) => i.operationId === args.operationId);
      break;
    case "preview_cleanup": {
      const operationId = args.operationId as number;
      const operation = operations.find((row) => row.id === operationId);
      if (
        !operation ||
        !["copy_to_destination", "copy_to_source", "keep_both"].includes(
          operation.action,
        )
      )
        throw new Error("Choose a finished copy operation.");
      result = {
        originalRoot:
          operation.action === "copy_to_source"
            ? folders.destination
            : folders.source,
        targetRoot:
          operation.action === "copy_to_source"
            ? folders.source
            : folders.destination,
        isNetwork: operation.action !== "copy_to_source",
        items: items
          .filter((item) => item.operationId === operationId)
          .map((item) => ({
            item,
            eligible:
              item.state === "copied" &&
              (!item.cleanup ||
                ["failed", "restored"].includes(item.cleanup.state)),
            reason:
              item.state !== "copied"
                ? "Only completed copies are eligible."
                : item.cleanup
                  ? "Already handled or under review."
                  : "",
          })),
      } satisfies CleanupPreview;
      break;
    }
    case "cleanup_originals": {
      const request = args.request as CleanupRequest;
      if (request.confirmation !== `REMOVE ${request.itemIds.length}`)
        throw new Error("Type the confirmation exactly.");
      if (request.handling === "sharepoint" && !request.sharepointConfirmed)
        throw new Error("Confirm OneDrive sync first.");
      const selected = items.filter(
        (item) =>
          item.operationId === request.operationId &&
          request.itemIds.includes(item.id),
      );
      selected.forEach((item) => {
        item.cleanup = {
          method: request.handling,
          state: request.handling === "network" ? "held" : "recycled",
          holdingPath: `${request.handling === "network" ? folders.source + "/_ToDelete/" + request.operationId : folders.destination + "/.folderbridge-staging/cleanup-" + request.operationId}/${item.relativePath}`,
          message: "Simulated cleanup. No real files changed.",
        };
      });
      result = {
        completed: selected.length,
        skipped: 0,
        message: "Simulated cleanup. No real files changed.",
      } satisfies CleanupSummary;
      break;
    }
    case "restore_originals": {
      const selected = items.filter(
        (item) =>
          item.operationId === args.operationId &&
          (args.itemIds as number[]).includes(item.id),
      );
      selected.forEach((item) => {
        if (item.cleanup) {
          item.cleanup.state = "restored";
          item.cleanup.message = "Simulated restore. No real files changed.";
        }
      });
      result = {
        completed: selected.length,
        skipped: 0,
        message: "Simulated restore. No real files changed.",
      } satisfies CleanupSummary;
      break;
    }
    case "reveal_operation_item":
      result = { path: "Example file", selectFile: true };
      break;
    case "restore_operation":
      items
        .filter((i) => i.operationId === args.operationId)
        .forEach((i) => {
          i.state = "restored";
        });
      break;
    case "save_pair":
      pairs.push({
        id: Date.now(),
        name: args.name as string,
        options: args.options as ScanOptions,
      });
      break;
    case "delete_pair": {
      const index = pairs.findIndex((p) => p.id === args.id);
      if (index >= 0) pairs.splice(index, 1);
      break;
    }
    default:
      throw new Error("This command is available in the desktop app.");
  }
  return result as T;
}
