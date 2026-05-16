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
      case "create_group_from_photos":
      case "ungroup_photos":
      case "mark_photo_visited_in_select":
      case "bump_select_max_floor":
        return undefined;
      case "sync_layout_if_eligible":
        return null;
      case "get_settings":
        return {
          nearDupThreshold: 4,
          relatedThreshold: 12,
          groupTimeWindowS: 60,
        };
      case "recluster_shoot":
      case "recluster_shoot_with":
        return 0;
      case "get_triage_judgments_for_shoot":
      case "apply_triage_rejects":
      case "get_bracket_decisions_for_shoot":
        return [];
      case "record_bracket_decision":
      case "delete_bracket_decision":
        return undefined;
      case "export_xmp":
        return 0;
      default:
        throw new Error(`Unmocked IPC command: ${cmd}`);
    }
  });
}
