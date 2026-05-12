import { describe, it, expect } from "vitest";
import {
  validateFolderTemplate,
  previewBucketTree,
  previewShootDir,
} from "../folderTemplate";
import { DEFAULT_FOLDER_TEMPLATE } from "../../stores/settingsStore";

describe("validateFolderTemplate", () => {
  it("accepts the default template", () => {
    expect(validateFolderTemplate(DEFAULT_FOLDER_TEMPLATE).all).toEqual([]);
  });

  it("flags a path template missing {slug}", () => {
    const v = validateFolderTemplate({
      ...DEFAULT_FOLDER_TEMPLATE,
      pathTemplate: "{root}/{year}",
    });
    expect(v.pathError).toMatch(/\{slug\}/);
    expect(v.all.length).toBeGreaterThan(0);
  });

  it("flags an unrecognized token", () => {
    const v = validateFolderTemplate({
      ...DEFAULT_FOLDER_TEMPLATE,
      pathTemplate: "{root}/{slug}/{camera}",
    });
    expect(v.pathError).toMatch(/[Uu]nrecognized/);
  });

  it("flags empty, duplicate, and illegal bucket names", () => {
    const v = validateFolderTemplate({
      pathTemplate: "{root}/{slug}",
      buckets: {
        raw: "RAW",
        rejects: "",
        selects: "RAW",
        edit: "bad/name",
        export: "Export",
      },
    });
    expect(v.bucketErrors.rejects).toMatch(/empty/i);
    expect(v.bucketErrors.selects).toMatch(/[Dd]uplicate/);
    expect(v.bucketErrors.edit).toMatch(/\//);
    expect(v.all.length).toBe(3);
  });
});

describe("preview helpers", () => {
  it("substitutes sample values into the path template", () => {
    expect(previewShootDir("{root}/DSLR/{year}/{year-month}_{slug}")).toBe(
      "~/Pictures/DSLR/2026/2026-05_greece-trip",
    );
  });

  it("nests rejects/selects/edit under raw and keeps export top-level", () => {
    const rows = previewBucketTree(DEFAULT_FOLDER_TEMPLATE);
    const byLabel = Object.fromEntries(rows.map((r) => [r.label, r.path]));
    expect(byLabel.import).toMatch(/_greece-trip\/RAW\/$/);
    expect(byLabel.rejects).toMatch(/_greece-trip\/RAW\/rejects\/$/);
    expect(byLabel.export).toMatch(/_greece-trip\/Export\/$/);
    expect(byLabel.export).not.toMatch(/RAW\/Export/);
  });
});
