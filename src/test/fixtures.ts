import type { ImageEntry, ShootSummary, Group } from "../types";

let nextImageId = 1;
let nextGroupId = 1;

export function resetIds() {
  nextImageId = 1;
  nextGroupId = 1;
}

export function makeImage(overrides: Partial<ImageEntry> = {}): ImageEntry {
  const id = overrides.id ?? nextImageId++;
  return {
    id,
    filepath: `/photos/IMG_${String(id).padStart(4, "0")}.NEF`,
    filename: `IMG_${String(id).padStart(4, "0")}.NEF`,
    captureTime: "2026-03-15T10:30:00",
    cameraModel: "NIKON D750",
    lens: "AF-S NIKKOR 70-200mm f/2.8E FL ED VR",
    focalLength: 135,
    aperture: 2.8,
    shutterSpeed: "1/500",
    iso: 400,
    flag: "unreviewed",
    destination: "unrouted",
    starRating: 0,
    ...overrides,
  };
}

export function makeGroup(
  members: { photoId: number; isCover?: boolean }[],
  overrides: Partial<Group> = {},
): Group {
  const id = overrides.id ?? nextGroupId++;
  return {
    id,
    shootId: overrides.shootId ?? 1,
    groupType: overrides.groupType ?? "near_duplicate",
    members: members.map((m, i) => ({
      photoId: m.photoId,
      isCover: m.isCover ?? i === 0,
    })),
  };
}

export function makeShoot(overrides: Partial<ShootSummary> = {}): ShootSummary {
  return {
    id: 1,
    slug: "test-shoot",
    date: "2026-03-15",
    sourcePath: "/source/photos",
    destPath: "/dest/photos",
    photoCount: 10,
    importedAt: "2026-03-15T10:00:00",
    ...overrides,
  };
}

export function makeGroupWithImages(
  count: number,
  overrides?: { flag?: string; groupType?: Group["groupType"] },
): { images: ImageEntry[]; group: Group } {
  const images = Array.from({ length: count }, () =>
    makeImage({ flag: overrides?.flag ?? "unreviewed" }),
  );
  const group = makeGroup(
    images.map((img, i) => ({ photoId: img.id, isCover: i === 0 })),
    { groupType: overrides?.groupType },
  );
  return { images, group };
}
