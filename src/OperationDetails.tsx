import { useEffect, useState } from "react";
import {
  ArrowDownToLine,
  ChevronLeft,
  ChevronRight,
  Undo2,
  LoaderCircle,
} from "lucide-react";
import { call, desktop } from "./api";
import { Dialog, StatusBadge } from "./components";
import { actions, date, readablePath } from "./format";
import type {
  CleanupPreview,
  CleanupRequest,
  CleanupSummary,
  Operation,
  OperationItem,
} from "./types";

type Props = {
  operation: Operation;
  items: OperationItem[];
  running: boolean;
  onClose: () => void;
  onExport: () => void;
  onQuarantineRestore: () => void;
  onCleanup: (request: CleanupRequest) => Promise<CleanupSummary>;
  onRestore: (itemIds: number[]) => Promise<CleanupSummary>;
  onRefresh: () => Promise<OperationItem[]>;
};

export function OperationDetails(props: Props) {
  const [items, setItems] = useState(props.items);
  const [stage, setStage] = useState<
    "details" | "review" | "confirm" | "restore"
  >("details");
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [page, setPage] = useState(0);
  const [handling, setHandling] = useState<CleanupRequest["handling"]>("local");
  const [confirmation, setConfirmation] = useState("");
  const [sharepointConfirmed, setSharepointConfirmed] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const copyAction = [
    "copy_to_destination",
    "copy_to_source",
    "keep_both",
  ].includes(props.operation.action);
  const restoreable = items.filter(
    (item) =>
      item.cleanup &&
      ["held", "recycled", "needs_review", "planned", "restoring"].includes(
        item.cleanup.state,
      ),
  );
  const rows =
    stage === "review"
      ? preview?.items.map((row) => row.item) || []
      : stage === "restore"
        ? restoreable
        : items;
  const [pageSize, setPageSize] = useState(window.innerHeight < 650 ? 1 : 2);
  useEffect(() => {
    const resize = () => setPageSize(window.innerHeight < 650 ? 1 : 2);
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const visible = rows.slice(page * pageSize, (page + 1) * pageSize);
  useEffect(() => setPage(0), [stage]);
  useEffect(
    () => setPage((value) => Math.min(value, pageCount - 1)),
    [pageCount],
  );

  async function run(work: () => Promise<void>) {
    setWorking(true);
    setError("");
    setMessage("");
    try {
      await work();
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  }
  async function reviewCleanup() {
    await run(async () => {
      const next = await call<CleanupPreview>("preview_cleanup", {
        operationId: props.operation.id,
      });
      setPreview(next);
      setSelected(
        new Set(
          next.items.filter((row) => row.eligible).map((row) => row.item.id),
        ),
      );
      setHandling(next.isNetwork ? "network" : "local");
      setConfirmation("");
      setSharepointConfirmed(false);
      setStage("review");
    });
  }
  async function cleanup() {
    await run(async () => {
      try {
        const summary = await props.onCleanup({
          operationId: props.operation.id,
          itemIds: [...selected],
          handling,
          confirmation,
          sharepointConfirmed,
        });
        setMessage(summary.message);
      } finally {
        setItems(await props.onRefresh());
        setStage("details");
      }
    });
  }
  async function restore() {
    await run(async () => {
      try {
        const summary = await props.onRestore([...selected]);
        setMessage(summary.message);
      } finally {
        setItems(await props.onRefresh());
        setStage("details");
      }
    });
  }
  function reveal(
    item: OperationItem,
    location: "original" | "target" | "holding",
  ) {
    if (!desktop) {
      setMessage("Explorer is available in the desktop app.");
      return;
    }
    void run(async () => {
      await call("reveal_operation_item", {
        operationId: props.operation.id,
        itemId: item.id,
        location,
      });
      setMessage("Opened in Explorer.");
    });
  }
  function toggle(id: number) {
    setSelected((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  const eligible = new Set(
    preview?.items.filter((row) => row.eligible).map((row) => row.item.id),
  );
  const title =
    stage === "review"
      ? "Review original cleanup"
      : stage === "confirm"
        ? "Confirm original cleanup"
        : stage === "restore"
          ? "Restore originals"
          : actions[props.operation.action]?.label || "Operation details";
  return (
    <Dialog
      title={title}
      onClose={props.onClose}
      className="wide-dialog operation-dialog"
    >
      <p className="dialog-intro">
        {date(props.operation.startedAt)} · Operation {props.operation.id}
      </p>
      {stage === "review" && preview && (
        <div className="cleanup-roots">
          <div>
            <span>Original folder</span>
            <code>{readablePath(preview.originalRoot)}</code>
          </div>
          <div>
            <span>Copy folder</span>
            <code>{readablePath(preview.targetRoot)}</code>
          </div>
        </div>
      )}
      {stage === "confirm" && preview && (
        <>
          <div className="cleanup-roots">
            <div>
              <span>Originals to handle</span>
              <code>{readablePath(preview.originalRoot)}</code>
            </div>
            <div>
              <span>Verified copy location</span>
              <code>{readablePath(preview.targetRoot)}</code>
            </div>
          </div>
          <p className="dialog-note">
            The app rechecks both files before moving each original. Wait until
            cloud copies finish uploading before proceeding.
          </p>
          <fieldset className="cleanup-choices" disabled={preview.isNetwork}>
            <legend>Original location</legend>
            <label>
              <input
                type="radio"
                name="handling"
                checked={handling === "local"}
                onChange={() => setHandling("local")}
              />{" "}
              Local drive · Recycle Bin
            </label>
            <label>
              <input
                type="radio"
                name="handling"
                checked={handling === "sharepoint"}
                onChange={() => setHandling("sharepoint")}
              />{" "}
              Synced SharePoint · Recycle Bin, then OneDrive sync
            </label>
            <label>
              <input
                type="radio"
                name="handling"
                checked={handling === "network"}
                onChange={() => setHandling("network")}
              />{" "}
              Network share · _ToDelete holding folder
            </label>
          </fieldset>
          {preview.isNetwork && (
            <p className="dialog-note">
              Network folder detected. Originals move to _ToDelete, where you
              can restore them directly.
            </p>
          )}
          {handling === "sharepoint" && (
            <label className="check-label cleanup-sharepoint">
              <input
                type="checkbox"
                checked={sharepointConfirmed}
                onChange={(e) => setSharepointConfirmed(e.target.checked)}
              />{" "}
              I checked OneDrive sync. Deletion will sync to SharePoint.
            </label>
          )}
          <label className="cleanup-type">
            Type REMOVE {selected.size} to confirm
            <input
              value={confirmation}
              onChange={(e) => setConfirmation(e.target.value)}
              autoComplete="off"
            />
          </label>
        </>
      )}
      {stage === "restore" && (
        <p className="dialog-note">
          {restoreable.some((item) => item.cleanup?.method !== "network")
            ? "First restore Recycle Bin items in Windows. Then use Restore selected here to return them from the recovery path to the original path."
            : "Return selected files from _ToDelete to their original paths."}{" "}
          Existing files are never overwritten.
        </p>
      )}
      {stage !== "confirm" && (
        <>
          <div className="operation-items">
            {visible.map((item) => {
              const previewRow = preview?.items.find(
                (row) => row.item.id === item.id,
              );
              const canSelect =
                stage === "review"
                  ? eligible.has(item.id)
                  : stage === "restore";
              return (
                <div className="operation-item" key={item.id}>
                  <div>
                    {(stage === "review" || stage === "restore") && (
                      <input
                        type="checkbox"
                        aria-label={`Select ${item.relativePath}`}
                        checked={selected.has(item.id)}
                        disabled={!canSelect}
                        onChange={() => toggle(item.id)}
                      />
                    )}
                    <strong>{item.relativePath}</strong>
                    <StatusBadge status={item.state} />
                    {item.cleanup && (
                      <StatusBadge status={item.cleanup.state} />
                    )}
                  </div>
                  <p>
                    {stage === "review" && !previewRow?.eligible
                      ? previewRow?.reason
                      : item.cleanup && copyAction
                        ? "Copy completed and verified."
                        : item.message}
                  </p>
                  {item.cleanup?.message && <p>{item.cleanup.message}</p>}
                  <div className="cleanup-path">
                    <span>Original</span>
                    <code>{readablePath(item.originalPath)}</code>
                    {copyAction && (
                      <button
                        className="text-button"
                        aria-label={`Show original ${item.relativePath} in Explorer`}
                        onClick={() => reveal(item, "original")}
                      >
                        Explorer
                      </button>
                    )}
                  </div>
                  <div className="cleanup-path">
                    <span>{copyAction ? "Copy" : "Moved to"}</span>
                    <code>{readablePath(item.targetPath)}</code>
                    {copyAction && (
                      <button
                        className="text-button"
                        aria-label={`Show copy ${item.relativePath} in Explorer`}
                        onClick={() => reveal(item, "target")}
                      >
                        Explorer
                      </button>
                    )}
                  </div>
                  {item.cleanup?.holdingPath && (
                    <div className="cleanup-path">
                      <span>Recovery</span>
                      <code>{readablePath(item.cleanup.holdingPath)}</code>
                      <button
                        className="text-button"
                        aria-label={`Show recovery ${item.relativePath} in Explorer`}
                        onClick={() => reveal(item, "holding")}
                      >
                        Explorer
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {!rows.length && (
              <p className="dialog-note">
                No files are available for this step.
              </p>
            )}
          </div>
          {pageCount > 1 && (
            <div className="cleanup-pages">
              <button
                className="text-button"
                onClick={() => setPage(page - 1)}
                disabled={page === 0}
                aria-label="Previous item page"
              >
                <ChevronLeft size={15} /> Previous
              </button>
              <span>
                {page + 1} / {pageCount}
              </span>
              <button
                className="text-button"
                onClick={() => setPage(page + 1)}
                disabled={page + 1 === pageCount}
                aria-label="Next item page"
              >
                Next <ChevronRight size={15} />
              </button>
            </div>
          )}
          {stage === "review" && (
            <div className="cleanup-selection">
              <span>{selected.size} eligible copied originals selected.</span>
              <button
                className="text-button"
                onClick={() => setSelected(new Set(eligible))}
              >
                Select eligible
              </button>
              <button
                className="text-button"
                onClick={() => setSelected(new Set())}
              >
                Clear
              </button>
            </div>
          )}
          {stage === "review" && (
            <p className="dialog-note">
              Contents are checked again at cleanup. Older copies without file
              identity records stay unavailable.
            </p>
          )}
        </>
      )}
      {working && (
        <div className="cleanup-working" role="status">
          <LoaderCircle className="spin" size={14} />
          <span>
            {props.running
              ? "Checking files and applying confirmed changes..."
              : "Checking locations..."}
          </span>
          {props.running && (
            <button
              className="text-button"
              onClick={() =>
                void call("cancel_job")
                  .then(() => setMessage("Stopping after the current file..."))
                  .catch((e) => setError(String(e)))
              }
            >
              Cancel job
            </button>
          )}
        </div>
      )}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
      {message && (
        <p className="dialog-note" role="status">
          {message}
        </p>
      )}
      <div className="dialog-actions cleanup-actions">
        {stage === "details" && (
          <>
            <button className="button secondary" onClick={props.onExport}>
              <ArrowDownToLine size={15} /> Export log
            </button>
            {copyAction && (
              <button
                className="button secondary"
                disabled={working || props.running}
                onClick={() => void reviewCleanup()}
              >
                Review cleanup
              </button>
            )}
            {copyAction && restoreable.length > 0 && (
              <button
                className="button secondary"
                disabled={working || props.running}
                onClick={() => {
                  setSelected(new Set(restoreable.map((item) => item.id)));
                  setStage("restore");
                }}
              >
                <Undo2 size={15} /> Review restore
              </button>
            )}
            {props.operation.action === "quarantine_destination" && (
              <button
                className="button primary"
                disabled={
                  props.running ||
                  !items.some(
                    (i) => i.state === "quarantined" || i.state === "planned",
                  )
                }
                onClick={props.onQuarantineRestore}
              >
                <Undo2 size={15} /> Review restore
              </button>
            )}
          </>
        )}
        {stage !== "details" && (
          <button
            className="button secondary"
            disabled={working}
            onClick={() => setStage(stage === "confirm" ? "review" : "details")}
          >
            Back
          </button>
        )}
        {stage === "review" && (
          <button
            className="button primary"
            disabled={!selected.size || working}
            onClick={() => {
              setConfirmation("");
              setStage("confirm");
            }}
          >
            Continue
          </button>
        )}
        {stage === "confirm" && (
          <button
            className="button primary"
            disabled={
              working ||
              props.running ||
              confirmation !== `REMOVE ${selected.size}` ||
              (handling === "sharepoint" && !sharepointConfirmed)
            }
            onClick={() => void cleanup()}
          >
            {handling === "network"
              ? "Move to _ToDelete"
              : handling === "sharepoint"
                ? "Remove synced originals"
                : "Recycle originals"}
          </button>
        )}
        {stage === "restore" && (
          <button
            className="button primary"
            disabled={!selected.size || working || props.running}
            onClick={() => void restore()}
          >
            Restore selected
          </button>
        )}
      </div>
    </Dialog>
  );
}
