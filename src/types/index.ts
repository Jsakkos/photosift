export interface ImageEntry {
  id: number;
  filepath: string;
  filename: string;
  captureTime: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalLength: number | null;
  aperture: number | null;
  shutterSpeed: string | null;
  iso: number | null;
  flag: string;
  destination: string;
  starRating: number;
  // AI fields (optional — populated by background worker)
  faceCount?: number | null;
  eyesOpenCount?: number | null;
  sharpnessScore?: number | null;
  aiAnalyzedAt?: string | null;
}

export interface ShootSummary {
  id: number;
  slug: string;
  date: string;
  sourcePath: string;
  destPath: string;
  photoCount: number;
  importedAt: string;
}

export type CullView = "triage" | "select" | "route";
export type ViewMode = "sequential" | "grid" | "comparison";

export interface DisplayItem {
  imageIndex: number;
  image: ImageEntry;
  groupId?: number;
  isGroupCover?: boolean;
  groupMemberCount?: number;
}

export interface GroupMemberInfo {
  photoId: number;
  isCover: boolean;
}

export interface Group {
  id: number;
  shootId: number;
  groupType: "near_duplicate" | "related";
  members: GroupMemberInfo[];
}

export interface Face {
  photoId: number;
  bboxX: number; bboxY: number; bboxW: number; bboxH: number;
  leftEyeX: number; leftEyeY: number;
  rightEyeX: number; rightEyeY: number;
  leftEyeOpen: 0 | 1;
  rightEyeOpen: 0 | 1;
  leftEyeSharpness: number;
  rightEyeSharpness: number;
  detectionConfidence: number;
}

export type AiProviderStatus = "cuda" | "cpu" | "disabled";

export interface AiProgressEvent {
  photoId: number;
  ok: boolean;
  done: number;
  total: number;
  failed: number;
}
