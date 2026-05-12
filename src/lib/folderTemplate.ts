import type { FolderTemplate } from "../stores/settingsStore";

export type BucketKey = keyof FolderTemplate["buckets"];

export const BUCKET_ORDER: BucketKey[] = [
  "raw",
  "rejects",
  "selects",
  "edit",
  "export",
];

export const BUCKET_LABELS: Record<BucketKey, string> = {
  raw: "Import (RAW)",
  rejects: "Rejects",
  selects: "Selects",
  edit: "Edit",
  export: "Export",
};

export const PATH_TOKENS = ["{root}", "{year}", "{year-month}", "{slug}"] as const;

/// Characters illegal in a path segment on at least one of
/// Windows / macOS / Linux. `/` is the template separator; not allowed
/// inside a bucket name.
const ILLEGAL_CHARS = ['<', '>', ':', '"', '\\', '|', '?', '*'];

function stripTokens(s: string): string {
  return PATH_TOKENS.reduce((acc, t) => acc.split(t).join(""), s);
}

export interface FolderTemplateValidation {
  pathError: string | null;
  bucketErrors: Record<BucketKey, string | null>;
  /// Flat list of every problem, for the Save-gate / summary line.
  all: string[];
}

export function validateFolderTemplate(t: FolderTemplate): FolderTemplateValidation {
  let pathError: string | null = null;
  const bucketErrors: Record<BucketKey, string | null> = {
    raw: null,
    rejects: null,
    selects: null,
    edit: null,
    export: null,
  };

  const path = t.pathTemplate ?? "";
  if (path.trim().length === 0) {
    pathError = "Path template can't be empty.";
  } else if (!path.includes("{slug}")) {
    pathError = "Must include {slug} so shoots don't collide on disk.";
  } else {
    const stripped = stripTokens(path);
    if (stripped.split("").some((c) => ILLEGAL_CHARS.includes(c))) {
      pathError = `Illegal character. Avoid: ${ILLEGAL_CHARS.join(" ")}`;
    } else if (stripped.includes("{") || stripped.includes("}")) {
      pathError =
        "Unrecognized {token}. Supported: {root} {year} {year-month} {slug}.";
    }
  }

  const seen: string[] = [];
  for (const key of BUCKET_ORDER) {
    const name = (t.buckets?.[key] ?? "").trim();
    if (name.length === 0) {
      bucketErrors[key] = "Can't be empty.";
      continue;
    }
    if (
      name.includes("/") ||
      name.includes("\\") ||
      name.split("").some((c) => ILLEGAL_CHARS.includes(c))
    ) {
      bucketErrors[key] = `No / \\ ${ILLEGAL_CHARS.join(" ")}`;
      continue;
    }
    if (seen.some((s) => s.toLowerCase() === name.toLowerCase())) {
      bucketErrors[key] = "Duplicates another bucket name.";
      continue;
    }
    seen.push(name);
  }

  const all: string[] = [];
  if (pathError) all.push(`Path template: ${pathError}`);
  for (const key of BUCKET_ORDER) {
    const e = bucketErrors[key];
    if (e) all.push(`${BUCKET_LABELS[key]} bucket: ${e}`);
  }
  return { pathError, bucketErrors, all };
}

/// Sample values for the live preview in Settings.
const SAMPLE = {
  root: "~/Pictures",
  year: "2026",
  "year-month": "2026-05",
  slug: "greece-trip",
} as const;

export function previewShootDir(pathTemplate: string): string {
  let out = pathTemplate;
  out = out.split("{root}").join(SAMPLE.root);
  out = out.split("{year-month}").join(SAMPLE["year-month"]);
  out = out.split("{year}").join(SAMPLE.year);
  out = out.split("{slug}").join(SAMPLE.slug);
  return out;
}

/// The on-disk tree a fresh-imported, fully-culled shoot would have,
/// given the template. `rejects`/`selects`/`edit` nest under `raw`;
/// `export` is a top-level sibling (issue #7).
export function previewBucketTree(t: FolderTemplate): { label: string; path: string }[] {
  const shoot = previewShootDir(t.pathTemplate);
  const b = t.buckets;
  return [
    { label: "import", path: `${shoot}/${b.raw}/` },
    { label: "rejects", path: `${shoot}/${b.raw}/${b.rejects}/` },
    { label: "selects", path: `${shoot}/${b.raw}/${b.selects}/` },
    { label: "edit", path: `${shoot}/${b.raw}/${b.edit}/` },
    { label: "export", path: `${shoot}/${b.export}/` },
  ];
}
