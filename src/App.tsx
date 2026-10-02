import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowRight,
  BarChart3,
  Bookmark,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  CircleHelp,
  Clock3,
  Copy,
  Database,
  FileSearch,
  Flag,
  Folder,
  FolderInput,
  FolderOpen,
  History,
  Info,
  Layers2,
  LoaderCircle,
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Undo2,
  X,
} from "lucide-react";
import { call, chooseFolder, chooseReport, desktop } from "./api";
import { OperationDetails } from "./OperationDetails";
import { RulesDialog } from "./RulesDialog";
import { emptyRules, rulesSummary, withRules } from "./rules";
import {
  actions,
  basename,
  bytes,
  count,
  date,
  readablePath,
  statuses,
} from "./format";
import {
  Dialog,
  EmptyState,
  FileIcon,
  ProgressBar,
  StatusBadge,
  WindowControls,
} from "./components";
import type {
  Action,
  Analysis,
  CleanupRequest,
  CleanupSummary,
  Entry,
  EntryFilter,
  EntryPage,
  GroupTotal,
  Operation,
  OperationItem,
  OperationRequest,
  Preview,
  Progress,
  SavedPair,
  Scan,
  ScanOptions,
} from "./types";

type Page = "compare" | "migrate" | "insights" | "history";
const emptyOptions: ScanOptions = {
  source: "",
  destination: "",
  compareBothWays: false,
  verifyContents: true,
  excludedExtensions: [],
  collectOwners: false,
  rules: emptyRules,
};
const pageNames: Record<Page, string> = {
  compare: "Compare",
  migrate: "Migrate",
  insights: "Storage",
  history: "History",
};
const blankAnalysis: Analysis = {
  statuses: [],
  sourceStatuses: [],
  migrationStates: [],
  extensions: [],
  folders: [],
  sourceBytes: 0,
  destinationBytes: 0,
  duplicateBytes: 0,
  sizeStatistics: [],
};

