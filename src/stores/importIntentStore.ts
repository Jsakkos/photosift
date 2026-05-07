import { create } from "zustand";
import type { DriveInfo } from "../types";

interface ImportIntentState {
  requestedDrive: DriveInfo | null;
  requestImport: (drive: DriveInfo | null) => void;
  clearRequest: () => void;
}

export const useImportIntentStore = create<ImportIntentState>((set) => ({
  requestedDrive: null,
  requestImport: (drive) => set({ requestedDrive: drive }),
  clearRequest: () => set({ requestedDrive: null }),
}));
