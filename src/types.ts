export type ScanOptions = {
  source: string;
  destination: string | null;
  verifyContents: boolean;
  excludedExtensions: string[];
  collectOwners: boolean;
};
export type Scan = {
  id: number;
  source: string;
  destination: string | null;
  startedAt: number;
  state: string;
  verified: boolean;
  files: number;
  errors: number;
};
export type Status =
  | "source_only"
  | "destination_only"
  | "identical"
  | "different"
  | "unverified"
  | "excluded"
  | "error"
  | "inventory"
  | "pending";
export type Entry = {
  id: number;
  scanId: number;
  relativePath: string;
  sourceRelative: string | null;
  destinationRelative: string | null;
  extension: string;
  status: Status;
  sourceSize: number | null;
  destinationSize: number | null;
  sourceModified: number | null;
  destinationModified: number | null;
  sourceHash: string | null;
  destinationHash: string | null;
  owner: string | null;
  issue: string | null;
};
export type EntryFilter = {
  scanId: number;
  search: string;
  status: string;
  extension: string;
  minSize: number;
  offset: number;
  limit: number;
};
export type EntryPage = { entries: Entry[]; total: number };
export type GroupTotal = { label: string; count: number; bytes: number };
export type Analysis = {
  statuses: GroupTotal[];
  extensions: GroupTotal[];
  folders: GroupTotal[];
  sourceBytes: number;
  destinationBytes: number;
  duplicateBytes: number;
  sizeStatistics: SizeStatistics[];
};
export type Progress = {
  running: boolean;
  kind: string;
  phase: string;
  currentPath: string;
  processed: number;
  total: number;
  bytes: number;
  errors: number;
  scanId: number | null;
  operationId: number | null;
  message: string;
};
export type Action =
  | "copy_to_destination"
  | "copy_to_source"
  | "keep_both"
  | "quarantine_destination";
export type OperationRequest = {
  scanId: number;
  entryIds: number[];
  action: Action;
};
export type Preview = {
  action: Action;
  eligible: number;
  skipped: number;
  bytes: number;
  description: string;
};
export type Operation = {
  id: number;
  scanId: number;
  action: Action;
  startedAt: number;
  state: string;
  completed: number;
  skipped: number;
  errors: number;
};
export type OperationItem = {
  id: number;
  operationId: number;
  relativePath: string;
  originalPath: string;
  targetPath: string;
  state: string;
  message: string;
  contentHash: string | null;
  cleanup: CleanupRecord | null;
};
export type CleanupRecord = {
  method: string;
  state: string;
  holdingPath: string;
  message: string;
};
export type CleanupPreview = {
  originalRoot: string;
  targetRoot: string;
  isNetwork: boolean;
  items: { item: OperationItem; eligible: boolean; reason: string }[];
};
export type CleanupRequest = {
  operationId: number;
  itemIds: number[];
  handling: "local" | "sharepoint" | "network";
  confirmation: string;
  sharepointConfirmed: boolean;
};
export type CleanupSummary = {
  completed: number;
  skipped: number;
  message: string;
};
export type SavedPair = { id: number; name: string; options: ScanOptions };

export type SizeStatistics = {
  extension: string;
  count: number;
  mean: number;
  median: number;
  lowerQuartile: number;
  upperQuartile: number;
};