export default function App() {
  const [page, setPage] = useState<Page>("compare");
  const [options, setOptions] = useState<ScanOptions>(emptyOptions);
  const sourceOnly =
    page === "compare" &&
    Boolean(options.destination) &&
    !options.compareBothWays;
  const [scans, setScans] = useState<Scan[]>([]);
  const [scanId, setScanId] = useState<number | null>(null);
  const [pairs, setPairs] = useState<SavedPair[]>([]);
  const [operations, setOperations] = useState<Operation[]>([]);
  const [moreOperations, setMoreOperations] = useState(false);
  const [moreScans, setMoreScans] = useState(false);
  const [loadedRows, setData] = useState<EntryPage>({ entries: [], total: 0 });
  const data = loadedRows.entries.some((entry) => entry.scanId !== scanId)
    ? { entries: [], total: 0 }
    : sourceOnly
      ? {
          ...loadedRows,
          entries: loadedRows.entries.filter(
            (entry) =>
              entry.sourceRelative != null ||
              entry.destinationRelative == null ||
              entry.status === "error",
          ),
        }
      : loadedRows;
  const [analysis, setAnalysis] = useState<Analysis>(blankAnalysis);
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [status, setStatus] = useState("");
  const [migrationState, setMigrationState] = useState("");
  const [extension, setExtension] = useState("");
  const [minSize, setMinSize] = useState("");
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState(new Set<number>());
  const [details, setDetails] = useState<Entry | null>(null);
  const [rulesDraft, setRulesDraft] = useState<ScanOptions | null>(null);
  const [rulesSection, setRulesSection] = useState<"Types" | "Files">("Types");
  const [showOptions, setShowOptions] = useState(false);
  const [showFilters, setShowFilters] = useState(false);
  const [help, setHelp] = useState(false);
  const [showPairs, setShowPairs] = useState(false);
  const [revealMessage, setRevealMessage] = useState("");
  const [storageView, setStorageView] = useState<
    "types" | "folders" | "statistics"
  >("types");
  const [historyView, setHistoryView] = useState<"operations" | "scans">(
    "operations",
  );
  const [pageSize, setPageSize] = useState(6);
  const tableRef = useRef<HTMLDivElement>(null);
  const [savePair, setSavePair] = useState(false);
  const [pairName, setPairName] = useState("");
  const [action, setAction] = useState<Action>("copy_to_destination");
  const [review, setReview] = useState<{
    preview: Preview;
    request: OperationRequest;
  } | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [flagsApproved, setFlagsApproved] = useState(false);
  const [migrationReview, setMigrationReview] = useState<{
    plans: { request: OperationRequest; preview: Preview }[];
    selectedCount: number;
  } | null>(null);
  const cancelRequested = useRef(false);
  const [operationDetail, setOperationDetail] = useState<{
    operation: Operation;
    items: OperationItem[];
  } | null>(null);
  const [restoreReview, setRestoreReview] = useState<Operation | null>(null);
  const [busy, setBusy] = useState(true);
  const [loadingRows, setLoadingRows] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const current = scans.find((s) => s.id === scanId);
  const running = busy || Boolean(progress?.running);
  const filter = useMemo<EntryFilter>(
    () => ({
      scanId: scanId || 0,
      sourceOnly,
      search: deferredSearch,
      status,
      migrationState: page === "migrate" ? migrationState : "",
      extension,
      minSize: (Number(minSize) || 0) * 1024 * 1024,
      offset,
      limit: pageSize,
    }),
    [
      scanId,
      sourceOnly,
      deferredSearch,
      status,
      migrationState,
      page,
      extension,
      minSize,
      offset,
      pageSize,
    ],
  );
  const reload = useCallback(async () => {
    const [s, p, o] = await Promise.all([
      call<Scan[]>("list_scans"),
      call<SavedPair[]>("list_pairs"),
      call<Operation[]>("list_operations"),
    ]);
    setScans(s);
    setPairs(p);
    setOperations(o);
    setMoreOperations(o.length === 100);
    setMoreScans(s.length === 100);
    return s;
  }, []);
  useEffect(() => {
    reload()
      .then((s) => {
        if (s[0]) {
          setScanId(s[0].id);
          setOptions(scanOptions(s[0]));
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  }, [reload]);
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => {
      call<Progress>("get_progress")
        .then(setProgress)
        .catch((e) => setError(String(e)));
    }, 400);
    return () => clearInterval(timer);
  }, [running]);
  useEffect(() => {
    if (!scanId) {
      setData({ entries: [], total: 0 });
      return;
    }
    let active = true;
    setLoadingRows(true);
    call<EntryPage>("list_entries", { filter })
      .then((rows) => {
        if (active) {
          setData(rows);
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setLoadingRows(false);
      });
    return () => {
      active = false;
    };
  }, [filter, scanId]);
  useEffect(() => {
    setAnalysis(blankAnalysis);
    if (!scanId) return;
    let active = true;
    call<Analysis>("get_analysis", { scanId })
      .then((stats) => {
        if (active) setAnalysis(stats);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [scanId]);
  useEffect(() => {
    setOffset(0);
  }, [
    deferredSearch,
    status,
    migrationState,
    page,
    extension,
    minSize,
    scanId,
  ]);
  useEffect(() => {
    setSelected(new Set());
    setDetails(null);
    if (sourceOnly) setAction("copy_to_destination");
  }, [scanId, sourceOnly]);

  useEffect(() => {
    const element = tableRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      if (element.clientHeight > 0)
        setPageSize(
          Math.max(
            1,
            Math.min(50, Math.floor((element.clientHeight - 33) / 44)),
          ),
        );
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [page, scanId]);

  async function reveal(entry: Entry, side: "source" | "destination") {
    setRevealMessage("");
    if (!desktop) {
      setNotice("Explorer is available in the desktop app.");
      setRevealMessage("Explorer is available in the desktop app.");
      return;
    }
    try {
      const result = await call<{ path: string; selectFile: boolean }>(
        "reveal_entry",
        { scanId: entry.scanId, entryId: entry.id, side },
      );
      const message = result.selectFile
        ? "Selected in Explorer."
        : "Opened the nearest existing folder.";
      setNotice(message);
      setRevealMessage(message);
    } catch (e) {
      setError(String(e));
      setRevealMessage(String(e));
    }
  }

  async function task<T>(work: () => Promise<T>): Promise<T | undefined> {
    setError("");
    try {
      return await work();
    } catch (e) {
      setError(String(e));
      return undefined;
    }
  }
  async function scan() {
    setBusy(true);
    setProgress(null);
    setNotice("");
    setSelected(new Set());
    await task(async () => {
      const id = await call<number>("scan_folders", {
        options: {
          ...options,
          source: options.source.trim(),
          destination: options.destination?.trim() || null,
        },
      });
      const updated = await reload();
      setScanId(id);
      const saved = updated.find((row) => row.id === id);
      if (saved) setOptions(scanOptions(saved));
      setMigrationState("");
      setStatus("");
      setExtension("");
      setSearch("");
      setProgress(await call<Progress>("get_progress"));
    });
    setBusy(false);
  }
  async function browse(side: "source" | "destination") {
    await task(async () => {
      const path = await chooseFolder();
      if (path) changePath(side, path);
      else if (!desktop)
        setNotice(
          "Folder pickers are available in the desktop app. This browser preview uses example data.",
        );
    });
  }
  async function reviewAction() {
    setBusy(true);
    await task(async () => {
      const request: OperationRequest = {
        scanId: scanId!,
        entryIds: [...selected],
        action,
      };
      const preview = await call<Preview>("preview_operation", { request });
      setReview({ request, preview });
      setConfirmed(false);
      setFlagsApproved(false);
    });
    setBusy(false);
  }
  async function reviewMigration() {
    setBusy(true);
    await task(async () => {
      const plans = await Promise.all(
        (["copy_to_destination", "keep_both"] as const).map(async (action) => {
          const request: OperationRequest = {
            scanId: scanId!,
            entryIds: [...selected],
            action,
          };
          const preview = await call<Preview>("preview_operation", { request });
          return {
            request: { ...request, entryIds: preview.eligibleEntryIds },
            preview,
          };
        }),
      );
      const eligible = plans.filter((plan) => plan.preview.eligible > 0);
      if (!eligible.length)
        throw new Error("Select Ready or Review files to migrate.");
      setMigrationReview({ plans: eligible, selectedCount: selected.size });
      setConfirmed(false);
      setFlagsApproved(false);
    });
    setBusy(false);
  }
  async function executeMigration() {
    if (
      !migrationReview ||
      !confirmed ||
      (migrationReview.plans.some(
        (plan) => plan.preview.reviewEntryIds.length > 0,
      ) &&
        !flagsApproved)
    )
      return;
    const plans = migrationReview.plans;
    setMigrationReview(null);
    setBusy(true);
    setProgress(null);
    setNotice("");
    cancelRequested.current = false;
    await task(async () => {
      let completed = 0;
      for (const plan of plans) {
        if (cancelRequested.current) break;
        await call<number>("execute_operation", {
          request: {
            ...plan.request,
            approvedEntryIds: plan.preview.reviewEntryIds,
          },
        });
        completed++;
        const outcome = await call<Progress>("get_progress");
        setProgress(outcome);
        if (outcome.phase === "cancelled" || outcome.errors > 0) {
          cancelRequested.current = true;
          break;
        }
      }
      await reload();
      setSelected(new Set());
      setNotice(
        `${cancelRequested.current ? "Migration stopped" : "Migration finished"}. ${completed} operation${completed === 1 ? "" : "s"} in History. Review outcomes and optional cleanup there; prepare again to refresh this list.`,
      );
    });
    setBusy(false);
  }
  async function execute() {
    if (!review) return;
    const request = {
      ...review.request,
      approvedEntryIds: flagsApproved ? review.preview.reviewEntryIds : [],
    };
    setReview(null);
    setBusy(true);
    setProgress(null);
    setNotice("");
    await task(async () => {
      await call<number>("execute_operation", { request });
      await reload();
      const outcome = await call<Progress>("get_progress");
      setProgress(outcome);
      setSelected(new Set());
      setNotice(
        `${outcome.message} Open History to review copied files and optional original cleanup.`,
      );
    });
    setBusy(false);
  }
  async function selectMatching() {
    setBusy(true);
    await task(async () => {
      let ids: number[];
      if (page === "migrate" && !migrationState) {
        const groups = await Promise.all(
          ["ready", "review"].map((migrationState) =>
            call<number[]>("matching_entry_ids", {
              filter: { ...filter, migrationState },
            }),
          ),
        );
        ids = groups.flat();
      } else ids = await call<number[]>("matching_entry_ids", { filter });
      const next =
        page === "migrate" ? new Set([...selected, ...ids]) : new Set(ids);
      if (next.size > 10000)
        throw new Error("Select up to 10,000 files. Narrow the filters first.");
      setSelected(next);
    });
    setBusy(false);
  }
  async function loadOlder(kind: "scans" | "operations") {
    await task(async () => {
      if (kind === "scans") {
        const next = await call<Scan[]>("list_scans", { offset: scans.length });
        setScans((old) => [...old, ...next]);
        setMoreScans(next.length === 100);
      } else {
        const next = await call<Operation[]>("list_operations", {
          offset: operations.length,
        });
        setOperations((old) => [...old, ...next]);
        setMoreOperations(next.length === 100);
      }
    });
  }
  async function exportRows() {
    await task(async () => {
      const path = await chooseReport(
        `folderbridge-comparison-${scanId}-${Date.now()}.csv`,
      );
      if (path) {
        setBusy(true);
        setProgress(null);
        let total: number;
        try {
          total = await call<number>("export_scan", { filter, path });
        } finally {
          setBusy(false);
          setProgress(null);
        }
        setNotice(`Exported ${count(total)} matching files to ${path}`);
      } else if (!desktop)
        setNotice("CSV export is available in the desktop app.");
    });
  }
  async function exportStatistics() {
    await task(async () => {
      const path = await chooseReport(
        `folderbridge-size-statistics-${scanId}-${Date.now()}.csv`,
      );
      if (path) {
        await call("export_statistics", { scanId, path });
        setNotice(`Size statistics saved to ${path}`);
      } else if (!desktop)
        setNotice("CSV export is available in the desktop app.");
    });
  }
  async function viewOperation(operation: Operation) {
    await task(async () => {
      const items = await call<OperationItem[]>("get_operation_items", {
        operationId: operation.id,
      });
      setOperationDetail({ operation, items });
    });
  }
  async function refreshOperationItems(operationId: number) {
    const items = await call<OperationItem[]>("get_operation_items", {
      operationId,
    });
    setOperationDetail((current) =>
      current?.operation.id === operationId ? { ...current, items } : current,
    );
    return items;
  }
  async function runCleanup(request: CleanupRequest): Promise<CleanupSummary> {
    setBusy(true);
    setProgress(null);
    try {
      const summary = await call<CleanupSummary>("cleanup_originals", {
        request,
      });
      setProgress(await call<Progress>("get_progress"));
      await reload();
      return summary;
    } finally {
      setBusy(false);
    }
  }
  async function runOriginalRestore(
    operationId: number,
    itemIds: number[],
  ): Promise<CleanupSummary> {
    setBusy(true);
    setProgress(null);
    try {
      const summary = await call<CleanupSummary>("restore_originals", {
        operationId,
        itemIds,
      });
      setProgress(await call<Progress>("get_progress"));
      await reload();
      return summary;
    } finally {
      setBusy(false);
    }
  }
  async function exportOperation(id: number) {
    await task(async () => {
      const path = await chooseReport(
        `folderbridge-operation-${id}-${Date.now()}.csv`,
      );
      if (path) {
        await call("export_operation", { operationId: id, path });
        setNotice(`Operation report saved to ${path}`);
      } else if (!desktop)
        setNotice("CSV export is available in the desktop app.");
    });
  }
  async function restore() {
    if (!restoreReview) return;
    const id = restoreReview.id;
    setRestoreReview(null);
    setOperationDetail(null);
    setBusy(true);
    setProgress(null);
    setNotice("");
    await task(async () => {
      await call("restore_operation", { operationId: id });
      setProgress(await call<Progress>("get_progress"));
      await reload();
      setNotice("Restore finished. Check History for skipped files.");
    });
    setBusy(false);
  }
  function changePath(side: "source" | "destination", value: string) {
    setOptions((v) => ({ ...v, [side]: value }));
    setScanId(null);
  }
  function changeDirection(compareBothWays: boolean) {
    setOptions((old) => ({ ...old, compareBothWays }));
    setStatus("");
    setOffset(0);
    setSelected(new Set());
    setDetails(null);
    setReview(null);
    setData({ entries: [], total: 0 });
    setAction("copy_to_destination");
  }
  function loadScan(s: Scan) {
    setScanId(s.id);
    setOptions(scanOptions(s));
    setPage("compare");
    setStatus("");
    setExtension("");
    setSearch("");
  }
  function toggle(id: number) {
    setSelected((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  function scanOptions(scan: Scan): ScanOptions {
    return withRules({
      ...(scan.options || emptyOptions),
      source: readablePath(scan.source),
      destination: scan.destination ? readablePath(scan.destination) : "",
      verifyContents: scan.verified,
    });
  }
  function openRules(path?: string) {
    if (path && details?.scanId !== scanId) {
      setError(
        "The comparison changed. Open the file again before adding a rule.",
      );
      return;
    }
    setRulesDraft(
      path
        ? {
            ...options,
            rules: {
              ...options.rules,
              excludedPaths: [
                ...new Set([...options.rules.excludedPaths, path]),
              ],
            },
          }
        : options,
    );
    setRulesSection(path ? "Files" : "Types");
    setDetails(null);
    setShowOptions(true);
  }
  function applyRules(updated: ScanOptions) {
    setShowOptions(false);
    if (JSON.stringify(updated) !== JSON.stringify(options)) {
      setOptions(updated);
      setScanId(null);
      setReview(null);
      setMigrationReview(null);
      setNotice("Rules changed. Compare or Prepare again to apply them.");
    }
  }
  const selectable = data.entries.filter(
    (e) => page !== "migrate" || e.migrationState !== "skipped",
  );
  const allPageSelected =
    selectable.length > 0 && selectable.every((e) => selected.has(e.id));
  const comparisonStatuses = sourceOnly
    ? analysis.sourceStatuses
    : analysis.statuses;
  const statusCount = (value: string) =>
    comparisonStatuses.find((s) => s.label === value)?.count || 0;

  return (
    <div className="app-shell">
      <header
        className={desktop ? "app-header desktop-titlebar" : "app-header"}
        data-tauri-drag-region
      >
        <button
          className="brand"
          onClick={() => setPage("compare")}
          aria-label="FolderBridge home"
        >
          <span className="brand-mark">
            <FolderInput size={20} />
          </span>
          FolderBridge
        </button>
        <nav aria-label="Main navigation" className="main-navigation">
          {(["compare", "migrate", "insights", "history"] as const).map(
            (item) => (
              <button
                key={item}
                className={page === item ? "nav-item active" : "nav-item"}
                aria-current={page === item ? "page" : undefined}
                onClick={() => {
                  setPage(item);
                  setStatus("");
                  setMigrationState("");
                  setOffset(0);
                  setSelected(new Set());
                }}
              >
                {pageNames[item]}
              </button>
            ),
          )}
        </nav>
        <div className="header-actions">
          <button
            className="icon-button"
            aria-label="Help"
            title="Help"
            onClick={() => setHelp(true)}
          >
            <CircleHelp size={18} />
          </button>
          {desktop && <WindowControls onError={setError} />}
        </div>
      </header>
      {!desktop && (
        <div className="preview-banner">
          <Info size={13} />
          Browser preview · Sample files
        </div>
      )}
      <div className="main-shell">
        <main aria-label={pageNames[page]}>
          {error && (
            <div className="message error" role="alert">
              <Info size={18} />
              <span>{error}</span>
              <button
                className="icon-button"
                onClick={() => setError("")}
                aria-label="Dismiss error"
              >
                <X size={16} />
              </button>
            </div>
          )}
          {(running || progress?.message || notice) && (
            <section
              className={running ? "job-panel is-running" : "job-panel"}
              aria-label="Job status"
              role="status"
            >
              <div className="job-label">
                {running ? (
                  <LoaderCircle size={15} className="spin" />
                ) : (
                  <Info size={15} />
                )}
                <span className="job-message">
                  {running
                    ? (progress?.phase || "Starting") +
                      "… " +
                      (progress?.processed || 0) +
                      (progress?.total ? " / " + progress.total : "")
                    : notice || progress?.message}
                </span>
                {running ? (
                  <button
                    className="text-button"
                    onClick={() =>
                      void task(async () => {
                        cancelRequested.current = true;
                        await call("cancel_job");
                        setNotice("Cancelling after the current file request…");
                      })
                    }
                  >
                    Cancel
                  </button>
                ) : (
                  <>
                    {progress?.kind === "operation" ||
                    progress?.kind === "restore" ? (
                      <button
                        className="text-button"
                        onClick={() => setPage("history")}
                      >
                        Details
                      </button>
                    ) : null}
                    <button
                      className="icon-button"
                      aria-label="Dismiss job status"
                      onClick={() => {
                        setNotice("");
                        setProgress(null);
                      }}
                    >
                      <X size={14} />
                    </button>
                  </>
                )}
              </div>
              {running && (
                <ProgressBar
                  value={
                    progress?.total
                      ? (progress.processed / progress.total) * 100
                      : null
                  }
                />
              )}
            </section>
          )}

          {(page === "compare" || page === "migrate") && (
            <>
              <section className="folder-setup" aria-label="Folders to compare">
                <div className="folder-inputs">
                  {(["source", "destination"] as const).map((side, i) => (
                    <div className="folder-field" key={side}>
                      <div className="folder-field-heading">
                        <label htmlFor={`${side}-path`}>
                          {i === 0 ? "Source folder" : "Destination folder"}
                        </label>
                      </div>
                      <div className="path-input">
                        <Folder size={19} />
                        <input
                          id={`${side}-path`}
                          value={options[side] || ""}
                          placeholder={
                            i === 0
                              ? "Choose source folder"
                              : page === "migrate"
                                ? "Choose destination folder"
                                : "Optional for inventory"
                          }
                          onChange={(e) => changePath(side, e.target.value)}
                          disabled={running}
                          spellCheck={false}
                        />
                        <button
                          onClick={() => void browse(side)}
                          aria-label={`Browse ${side} folder`}
                          disabled={running}
                        >
                          <FolderOpen size={17} />
                          Browse
                        </button>
                      </div>
                    </div>
                  ))}
                  <button
                    className="swap-button"
                    onClick={() => {
                      setOptions((v) => ({
                        ...v,
                        source: v.destination || "",
                        destination: v.source,
                      }));
                      setScanId(null);
                    }}
                    disabled={running || !options.destination}
                    aria-label="Swap source and destination"
                    title="Swap source and destination"
                  >
                    <ArrowLeftRight size={18} />
                  </button>
                </div>
                <div className="setup-toolbar">
                  <div className="setup-actions">
                    <button
                      className="text-button"
                      onClick={() => setShowPairs(true)}
                      disabled={running}
                    >
                      <Folder size={14} />
                      Saved folders
                      <ChevronDown size={12} />
                    </button>
                    <button
                      className="text-button"
                      onClick={() => setSavePair(true)}
                      disabled={running || !options.source}
                    >
                      <Bookmark size={14} />
                      Save pair
                    </button>
                  </div>
                  {page === "compare" && (
                    <label
                      className="direction-toggle"
                      title="Include files found only in the destination. Switch views without rescanning."
                    >
                      <input
                        type="checkbox"
                        checked={options.compareBothWays}
                        onChange={(event) =>
                          changeDirection(event.target.checked)
                        }
                        disabled={running || !options.destination?.trim()}
                      />
                      Compare both ways
                    </label>
                  )}
                  <div className="setup-actions">
                    <button
                      className="text-button"
                      onClick={() => openRules()}
                      disabled={running}
                      aria-haspopup="dialog"
                    >
                      <Settings2 size={14} />
                      Rules
                    </button>
                    <button
                      className="button primary"
                      onClick={() => void scan()}
                      disabled={
                        running ||
                        !options.source.trim() ||
                        (page === "migrate" && !options.destination?.trim())
                      }
                    >
                      {page === "migrate"
                        ? "Prepare"
                        : options.destination?.trim()
                          ? "Compare"
                          : "Scan inventory"}
                      <ArrowRight size={15} />
                    </button>
                  </div>
                </div>
                <p className="rules-summary" title={rulesSummary(options)}>
                  {sourceOnly ? "Source to destination · " : ""}
                  {rulesSummary(options)}
                </p>
              </section>

              {page === "migrate" && current && !current.options && (
                <p className="rules-hint">
                  This older comparison has no saved rules. Prepare again before
                  migrating.
                </p>
              )}
              {current && (
                <div className="comparison-meta">
                  <span>
                    <Clock3 size={13} />
                    {date(current.startedAt)}
                    <span className="meta-divider">·</span>
                    {count(
                      sourceOnly
                        ? comparisonStatuses.reduce(
                            (total, group) => total + group.count,
                            0,
                          )
                        : current.files,
                    )}{" "}
                    paths
                    <span className="meta-divider">·</span>
                    {current.verified ? "Verification on" : "Quick comparison"}
                  </span>
                </div>
              )}
              {current && current.state !== "complete" && (
                <div className="message error">
                  <Info size={18} />
                  <span>
                    Scan {current.state} · {count(current.errors)} issues.
                    Resolve and rescan to enable actions.
                  </span>
                  <button
                    className="text-button"
                    onClick={() => setStatus("error")}
                  >
                    Show issues
                  </button>
                </div>
              )}
              {current && page === "migrate" && (
                <div
                  className="comparison-summary migration-summary"
                  aria-label="Migration summary"
                >
                  {(["ready", "review", "skipped"] as const).map((state) => {
                    const label = state[0].toUpperCase() + state.slice(1);
                    const total =
                      analysis.migrationStates.find(
                        (group) => group.label === state,
                      )?.count || 0;
                    return (
                      <button
                        key={state}
                        className={`summary-item ${migrationState === state ? "selected" : ""}`}
                        aria-label={`${label} ${total}`}
                        aria-pressed={migrationState === state}
                        onClick={() => {
                          setMigrationState(
                            migrationState === state ? "" : state,
                          );
                          setStatus("");
                        }}
                      >
                        <span className={`summary-symbol symbol-${state}`}>
                          {state === "ready" ? (
                            <Check size={17} />
                          ) : state === "review" ? (
                            <Flag size={17} />
                          ) : (
                            <X size={17} />
                          )}
                        </span>
                        <span>
                          {label}
                          <strong>{count(total)}</strong>
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
              {current && page === "compare" && (
                <div
                  className={`comparison-summary ${sourceOnly ? "one-way-summary" : ""}`}
                  aria-label="Comparison summary"
                >
                  {(
                    [
                      {
                        id: "source_only",
                        label: "Source only",
                        icon: ArrowRight,
                      },
                      { id: "different", label: "Different", icon: Layers2 },
                      { id: "identical", label: "Identical", icon: Check },
                      {
                        id: "destination_only",
                        label: "Destination only",
                        icon: Undo2,
                      },
                    ] as const
                  )
                    .filter(
                      (item) => !sourceOnly || item.id !== "destination_only",
                    )
                    .map((item) => (
                      <button
                        key={item.id}
                        className={`summary-item ${status === item.id ? "selected" : ""}`}
                        onClick={() =>
                          setStatus(status === item.id ? "" : item.id)
                        }
                        aria-pressed={status === item.id}
                      >
                        <span className={`summary-symbol symbol-${item.id}`}>
                          <item.icon size={17} />
                        </span>
                        <span>
                          {item.label}
                          <strong>{count(statusCount(item.id))}</strong>
                        </span>
                        <ChevronRight size={14} />
                      </button>
                    ))}
                </div>
              )}

              <section
                className="results-panel"
                aria-label={
                  page === "migrate"
                    ? "Migration files"
                    : "File comparison results"
                }
              >
                <div className="results-tools">
                  <label className="search-field">
                    <Search size={17} />
                    <input
                      aria-label="Search files"
                      value={search}
                      onChange={(e) => setSearch(e.target.value)}
                      placeholder="Search files…"
                      disabled={!current}
                    />
                    {search && (
                      <button
                        className="icon-button"
                        onClick={() => setSearch("")}
                        aria-label="Clear search"
                      >
                        <X size={13} />
                      </button>
                    )}
                  </label>
                  <label className="status-select">
                    <span className="sr-only">
                      {page === "migrate"
                        ? "Filter by decision"
                        : "Filter by status"}
                    </span>
                    <select
                      value={page === "migrate" ? migrationState : status}
                      onChange={(e) =>
                        page === "migrate"
                          ? setMigrationState(e.target.value)
                          : setStatus(e.target.value)
                      }
                      disabled={!current}
                    >
                      <option value="">
                        {page === "migrate" ? "All decisions" : "All statuses"}
                      </option>
                      {Object.entries(
                        page === "migrate"
                          ? {
                              ready: "Ready",
                              review: "Review",
                              skipped: "Skipped",
                            }
                          : statuses,
                      )
                        .filter(
                          ([key]) => !sourceOnly || key !== "destination_only",
                        )
                        .map(([key, label]) => (
                          <option key={key} value={key}>
                            {label}
                          </option>
                        ))}
                    </select>
                  </label>
                  <button
                    className="text-button"
                    onClick={() => setShowFilters(!showFilters)}
                    aria-expanded={showFilters}
                  >
                    <SlidersHorizontal size={15} />
                    Filters
                    {(status || extension || minSize) && (
                      <span className="filter-dot" />
                    )}
                  </button>
                  <button
                    className="button secondary compact"
                    onClick={() => void exportRows()}
                    disabled={!current || running}
                  >
                    <ArrowDownToLine size={14} />
                    Export CSV
                  </button>
                </div>
                {!current ? (
                  <EmptyState
                    icon={<FolderInput size={27} />}
                    title={
                      page === "migrate"
                        ? "Prepare your migration."
                        : "Choose folders to compare."
                    }
                  >
                    <p>
                      {page === "migrate"
                        ? "Choose both folders, set rules, then Prepare. No files change until you confirm."
                        : "Leave destination empty for an inventory."}
                    </p>
                  </EmptyState>
                ) : (
                  <>
                    <div
                      ref={tableRef}
                      className={`table-scroll file-table ${page === "migrate" ? "migration-table" : ""} ${loadingRows ? "loading" : ""}`}
                    >
                      <table>
                        <thead>
                          <tr>
                            <th className="checkbox-cell">
                              <input
                                type="checkbox"
                                aria-label="Select all files on this page"
                                checked={allPageSelected}
                                disabled={!selectable.length || running}
                                onChange={() =>
                                  setSelected((old) => {
                                    const next = new Set(old);
                                    selectable.forEach((e) =>
                                      allPageSelected
                                        ? next.delete(e.id)
                                        : next.add(e.id),
                                    );
                                    return next;
                                  })
                                }
                              />
                            </th>
                            <th>File</th>
                            <th>
                              {page === "migrate" ? "Decision" : "Status"}
                            </th>
                            <th className="numeric">
                              {page === "migrate" ? "Size" : "Source"}
                            </th>
                            <th
                              className={
                                page === "migrate" ? "reason-cell" : "numeric"
                              }
                            >
                              {page === "migrate" ? "Reason" : "Destination"}
                            </th>
                            <th className="table-arrow">
                              <span className="sr-only">Explorer</span>
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {data.entries.map((entry) => (
                            <tr
                              key={entry.id}
                              className={
                                selected.has(entry.id) ? "selected-row" : ""
                              }
                            >
                              <td className="checkbox-cell">
                                <input
                                  type="checkbox"
                                  aria-label={`Select ${entry.relativePath}`}
                                  checked={selected.has(entry.id)}
                                  onChange={() => toggle(entry.id)}
                                  disabled={
                                    running ||
                                    (page === "migrate" &&
                                      entry.migrationState === "skipped")
                                  }
                                />
                              </td>
                              <td>
                                <button
                                  className="file-name"
                                  title={entry.relativePath}
                                  onClick={() => {
                                    setRevealMessage("");
                                    setDetails(entry);
                                  }}
                                >
                                  <FileIcon extension={entry.extension} />
                                  <span>
                                    <strong>
                                      {basename(entry.relativePath)}
                                    </strong>
                                    <small>
                                      {entry.relativePath.includes("/")
                                        ? entry.relativePath.slice(
                                            0,
                                            entry.relativePath.lastIndexOf("/"),
                                          )
                                        : "Root folder"}
                                    </small>
                                  </span>
                                </button>
                              </td>
                              <td>
                                {page === "migrate" ? (
                                  <span
                                    className={`status-badge decision-${entry.migrationState}`}
                                  >
                                    {entry.migrationState[0].toUpperCase() +
                                      entry.migrationState.slice(1)}
                                  </span>
                                ) : (
                                  <>
                                    <StatusBadge status={entry.status} />
                                    {(entry.ruleReview ||
                                      entry.status === "different") && (
                                      <Flag
                                        className="rule-flag"
                                        size={12}
                                        aria-label="Needs review"
                                      />
                                    )}
                                  </>
                                )}
                              </td>
                              <td className="numeric">
                                {bytes(
                                  page === "migrate"
                                    ? (entry.sourceSize ??
                                        entry.destinationSize)
                                    : entry.sourceSize,
                                )}
                              </td>
                              <td
                                className={
                                  page === "migrate" ? "reason-cell" : "numeric"
                                }
                                title={
                                  page === "migrate"
                                    ? entry.migrationReason
                                    : undefined
                                }
                              >
                                {page === "migrate"
                                  ? entry.migrationReason
                                  : bytes(entry.destinationSize)}
                              </td>
                              <td className="table-arrow">
                                <button
                                  className="icon-button"
                                  aria-label={`Show ${entry.relativePath} in Explorer`}
                                  title="Show in Explorer"
                                  onClick={() =>
                                    void reveal(
                                      entry,
                                      entry.sourceRelative
                                        ? "source"
                                        : "destination",
                                    )
                                  }
                                >
                                  <FolderOpen size={16} />
                                </button>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                    {!data.entries.length && (
                      <EmptyState
                        icon={<Search size={27} />}
                        title="No files match this view."
                      >
                        <p>Try a different search or clear the filters.</p>
                        <button
                          className="text-button"
                          onClick={() => {
                            setSearch("");
                            setStatus("");
                            setExtension("");
                            setMinSize("");
                            setMigrationState("");
                          }}
                        >
                          Clear all filters
                        </button>
                      </EmptyState>
                    )}
                    <div className="table-footer">
                      <span>
                        {data.total
                          ? `${count(offset + 1)}–${count(Math.min(offset + pageSize, data.total))} of ${count(data.total)} files`
                          : "0 files"}
                        {selected.size > 0 &&
                          ` · ${count(selected.size)} selected`}
                      </span>
                      <div>
                        <button
                          className="text-button"
                          onClick={() => void selectMatching()}
                          disabled={
                            running ||
                            !data.total ||
                            data.total > 10000 ||
                            (page === "migrate" && migrationState === "skipped")
                          }
                          title="Select all matching files, up to 10,000"
                        >
                          {page === "migrate" && !migrationState
                            ? "Select eligible"
                            : `Select all ${count(data.total)}`}
                        </button>
                        <button
                          className="icon-button"
                          onClick={() =>
                            setOffset(Math.max(0, offset - pageSize))
                          }
                          disabled={offset === 0}
                          aria-label="Previous page"
                        >
                          <ChevronLeft size={16} />
                        </button>
                        <button
                          className="icon-button"
                          onClick={() => setOffset(offset + pageSize)}
                          disabled={offset + pageSize >= data.total}
                          aria-label="Next page"
                        >
                          <ChevronRight size={16} />
                        </button>
                      </div>
                    </div>
                  </>
                )}
              </section>
            </>
          )}

          {page === "insights" &&
            (current ? (
              <>
                <div className="insights-heading">
                  <div>
                    <Folder size={18} />
                    <strong>{basename(current.source)}</strong>
                    <ArrowRight size={15} />
                    <span>
                      {current.destination
                        ? basename(current.destination)
                        : "Inventory"}
                    </span>
                  </div>
                  <button
                    className="button secondary compact"
                    onClick={() => void exportRows()}
                  >
                    <ArrowDownToLine size={14} />
                    Export inventory
                  </button>
                </div>
                <div className="storage-totals">
                  <div>
                    <span>Source</span>
                    <strong>{bytes(analysis.sourceBytes)}</strong>
                    <small>Logical file size</small>
                  </div>
                  <div>
                    <span>Destination</span>
                    <strong>{bytes(analysis.destinationBytes)}</strong>
                    <small>Logical file size</small>
                  </div>
                  <div>
                    <span>Verified duplicates</span>
                    <strong>{bytes(analysis.duplicateBytes)}</strong>
                    <small>Quarantine preserves this data</small>
                  </div>
                </div>
                <div className="view-switch" aria-label="Storage view">
                  <button
                    aria-pressed={storageView === "types"}
                    onClick={() => setStorageView("types")}
                  >
                    File types
                  </button>
                  <button
                    aria-pressed={storageView === "folders"}
                    onClick={() => setStorageView("folders")}
                  >
                    Folders
                  </button>
                  <button
                    aria-pressed={storageView === "statistics"}
                    onClick={() => setStorageView("statistics")}
                  >
                    Statistics
                  </button>
                </div>
                <div
                  className="analysis-grid"
                  hidden={storageView === "statistics"}
                >
                  {storageView === "types" && (
                    <Distribution
                      title="Storage by file type"
                      groups={analysis.extensions}
                      onSelect={(value) => {
                        setExtension(value === "(no extension)" ? "" : value);
                        setPage("compare");
                      }}
                    />
                  )}
                  {storageView === "folders" && (
                    <Distribution
                      title="Storage by folder"
                      groups={analysis.folders}
                      onSelect={(value) => {
                        setSearch(value === "(root)" ? "" : `${value}/`);
                        setPage("compare");
                      }}
                    />
                  )}
                </div>
                <section
                  className="results-panel size-statistics"
                  hidden={storageView !== "statistics"}
                >
                  <div className="results-heading">
                    <h2>File-size statistics</h2>
                    <button
                      className="button secondary compact"
                      onClick={() => void exportStatistics()}
                    >
                      <ArrowDownToLine size={14} />
                      Export statistics
                    </button>
                  </div>
                  <div className="table-scroll">
                    <table>
                      <thead>
                        <tr>
                          <th>File type</th>
                          <th className="numeric">Files</th>
                          <th className="numeric">Mean</th>
                          <th className="numeric">Median</th>
                          <th className="numeric">Lower quartile</th>
                          <th className="numeric">Upper quartile</th>
                        </tr>
                      </thead>
                      <tbody>
                        {analysis.sizeStatistics.map((row) => (
                          <tr key={row.extension}>
                            <td>{row.extension || "(no extension)"}</td>
                            <td className="numeric">{count(row.count)}</td>
                            <td className="numeric">{bytes(row.mean)}</td>
                            <td className="numeric">{bytes(row.median)}</td>
                            <td className="numeric">
                              {bytes(row.lowerQuartile)}
                            </td>
                            <td className="numeric">
                              {bytes(row.upperQuartile)}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>
                <p className="workspace-footnote">
                  <Info size={14} />
                  Groups count each relative path once, using source size when
                  present. Online-only files can occupy less physical disk
                  space.
                </p>
              </>
            ) : (
              <EmptyState
                icon={<BarChart3 size={30} />}
                title="No scan selected."
              >
                <p>
                  Run an inventory or comparison to see file types, folder
                  sizes, and verified duplicates.
                </p>
                <button
                  className="button primary"
                  onClick={() => setPage("compare")}
                >
                  Choose folders
                  <ArrowRight size={16} />
                </button>
              </EmptyState>
            ))}

          {page === "history" && (
            <div className="history-layout">
              <div className="view-switch" aria-label="History view">
                <button
                  aria-pressed={historyView === "operations"}
                  onClick={() => setHistoryView("operations")}
                >
                  Operations
                </button>
                <button
                  aria-pressed={historyView === "scans"}
                  onClick={() => setHistoryView("scans")}
                >
                  Comparisons
                </button>
              </div>
              <section
                className="history-section"
                hidden={historyView !== "operations"}
              >
                <div className="section-heading">
                  <h2>
                    File operations
                    <span className="subtle-count">{operations.length}</span>
                  </h2>
                  <span>Stored on this device</span>
                </div>
                {operations.length ? (
                  operations.map((operation) => (
                    <div className="history-row" key={operation.id}>
                      <div className="history-symbol">
                        {operation.action === "quarantine_destination" ? (
                          <FolderInput size={20} />
                        ) : (
                          <Copy size={19} />
                        )}
                      </div>
                      <button
                        className="history-description"
                        onClick={() => void viewOperation(operation)}
                      >
                        <strong>
                          {actions[operation.action]?.label || operation.action}
                        </strong>
                        <span>
                          {date(operation.startedAt)} · {operation.completed}{" "}
                          completed · {operation.skipped} skipped
                        </span>
                      </button>
                      <StatusBadge status={operation.state} />
                      <button
                        className="icon-button"
                        onClick={() => void viewOperation(operation)}
                        aria-label={`View operation ${operation.id}`}
                      >
                        <ChevronRight size={17} />
                      </button>
                    </div>
                  ))
                ) : (
                  <EmptyState
                    icon={<History size={28} />}
                    title="No operations yet."
                  >
                    <p>
                      Your copy, recovery, and quarantine operations will appear
                      here.
                    </p>
                  </EmptyState>
                )}
                {moreOperations && (
                  <button
                    className="text-button history-more"
                    onClick={() => void loadOlder("operations")}
                  >
                    Load older operations
                  </button>
                )}
              </section>
              <section
                className="history-section"
                hidden={historyView !== "scans"}
              >
                <div className="section-heading">
                  <h2>
                    Comparisons
                    <span className="subtle-count">{scans.length}</span>
                  </h2>
                  <span>Newest first</span>
                </div>
                {scans.length ? (
                  scans.map((s) => (
                    <div className="history-row" key={s.id}>
                      <div className="history-symbol">
                        <ArrowLeftRight size={19} />
                      </div>
                      <button
                        className="history-description"
                        onClick={() => loadScan(s)}
                        disabled={running}
                      >
                        <strong>
                          {basename(s.source)} <ArrowRight size={13} />{" "}
                          {s.destination
                            ? basename(s.destination)
                            : "Inventory"}
                        </strong>
                        <span>
                          {date(s.startedAt)} · {count(s.files)} paths ·{" "}
                          {s.errors} issues
                        </span>
                      </button>
                      <StatusBadge status={s.state} />
                      <button
                        className="icon-button"
                        onClick={() => loadScan(s)}
                        disabled={running}
                        aria-label={`Open comparison ${s.id}`}
                      >
                        <ChevronRight size={17} />
                      </button>
                    </div>
                  ))
                ) : (
                  <p className="section-empty">
                    Your first comparison will appear here.
                  </p>
                )}
                {moreScans && (
                  <button
                    className="text-button history-more"
                    onClick={() => void loadOlder("scans")}
                  >
                    Load older comparisons
                  </button>
                )}
              </section>
            </div>
          )}
        </main>

        <AnimatePresence>
          {selected.size > 0 && (page === "compare" || page === "migrate") && (
            <motion.div
              className="selection-tray"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
            >
              <div className="selection-count">
                <span>{count(selected.size)}</span> selected
                <button
                  className="icon-button"
                  onClick={() => setSelected(new Set())}
                  aria-label="Clear selection"
                >
                  <X size={15} />
                </button>
              </div>
              {page === "migrate" ? (
                <span className="migration-tray-note">
                  Verified copies · Keep existing versions
                </span>
              ) : (
                <label>
                  <span className="sr-only">Action for selected files</span>
                  <select
                    value={action}
                    onChange={(e) => setAction(e.target.value as Action)}
                    disabled={running}
                  >
                    {Object.entries(actions)
                      .filter(
                        ([key]) => !sourceOnly || key !== "copy_to_source",
                      )
                      .map(([key, value]) => (
                        <option value={key} key={key}>
                          {value.label}
                        </option>
                      ))}
                  </select>
                </label>
              )}
              <button
                className="button primary"
                onClick={() =>
                  void (page === "migrate" ? reviewMigration() : reviewAction())
                }
                disabled={
                  running ||
                  current?.state !== "complete" ||
                  (page === "migrate" && !current?.options)
                }
              >
                {page === "migrate" ? "Review migration" : "Review"}
                <ArrowRight size={15} />
              </button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {showOptions && (
        <RulesDialog
          options={rulesDraft || options}
          extensions={analysis.extensions.map((group) => group.label)}
          initialSection={rulesSection}
          onApply={applyRules}
          onClose={() => setShowOptions(false)}
        />
      )}
      {showFilters && (
        <Dialog title="Filters" onClose={() => setShowFilters(false)}>
          {" "}
          <div className="advanced-filters">
            <label>
              File type
              <select
                value={extension}
                onChange={(e) => setExtension(e.target.value)}
              >
                <option value="">All file types</option>
                {analysis.extensions
                  .filter((g) => g.label !== "(no extension)")
                  .map((g) => (
                    <option key={g.label}>{g.label}</option>
                  ))}
              </select>
            </label>
            <label>
              Minimum size (MB)
              <input
                type="number"
                min="0"
                value={minSize}
                onChange={(e) => setMinSize(e.target.value)}
                placeholder="0"
              />
            </label>
            <button
              className="text-button"
              onClick={() => {
                setStatus("");
                setExtension("");
                setMinSize("");
                setSearch("");
              }}
            >
              Clear filters
            </button>
            <button
              className="text-button"
              onClick={() => {
                setExtension(".accdb");
                setStatus("");
              }}
            >
              Access databases
            </button>
          </div>
          <div className="dialog-actions">
            <button
              className="button primary"
              onClick={() => setShowFilters(false)}
            >
              Done
            </button>
          </div>
        </Dialog>
      )}
      {showPairs && (
        <Dialog title="Saved folders" onClose={() => setShowPairs(false)}>
          {" "}
          <div className="saved-pairs">
            {pairs.length ? (
              pairs.map((pair) => (
                <div className="saved-pair" key={pair.id}>
                  <button
                    onClick={() => {
                      setOptions(withRules(pair.options));
                      setShowPairs(false);
                      setScanId(null);
                      setNotice(
                        `Loaded ${pair.name}. Compare to see current files.`,
                      );
                    }}
                    disabled={running}
                  >
                    <Folder size={16} />
                    <span>{pair.name}</span>
                  </button>
                  <button
                    className="icon-button"
                    onClick={() =>
                      void task(async () => {
                        await call("delete_pair", { id: pair.id });
                        await reload();
                      })
                    }
                    aria-label={`Remove saved pair ${pair.name}`}
                    title="Remove saved shortcut"
                  >
                    <X size={13} />
                  </button>
                </div>
              ))
            ) : (
              <p className="sidebar-hint">No saved folders yet.</p>
            )}
          </div>
          <div className="dialog-actions">
            <button
              className="button primary"
              onClick={() => {
                setShowPairs(false);
                setSavePair(true);
              }}
              disabled={!options.source}
            >
              Save current pair
            </button>
          </div>
        </Dialog>
      )}

      {savePair && (
        <Dialog title="Save folder pair" onClose={() => setSavePair(false)}>
          <p className="dialog-intro">
            Save these paths, rules, and scan settings.
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void task(async () => {
                await call("save_pair", {
                  name: pairName,
                  options,
                });
                await reload();
                setSavePair(false);
                setPairName("");
                setNotice("Folder pair saved.");
              });
            }}
          >
            <label className="field-label">
              Name
              <input
                autoFocus
                className="text-input"
                placeholder="e.g. Shared documents → Team library"
                value={pairName}
                onChange={(e) => setPairName(e.target.value)}
                required
                maxLength={100}
              />
            </label>
            <dl className="detail-list">
              <dt>Source</dt>
              <dd>{options.source || "Not selected"}</dd>
              <dt>Destination</dt>
              <dd>{options.destination || "Inventory only"}</dd>
            </dl>
            <div className="dialog-actions">
              <button
                type="button"
                className="button secondary"
                onClick={() => setSavePair(false)}
              >
                Cancel
              </button>
              <button
                className="button primary"
                disabled={!pairName.trim() || !options.source}
              >
                <Bookmark size={15} />
                Save pair
              </button>
            </div>
          </form>
        </Dialog>
      )}
      {details && (
        <Dialog
          title={basename(details.relativePath)}
          onClose={() => setDetails(null)}
          className="file-dialog"
        >
          <div className="file-detail-status">
            <StatusBadge status={details.status} />
            <span>{details.extension || "No extension"}</span>
          </div>
          <dl className="detail-list">
            <dt>Locations</dt>
            <dd className="reveal-locations">
              {(["source", "destination"] as const).map((side) => {
                const root = current?.[side];
                if (!root) return null;
                const relative =
                  (side === "source"
                    ? details.sourceRelative
                    : details.destinationRelative) || details.relativePath;
                const exists =
                  side === "source"
                    ? details.sourceRelative !== null
                    : details.destinationRelative !== null;
                return (
                  <div key={side}>
                    <code>
                      {readablePath(root)}/{relative}
                    </code>
                    <button
                      className="text-button"
                      onClick={() => void reveal(details, side)}
                    >
                      <FolderOpen size={14} />
                      {exists
                        ? "Show " + side + " in Explorer"
                        : "Open " + side + " folder"}
                    </button>
                  </div>
                );
              })}
            </dd>
            <dt>Migration</dt>
            <dd>
              {details.migrationReason ||
                "Prepare again to classify this file."}
            </dd>
            {details.ruleReason &&
              details.ruleReason !== details.migrationReason && (
                <>
                  <dt>Rule</dt>
                  <dd>{details.ruleReason}</dd>
                </>
              )}
            <dt>Relative path</dt>
            <dd>{details.relativePath}</dd>
            <dt>Source size</dt>
            <dd>{bytes(details.sourceSize)}</dd>
            <dt>Destination size</dt>
            <dd>{bytes(details.destinationSize)}</dd>
            {details.issue && (
              <>
                <dt>Issue</dt>
                <dd className="error-text">{details.issue}</dd>
              </>
            )}
          </dl>
          <div className="file-rule-actions">
            <button
              className="text-button"
              disabled={running}
              onClick={() => openRules(details.relativePath)}
            >
              Skip this file
            </button>
            {details.relativePath.includes("/") && (
              <button
                className="text-button"
                disabled={running}
                onClick={() =>
                  openRules(
                    details.relativePath.slice(
                      0,
                      details.relativePath.lastIndexOf("/") + 1,
                    ),
                  )
                }
              >
                Skip this folder
              </button>
            )}
          </div>
          <details className="metadata-details">
            <summary>Dates, owners & hashes</summary>
            <dl className="detail-list">
              {" "}
              <dt>Source modified</dt>
              <dd>
                {details.sourceModified
                  ? date(details.sourceModified / 1000000)
                  : "N/A"}
              </dd>
              <dt>Destination modified</dt>
              <dd>
                {details.destinationModified
                  ? date(details.destinationModified / 1000000)
                  : "N/A"}
              </dd>
              {details.owner && (
                <>
                  <dt>Owner identifier</dt>
                  <dd>{details.owner}</dd>
                </>
              )}
              <dt>Source SHA-256</dt>
              <dd className="hash">{details.sourceHash || "Not calculated"}</dd>
              <dt>Destination SHA-256</dt>
              <dd className="hash">
                {details.destinationHash || "Not calculated"}
              </dd>
            </dl>
          </details>

          {revealMessage && (
            <p className="dialog-note" role="status">
              {revealMessage}
            </p>
          )}
          <p className="dialog-note">
            Matching file sizes alone do not establish identical contents. Every
            file is checked again before an action.
          </p>
          <div className="dialog-actions">
            <button
              className="button secondary"
              onClick={() => setDetails(null)}
            >
              Close
            </button>
            <button
              className="button primary"
              onClick={() => {
                toggle(details.id);
                setDetails(null);
              }}
              disabled={
                running ||
                (page === "migrate" && details.migrationState === "skipped")
              }
            >
              {selected.has(details.id) ? "Deselect file" : "Select file"}
            </button>
          </div>
        </Dialog>
      )}
      {review && (
        <Dialog
          title={actions[review.request.action].label}
          onClose={() => setReview(null)}
        >
          <div className="review-summary">
            <ShieldCheck size={24} />
            <div>
              <strong>{count(review.preview.eligible)} eligible files</strong>
              <span>
                {bytes(review.preview.bytes)} · {review.preview.skipped}{" "}
                selected files will be skipped
              </span>
            </div>
          </div>
          <p className="dialog-intro">{review.preview.description}</p>
          <dl className="detail-list">
            <dt>Source</dt>
            <dd>{current && readablePath(current.source)}</dd>
            <dt>Destination</dt>
            <dd>{current?.destination && readablePath(current.destination)}</dd>
          </dl>
          <p className="dialog-note">
            Files changed since the scan, occupied destinations, and unreadable
            files are skipped. Review outcomes in History.
          </p>
          {review.preview.reviewEntryIds.length > 0 && (
            <label className="check-label confirmation">
              <input
                type="checkbox"
                checked={flagsApproved}
                onChange={(e) => setFlagsApproved(e.target.checked)}
              />
              <span>
                Approve {review.preview.reviewEntryIds.length} flagged files
              </span>
            </label>
          )}
          <label className="check-label confirmation">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            <span>I reviewed the folders and selected action.</span>
          </label>
          <div className="dialog-actions">
            <button
              className="button secondary"
              onClick={() => setReview(null)}
            >
              Cancel
            </button>
            <button
              className="button primary"
              onClick={() => void execute()}
              disabled={
                !confirmed ||
                !review.preview.eligible ||
                (review.preview.reviewEntryIds.length > 0 && !flagsApproved)
              }
            >
              {actions[review.request.action].verb}
              <ArrowRight size={15} />
            </button>
          </div>
        </Dialog>
      )}
      {migrationReview &&
        (() => {
          const eligible = migrationReview.plans.reduce(
            (n, plan) => n + plan.preview.eligible,
            0,
          );
          const size = migrationReview.plans.reduce(
            (n, plan) => n + plan.preview.bytes,
            0,
          );
          const flagged = migrationReview.plans.reduce(
            (n, plan) => n + plan.preview.reviewEntryIds.length,
            0,
          );
          return (
            <Dialog
              title="Review migration"
              onClose={() => setMigrationReview(null)}
            >
              <div className="review-summary">
                <ShieldCheck size={24} />
                <div>
                  <strong>
                    {count(eligible)} files · {bytes(size)}
                  </strong>
                  <span>
                    {migrationReview.selectedCount - eligible} selected files
                    skipped
                  </span>
                </div>
              </div>
              <dl className="detail-list">
                <dt>Source</dt>
                <dd>{current && readablePath(current.source)}</dd>
                <dt>Destination</dt>
                <dd>
                  {current?.destination && readablePath(current.destination)}
                </dd>
                {migrationReview.plans.map((plan) => (
                  <div className="detail-contents" key={plan.request.action}>
                    <dt>
                      {plan.request.action === "keep_both"
                        ? "Keep both"
                        : "Copy missing"}
                    </dt>
                    <dd>{count(plan.preview.eligible)} files</dd>
                  </div>
                ))}
              </dl>
              <p className="dialog-note">
                Originals stay in place. Conflicts get a new filename. Changed
                or unavailable files are skipped. Cleanup is a separate action
                in History.
              </p>
              {flagged > 0 && (
                <label className="check-label confirmation">
                  <input
                    type="checkbox"
                    checked={flagsApproved}
                    onChange={(e) => setFlagsApproved(e.target.checked)}
                  />
                  <span>Approve {flagged} flagged files</span>
                </label>
              )}
              <label className="check-label confirmation">
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                <span>I reviewed the folders and selected action.</span>
              </label>
              <div className="dialog-actions">
                <button
                  className="button secondary"
                  onClick={() => setMigrationReview(null)}
                >
                  Cancel
                </button>
                <button
                  className="button primary"
                  disabled={!confirmed || (flagged > 0 && !flagsApproved)}
                  onClick={() => void executeMigration()}
                >
                  Start migration
                  <ArrowRight size={15} />
                </button>
              </div>
            </Dialog>
          );
        })()}
      {operationDetail && (
        <OperationDetails
          operation={operationDetail.operation}
          items={operationDetail.items}
          running={running}
          onClose={() => setOperationDetail(null)}
          onExport={() => void exportOperation(operationDetail.operation.id)}
          onQuarantineRestore={() => {
            setRestoreReview(operationDetail.operation);
            setOperationDetail(null);
          }}
          onCleanup={runCleanup}
          onRestore={(itemIds) =>
            runOriginalRestore(operationDetail.operation.id, itemIds)
          }
          onRefresh={() => refreshOperationItems(operationDetail.operation.id)}
        />
      )}
      {restoreReview && (
        <Dialog
          title="Restore quarantined files"
          onClose={() => setRestoreReview(null)}
        >
          <p className="dialog-intro">
            Return files from operation {restoreReview.id} to their original
            destination folders. Files are verified again; occupied locations
            and changed contents are retained for review.
          </p>
          <div className="dialog-actions">
            <button
              className="button secondary"
              onClick={() => setRestoreReview(null)}
            >
              Cancel
            </button>
            <button className="button primary" onClick={() => void restore()}>
              <Undo2 size={15} />
              Restore files
            </button>
          </div>
        </Dialog>
      )}
      {help && (
        <Dialog title="Help" onClose={() => setHelp(false)}>
          <div className="help-steps">
            <div>
              <FolderOpen size={20} />
              <section>
                <h3>Choose your folders</h3>
                <p>
                  Use local folders, connected network shares, or libraries
                  synced with OneDrive. Leave the destination empty for an
                  inventory.
                </p>
              </section>
            </div>
            <div>
              <FileSearch size={20} />
              <section>
                <h3>Review the differences</h3>
                <p>
                  Compare checks source files at the same relative paths in the
                  destination. Extra destination files are hidden by default.
                  Turn on Compare both ways to include them and recover files to
                  the source. Identical means verified contents; different files
                  keep both versions.
                </p>
              </section>
            </div>
            <div>
              <ShieldCheck size={20} />
              <section>
                <h3>Make a deliberate move</h3>
                <p>
                  Select files, choose an action, and review it. Copies preserve
                  the originals. After a copy, open History to review optional
                  original cleanup. The app checks the copy again before
                  cleanup.
                </p>
              </section>
            </div>
            <div>
              <Database size={20} />
              <section>
                <h3>Handle databases with care</h3>
                <p>
                  Filter by .accdb or .mdb to review Access files. Recover
                  missing files to the source first, compare again, then
                  quarantine verified destination duplicates. Close databases
                  before cleanup.
                </p>
              </section>
            </div>
          </div>
          <p className="dialog-note">
            History and saved paths stay in this computer’s app data. Copies are
            verified locally; wait for OneDrive upload before original cleanup.
            Linked folders are skipped and reported.
          </p>
          <div className="dialog-actions">
            <button className="button primary" onClick={() => setHelp(false)}>
              Got it
              <Check size={15} />
            </button>
          </div>
        </Dialog>
      )}
    </div>
  );
}

function Distribution({
  title,
  groups,
  onSelect,
}: {
  title: string;
  groups: GroupTotal[];
  onSelect: (value: string) => void;
}) {
  const maximum = groups[0]?.bytes || 1;
  return (
    <section className="distribution">
      <div className="section-heading">
        <h2>{title}</h2>
        <span>{groups.length} groups</span>
      </div>
      {groups.length ? (
        groups.slice(0, 12).map((group) => (
          <button
            className="distribution-row"
            key={group.label}
            onClick={() => onSelect(group.label)}
          >
            <div>
              <strong>{group.label}</strong>
              <span>
                {count(group.count)} files <b>{bytes(group.bytes)}</b>
              </span>
            </div>
            <span className="distribution-track">
              <span
                style={{
                  width: `${Math.max(0.5, (group.bytes / maximum) * 100)}%`,
                }}
              />
            </span>
          </button>
        ))
      ) : (
        <p className="section-empty">No files in this comparison.</p>
      )}
    </section>
  );
}
