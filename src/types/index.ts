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
  width: number | null;
  height: number | null;
  orientation: number | null;
  starRating: number;
}

export interface ProjectInfo {
  folderPath: string;
  imageCount: number;
  lastViewedIndex: number;
}
