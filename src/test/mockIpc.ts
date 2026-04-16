import { mockIPC } from "@tauri-apps/api/mocks";
import type { InvokeArgs } from "@tauri-apps/api/core";
import type { ImageEntry, ShootSummary, Group } from "../types";

export interface MockIpcHandlers {
  get_shoot?: ShootSummary;
  get_image_list?: ImageEntry[];
  get_groups_for_shoot?: Group[];
  get_view_cursor?: number | null;
  [key: string]: unknown;
}

export function setupMockIpc(
  handlers: MockIpcHandlers = {},
  spyFn?: (cmd: string, args?: InvokeArgs) => void,
) {
  mockIPC((cmd: string, args?: InvokeArgs) => {
    if (spyFn) spyFn(cmd, args);

    if (cmd in handlers) {
      return handlers[cmd];
    }

    switch (cmd) {
      case "get_shoot":
        return handlers.get_shoot;
      case "get_image_list":
        return handlers.get_image_list ?? [];
      case "get_groups_for_shoot":
        return handlers.get_groups_for_shoot ?? [];
      case "get_view_cursor":
        return handlers.get_view_cursor ?? null;
      case "set_view_cursor":
      case "set_flag":
      case "bulk_set_flag":
      case "set_destination":
      case "set_rating":
      case "undo_last":
      case "set_group_cover":
      case "update_settings":
        return undefined;
      case "get_settings":
        return {
          nearDupThreshold: 4,
          relatedThreshold: 12,
          triageExpandGroups: false,
        };
      case "recluster_shoot":
        return 0;
      default:
        throw new Error(`Unmocked IPC command: ${cmd}`);
    }
  });
}
