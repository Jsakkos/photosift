import { invoke } from "@tauri-apps/api/core";
import type { DriveInfo } from "../types";

export async function listRemovableDrives(): Promise<DriveInfo[]> {
  return invoke<DriveInfo[]>("list_removable_drives");
}

export async function scanFolderForDates(
  source: string,
  dedupKnown: boolean,
): Promise<number> {
  return invoke<number>("scan_folder", {
    source,
    withThumbnails: false,
    dedupKnown,
  });
}

export async function extractThumbnailsForPaths(
  paths: string[],
): Promise<number> {
  return invoke<number>("extract_thumbnails_for_paths", { paths });
}
