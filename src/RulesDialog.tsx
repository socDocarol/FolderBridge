import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, FolderOpen, Plus } from "lucide-react";
import { desktop } from "./api";
import { Dialog } from "./components";
import {
  backupTypes,
  emptyRules,
  mib,
  temporaryTypes,
  withRules,
} from "./rules";
import type { FileRules, ScanOptions } from "./types";

type Section = "Types" | "Sizes" | "Files" | "Scan";
const sections: Section[] = ["Types", "Sizes", "Files", "Scan"];
const sizeFields = [
  ["minSize", "Minimum size (MiB)"],
  ["maxSize", "Maximum size (MiB)"],
  ["reviewAbove", "Review above (MiB)"],
  ["reviewAccessAbove", "Review Access above (MiB)"],
] as const;
type SizeKey = (typeof sizeFields)[number][0];
const sizeInputs = (rules: FileRules) =>
  Object.fromEntries(
    sizeFields.map(([key]) => [
      key,
      rules[key] == null ? "" : String(rules[key]! / mib),
    ]),
  ) as Record<SizeKey, string>;

export function RulesDialog({
  options,
  extensions,
  initialSection = "Types",
  onApply,
  onClose,
}: {
  options: ScanOptions;
  extensions: string[];
  initialSection?: Section;
  onApply: (options: ScanOptions) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(() => withRules(options));
  const [section, setSection] = useState<Section>(initialSection);
  const [sizes, setSizes] = useState(() =>
    sizeInputs(withRules(options).rules),
  );
  const [paths, setPaths] = useState(
    options.rules?.excludedPaths.join("\n") || "",
  );
  const [search, setSearch] = useState("");
  const [customTypes, setCustomTypes] = useState<string[]>([]);
  const [typePage, setTypePage] = useState(0);
  const [error, setError] = useState("");
  const [picking, setPicking] = useState(false);
  const types = [
    ...new Set([
      ...backupTypes,
      ...temporaryTypes,
      ".mdb",
      ".accdb",
      ...extensions,
      ...draft.excludedExtensions,
      ...draft.rules.reviewExtensions,
      ...customTypes,
    ]),
  ]
    .filter((type) => type.startsWith("."))
    .sort()
    .filter((type) => type.includes(search.trim().toLowerCase()));
  const offset = Math.min(
    typePage * 5,
    Math.max(0, Math.floor((types.length - 1) / 5) * 5),
  );

  function setType(type: string, effect: string) {
    setDraft((old) => ({
      ...old,
      excludedExtensions: [
        ...old.excludedExtensions.filter((v) => v !== type),
        ...(effect === "skip" ? [type] : []),
      ],
      rules: {
        ...old.rules,
        reviewExtensions: [
          ...old.rules.reviewExtensions.filter((v) => v !== type),
          ...(effect === "review" ? [type] : []),
        ],
      },
    }));
  }
  function setGroup(types: string[], checked: boolean) {
    setDraft((old) => ({
      ...old,
      excludedExtensions: [
        ...old.excludedExtensions.filter((v) => !types.includes(v)),
        ...(checked ? types : []),
      ],
      rules: {
        ...old.rules,
        reviewExtensions: checked
          ? old.rules.reviewExtensions.filter((v) => !types.includes(v))
          : old.rules.reviewExtensions,
      },
    }));
  }
  async function pick(directory: boolean) {
    setError("");
    if (!desktop) {
      setError(
        "Folder pickers are available in the desktop app. You can enter relative paths below.",
      );
      return;
    }
    setPicking(true);
    try {
      const chosen = await open({
        directory,
        multiple: !directory,
        defaultPath: draft.source || undefined,
        title: directory
          ? "Choose a source folder to skip"
          : "Choose source files to skip",
      });
      if (!chosen) return;
      const root = draft.source.replaceAll("\\", "/").replace(/\/+$/, "");
      const relative = (Array.isArray(chosen) ? chosen : [chosen]).map(
        (path) => {
          const normalized = path.replaceAll("\\", "/");
          if (
            !root ||
            !normalized.toLowerCase().startsWith(root.toLowerCase() + "/")
          )
            throw new Error("Choose an item inside the source folder.");
          return normalized.slice(root.length + 1) + (directory ? "/" : "");
        },
      );
      setPaths((old) =>
        [...new Set([...old.split("\n").filter(Boolean), ...relative])].join(
          "\n",
        ),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setPicking(false);
    }
  }
  function apply() {
    setError("");
    const rules = { ...draft.rules };
    for (const [key] of sizeFields) {
      const value = sizes[key].trim();
      const converted = Number(value) * mib;
      if (
        value &&
        (!Number.isFinite(converted) ||
          converted < 0 ||
          converted > Number.MAX_SAFE_INTEGER)
      ) {
        setError("Enter a non-negative file size.");
        return;
      }
      rules[key] = value ? Math.round(converted) : null;
    }
    if (
      rules.minSize != null &&
      rules.maxSize != null &&
      rules.minSize > rules.maxSize
    ) {
      setError("Minimum size must not exceed maximum size.");
      return;
    }
    rules.excludedPaths = [
      ...new Set(
        paths
          .split("\n")
          .map((v) => v.trim().replaceAll("\\", "/"))
          .filter(Boolean),
      ),
    ];
    if (
      rules.excludedPaths.some(
        (p) =>
          p.startsWith("/") ||
          p.includes(":") ||
          p
            .replace(/\/$/, "")
            .split("/")
            .some((part) => !part || part === "." || part === ".."),
      )
    ) {
      setError(
        "Use relative paths inside your folders, without .. or empty path segments.",
      );
      return;
    }
    onApply({ ...draft, rules });
  }
  return (
    <Dialog
      title="Rules"
      className="rules-dialog"
      onClose={picking ? () => {} : onClose}
    >
      <p className="rules-intro">
        Shared by Compare and Migrate. Skips stay visible; flags need approval.
      </p>
      <div role="tablist" aria-label="Rule settings" className="rules-tabs">
        {sections.map((name, i) => (
          <button
            key={name}
            id={`rules-${name}`}
            role="tab"
            aria-selected={section === name}
            aria-controls="rules-panel"
            tabIndex={section === name ? 0 : -1}
            className={section === name ? "active" : ""}
            onClick={() => setSection(name)}
            onKeyDown={(e) => {
              if (!["ArrowLeft", "ArrowRight"].includes(e.key)) return;
              e.preventDefault();
              const next = sections[(i + (e.key === "ArrowRight" ? 1 : 3)) % 4];
              setSection(next);
              document.getElementById(`rules-${next}`)?.focus();
            }}
          >
            {name}
          </button>
        ))}
      </div>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          apply();
        }}
      >
        <div
          id="rules-panel"
          role="tabpanel"
          aria-labelledby={`rules-${section}`}
          className="rules-content"
        >
          {section === "Types" && (
            <>
              <div className="rule-presets">
                <label>
                  <input
                    type="checkbox"
                    aria-label="Skip backup files"
                    checked={backupTypes.every((t) =>
                      draft.excludedExtensions.includes(t),
                    )}
                    onChange={(e) => setGroup(backupTypes, e.target.checked)}
                  />{" "}
                  Skip backup files <small>{backupTypes.join(", ")}</small>
                </label>
                <label>
                  <input
                    type="checkbox"
                    aria-label="Skip temporary files and logs"
                    checked={temporaryTypes.every((t) =>
                      draft.excludedExtensions.includes(t),
                    )}
                    onChange={(e) => setGroup(temporaryTypes, e.target.checked)}
                  />{" "}
                  Skip temporary files and logs
                </label>
              </div>
              <div className="rule-type-search">
                <input
                  className="text-input"
                  aria-label="Find or add file type"
                  placeholder="Find or add a type, e.g. .pdf"
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value);
                    setTypePage(0);
                  }}
                />
                <button
                  type="button"
                  className="button secondary compact"
                  aria-label="Add file type"
                  onClick={() => {
                    const type =
                      "." + search.trim().replace(/^\.+/, "").toLowerCase();
                    if (!/^\.[\p{L}\p{N}_-]+$/u.test(type)) {
                      setError("Enter one extension, such as .pdf.");
                      return;
                    }
                    setCustomTypes((old) => [...old, type]);
                    setSearch(type);
                    setError("");
                  }}
                >
                  <Plus size={14} />
                  Add
                </button>
              </div>
              <div className="rule-type-list">
                {types.slice(offset, offset + 5).map((type) => (
                  <label key={type}>
                    <span>{type}</span>
                    <select
                      aria-label={`Rule for ${type}`}
                      value={
                        draft.excludedExtensions.includes(type)
                          ? "skip"
                          : draft.rules.reviewExtensions.includes(type)
                            ? "review"
                            : "include"
                      }
                      onChange={(e) => setType(type, e.target.value)}
                    >
                      <option value="include">Include</option>
                      <option value="skip">Skip</option>
                      <option value="review">Review</option>
                    </select>
                  </label>
                ))}
                {!types.length && <p>No matching type. Add it above.</p>}
              </div>
              <div className="rule-pagination">
                <span>
                  {types.length
                    ? `${offset + 1}–${Math.min(offset + 5, types.length)} of ${types.length} types`
                    : "0 types"}
                </span>
                <button
                  type="button"
                  className="icon-button"
                  aria-label="Previous file types"
                  disabled={offset === 0}
                  onClick={() => setTypePage(Math.max(0, typePage - 1))}
                >
                  <ChevronLeft size={15} />
                </button>
                <button
                  type="button"
                  className="icon-button"
                  aria-label="Next file types"
                  disabled={offset + 5 >= types.length}
                  onClick={() => setTypePage(typePage + 1)}
                >
                  <ChevronRight size={15} />
                </button>
              </div>
            </>
          )}
          {section === "Sizes" && (
            <>
              <p className="rules-hint">
                Blank means no limit. Outside the size range is skipped. MiB =
                1,048,576 bytes.
              </p>
              <div className="rule-sizes">
                {sizeFields.map(([key, label]) => (
                  <label className="field-label" key={key}>
                    {label}
                    <input
                      className="text-input"
                      type="number"
                      min="0"
                      step="any"
                      value={sizes[key]}
                      placeholder="No limit"
                      onChange={(e) =>
                        setSizes((old) => ({ ...old, [key]: e.target.value }))
                      }
                    />
                  </label>
                ))}
              </div>
              <p className="rules-hint">
                Review thresholds flag files strictly above the limit. Access
                means .mdb and .accdb. Rules use source size, or destination
                size when the source is absent.
              </p>
              <p className="dialog-note">
                Different versions always need approval and use Keep both.
              </p>
            </>
          )}
          {section === "Files" && (
            <>
              <div className="rule-file-pickers">
                <button
                  type="button"
                  className="button secondary compact"
                  disabled={picking || !draft.source}
                  onClick={() => void pick(false)}
                >
                  <Plus size={14} />
                  Add files
                </button>
                <button
                  type="button"
                  className="button secondary compact"
                  disabled={picking || !draft.source}
                  onClick={() => void pick(true)}
                >
                  <FolderOpen size={14} />
                  Add folder
                </button>
              </div>
              <label className="field-label">
                Skipped paths
                <textarea
                  className="text-input"
                  rows={6}
                  value={paths}
                  onChange={(e) => setPaths(e.target.value)}
                  placeholder={"Backups/\nReports/old-report.pdf"}
                />
              </label>
              <p className="rules-hint">
                One relative path per line. End folders with / to skip their
                contents. Rules apply to both selected roots. Remove a line to
                include it again.
              </p>
            </>
          )}
          {section === "Scan" && (
            <div className="scan-options">
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={draft.verifyContents}
                  onChange={(e) =>
                    setDraft((old) => ({
                      ...old,
                      verifyContents: e.target.checked,
                    }))
                  }
                />
                <span>
                  <strong>Verify contents</strong>
                  <small>
                    SHA-256 comparison. May download online-only files.
                  </small>
                </span>
              </label>
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={draft.collectOwners}
                  onChange={(e) =>
                    setDraft((old) => ({
                      ...old,
                      collectOwners: e.target.checked,
                    }))
                  }
                />
                <span>
                  <strong>Include owners</strong>
                  <small>Adds owner IDs. Slower on network shares.</small>
                </span>
              </label>
              <p className="dialog-note">
                Copies are always verified. Quick comparisons cannot authorize
                duplicate quarantine or resolve equal-size conflicts.
              </p>
            </div>
          )}
        </div>
        {error && (
          <p className="error-text rules-error" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button
            type="button"
            className="text-button rule-reset"
            onClick={() => {
              setDraft((old) => ({
                ...old,
                excludedExtensions: [],
                rules: { ...emptyRules },
              }));
              setSizes(sizeInputs(emptyRules));
              setPaths("");
              setError("");
            }}
          >
            Reset rules
          </button>
          <button
            type="button"
            className="button secondary"
            disabled={picking}
            onClick={onClose}
          >
            Cancel
          </button>
          <button className="button primary" disabled={picking}>
            Apply rules
          </button>
        </div>
      </form>
    </Dialog>
  );
}
