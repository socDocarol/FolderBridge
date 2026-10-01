import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

export const desktop = isTauri();
export async function call<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (desktop) return invoke<T>(command, args);
  if (import.meta.env.DEV)
    return (await import("./demo")).demoCall<T>(command, args);
  throw new Error("Open the FolderBridge desktop app to access folders.");
}
export async function chooseFolder(): Promise<string | null> {
  if (!desktop) return null;
  const path = await open({
    directory: true,
    multiple: false,
    title: "Choose a folder to compare",
  });
  return typeof path === "string" ? path : null;
}
export async function chooseReport(
  defaultPath: string,
): Promise<string | null> {
  if (!desktop) return null;
  return save({
    title: "Export report to a new CSV file",
    defaultPath,
    filters: [{ name: "CSV report", extensions: ["csv"] }],
  });
}
