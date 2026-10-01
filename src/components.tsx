import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion } from "motion/react";
import {
  X,
  Check,
  CircleAlert,
  File,
  FileSpreadsheet,
  FileText,
  FileArchive,
  Database,
  Image,
  FileCode,
  Minus,
  Square,
  Copy,
} from "lucide-react";
import { statuses } from "./format";
import type { Status } from "./types";

export function WindowControls({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const update = () => {
      void appWindow
        .isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value);
        })
        .catch((error) => {
          if (!disposed) onError(`Window status unavailable: ${String(error)}`);
        });
    };
    update();
    void appWindow
      .onResized(update)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((error) => {
        if (!disposed) onError(`Window status unavailable: ${String(error)}`);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [onError]);

  async function run(action: "minimize" | "toggleMaximize" | "close") {
    try {
      await getCurrentWindow()[action]();
    } catch (error) {
      onError(`Window control failed: ${String(error)}`);
    }
  }

  return (
    <div
      className="window-controls"
      role="group"
      aria-label="Window controls"
      data-tauri-drag-region="false"
    >
      <button
        className="window-button"
        aria-label="Minimize window"
        title="Minimize"
        onClick={() => void run("minimize")}
      >
        <Minus size={14} />
      </button>
      <button
        className="window-button"
        aria-label={maximized ? "Restore window" : "Maximize window"}
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => void run("toggleMaximize")}
      >
        {maximized ? <Copy size={12} /> : <Square size={12} />}
      </button>
      <button
        className="window-button window-close"
        aria-label="Close window"
        title="Close"
        onClick={() => void run("close")}
      >
        <X size={16} />
      </button>
    </div>
  );
}

export function StatusBadge({ status }: { status: Status | string }) {
  return (
    <span className={`status-badge status-${status}`}>
      {status === "identical" ? (
        <Check size={12} />
      ) : status === "error" ? (
        <CircleAlert size={12} />
      ) : (
        <span className="status-dot" />
      )}
      {statuses[status as Status] || status.replaceAll("_", " ")}
    </span>
  );
}

export function FileIcon({ extension }: { extension: string }) {
  const type = extension.toLowerCase();
  const Icon = [".xlsx", ".xls", ".csv"].includes(type)
    ? FileSpreadsheet
    : [".accdb", ".mdb", ".db"].includes(type)
      ? Database
      : [".docx", ".pdf", ".txt", ".md"].includes(type)
        ? FileText
        : [".zip", ".bak", ".rar"].includes(type)
          ? FileArchive
          : [".png", ".jpg", ".svg"].includes(type)
            ? Image
            : [".py", ".ps1", ".js", ".json", ".sql"].includes(type)
              ? FileCode
              : File;
  return (
    <span
      className={`file-icon file-${Icon === FileSpreadsheet ? "sheet" : Icon === Database ? "database" : Icon === FileText ? "document" : "default"}`}
    >
      <Icon size={17} strokeWidth={1.65} />
    </span>
  );
}

export function Dialog({
  title,
  onClose,
  children,
  className = "",
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return (
    <dialog
      ref={ref}
      className={`dialog ${className}`}
      aria-labelledby={id}
      onCancel={onClose}
      onClick={(e) => {
        if (e.target === ref.current) onClose();
      }}
    >
      <div className="dialog-heading">
        <h2 id={id}>{title}</h2>
        <button
          className="icon-button"
          onClick={onClose}
          aria-label="Close dialog"
        >
          <X size={19} />
        </button>
      </div>
      {children}
    </dialog>
  );
}

export function EmptyState({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <div>{children}</div>
    </div>
  );
}

export function ProgressBar({ value }: { value: number | null }) {
  return (
    <div
      className={`progress-track ${value === null ? "indeterminate" : ""}`}
      role="progressbar"
      aria-label="Job progress"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value ?? undefined}
    >
      <motion.div
        initial={false}
        animate={{
          width:
            value === null ? "28%" : `${Math.min(100, Math.max(0, value))}%`,
        }}
        transition={{ duration: 0.18, ease: "easeOut" }}
      />
    </div>
  );
}
