import { describe, it, expect } from "vitest";
import {
  computeSummary,
  slugify,
  type BenchmarkSummary,
} from "../benchmarkStore";
import { emptyPhotoRecord } from "../../types/benchmark";
import type {
  BenchmarkPhotoRecord,
  BenchmarkSet,
  BenchmarkFaceJudgment,
  SharpnessSnapshot,
} from "../../types/benchmark";

function face(
  index: number,
  fields: Partial<Omit<BenchmarkFaceJudgment, "faceIndex">>,
): BenchmarkFaceJudgment {
  return {
    faceIndex: index,
    bboxSnapshot: null,
    leftEyeSnapshot: null,
    rightEyeSnapshot: null,
    detectionCorrect: null,
    landmarkCorrect: null,
    leftEyeCorrect: null,
    rightEyeCorrect: null,
    smileCorrect: null,
    speciesCorrect: null,
    ...fields,
  };
}

function photo(
  id: number,
  fields: Partial<BenchmarkPhotoRecord>,
): BenchmarkPhotoRecord {
  return {
    ...emptyPhotoRecord(id, 1, "NIKON D750"),
    ...fields,
  };
}

function setOf(photos: BenchmarkPhotoRecord[]): BenchmarkSet {
  return {
    set: {
      name: "Test",
      slug: "test",
      createdAt: "2026-05-13T00:00:00Z",
      notes: "",
      schemaVersion: 1,
    },
    photos,
  };
}

describe("benchmarkStore — summary math", () => {
  it("empty set produces zeros without NaNs", () => {
    const summary = computeSummary(setOf([]));
    expect(summary.totalPhotos).toBe(0);
    expect(summary.judgedPhotos).toBe(0);
    expect(summary.faceOverall.precision).toBeNull();
    expect(summary.faceOverall.recall).toBeNull();
    expect(summary.faceOverall.f1).toBeNull();
    expect(summary.landmark).toEqual({ correct: 0, total: 0 });
    expect(summary.leftEye).toEqual({ correct: 0, total: 0 });
    expect(summary.leftEyeGivenLandmarkOk).toEqual({ correct: 0, total: 0 });
    expect(summary.facePerCamera).toEqual([]);
    for (const g of summary.sharpnessGroups) {
      expect(g.count).toBe(0);
      expect(g.globalScore.mean).toBeNull();
    }
  });

  it("face precision / recall / F1 against a small worked example", () => {
    // 3 photos, all D750:
    //   photo 1: 2 detected, both correct, 1 missed   → tp=2, fp=0, missed=1
    //   photo 2: 2 detected, 1 correct, 1 wrong, 0 missed → tp=1, fp=1
    //   photo 3: 1 detected, wrong, 2 missed          → tp=0, fp=1, missed=2
    // totals: tp=3, fp=2, missed=3
    // precision = 3/(3+2) = 0.6
    // recall    = 3/(3+3) = 0.5
    // F1        = 2*0.6*0.5 / 1.1 = 0.5454...
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        faces: [
          face(0, { detectionCorrect: true }),
          face(1, { detectionCorrect: true }),
        ],
        missedFaceCount: 1,
      }),
      photo(2, {
        faces: [
          face(0, { detectionCorrect: true }),
          face(1, { detectionCorrect: false }),
        ],
        missedFaceCount: 0,
      }),
      photo(3, {
        faces: [face(0, { detectionCorrect: false })],
        missedFaceCount: 2,
      }),
    ];
    const s = computeSummary(setOf(photos));
    expect(s.faceOverall.truePositive).toBe(3);
    expect(s.faceOverall.falsePositive).toBe(2);
    expect(s.faceOverall.missed).toBe(3);
    expect(s.faceOverall.precision).toBeCloseTo(0.6, 6);
    expect(s.faceOverall.recall).toBeCloseTo(0.5, 6);
    expect(s.faceOverall.f1).toBeCloseTo((2 * 0.6 * 0.5) / 1.1, 6);
  });

  it("per-camera breakdown splits by cameraModel and labels unknown", () => {
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        cameraModel: "NIKON D750",
        faces: [face(0, { detectionCorrect: true })],
      }),
      photo(2, {
        cameraModel: "FUJIFILM X-T5",
        faces: [face(0, { detectionCorrect: false })],
        missedFaceCount: 1,
      }),
      photo(3, {
        cameraModel: null,
        faces: [face(0, { detectionCorrect: true })],
      }),
    ];
    const s = computeSummary(setOf(photos));
    const cameras = s.facePerCamera.map((c) => c.cameraModel);
    expect(cameras).toContain("NIKON D750");
    expect(cameras).toContain("FUJIFILM X-T5");
    expect(cameras).toContain("Unknown");
    const xt5 = s.facePerCamera.find((c) => c.cameraModel === "FUJIFILM X-T5")!;
    expect(xt5.stats.precision).toBe(0);
    expect(xt5.stats.recall).toBe(0);
  });

  it("ignores null verdicts in eye / smile / species accuracy", () => {
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        faces: [
          // Mixed verdicts; null entries should not count toward total.
          face(0, {
            leftEyeCorrect: true,
            rightEyeCorrect: false,
            smileCorrect: null,
            speciesCorrect: true,
          }),
          face(1, {
            leftEyeCorrect: true,
            rightEyeCorrect: null,
            smileCorrect: true,
            speciesCorrect: null,
          }),
        ],
      }),
    ];
    const s = computeSummary(setOf(photos));
    expect(s.leftEye).toEqual({ correct: 2, total: 2 });
    expect(s.rightEye).toEqual({ correct: 0, total: 1 });
    expect(s.smile).toEqual({ correct: 1, total: 1 });
    expect(s.species).toEqual({ correct: 1, total: 1 });
  });

  it("sharpness verdict groups aggregate mean and stdev of all four signals", () => {
    const snap = (
      global: number,
      maxEye: number,
      meanEye: number,
      badge: number,
    ): SharpnessSnapshot => ({
      globalScore: global,
      maxEyeSharpness: maxEye,
      meanEyeSharpness: meanEye,
      aiSharpnessBadge1to10: badge,
    });
    // 2 photos tagged intended_bokeh with deliberate spread on globalScore
    // (values 40 and 60 → mean 50, stdev 10) but tight per-eye numbers
    // (80 and 84 → mean 82, stdev 2). This is exactly the pattern we
    // expect to see in real f/1.8 portraits: low + variable global,
    // high + tight per-eye.
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        subjectSharpnessVerdict: "intended_bokeh",
        sharpnessSignalsSnapshot: snap(40, 80, 75, 4),
      }),
      photo(2, {
        subjectSharpnessVerdict: "intended_bokeh",
        sharpnessSignalsSnapshot: snap(60, 84, 79, 5),
      }),
      photo(3, {
        subjectSharpnessVerdict: "all_sharp",
        sharpnessSignalsSnapshot: snap(90, 92, 91, 9),
      }),
    ];
    const s = computeSummary(setOf(photos));
    const bokeh = s.sharpnessGroups.find((g) => g.verdict === "intended_bokeh")!;
    expect(bokeh.count).toBe(2);
    expect(bokeh.globalScore.mean).toBe(50);
    expect(bokeh.globalScore.stdev).toBe(10);
    expect(bokeh.maxEyeSharpness.mean).toBe(82);
    expect(bokeh.maxEyeSharpness.stdev).toBe(2);

    const allSharp = s.sharpnessGroups.find((g) => g.verdict === "all_sharp")!;
    expect(allSharp.count).toBe(1);
    expect(allSharp.globalScore.mean).toBe(90);
    expect(allSharp.globalScore.stdev).toBe(0); // single value → 0 stdev, not NaN

    const empty = s.sharpnessGroups.find((g) => g.verdict === "all_blurry")!;
    expect(empty.count).toBe(0);
    expect(empty.globalScore.mean).toBeNull();
  });

  it("landmark accuracy + conditional eye accuracy stratify classifier vs landmark errors", () => {
    // 4 faces total, all with both eye-state verdicts filled:
    //   face A: landmark ok,    L ✓, R ✓
    //   face B: landmark ok,    L ✓, R ✕      ← classifier missed a real eye
    //   face C: landmark wrong, L ✕, R ✕      ← landmark on eyebrow, classifier said "closed"
    //   face D: landmark wrong, L ✓, R ✕      ← rare: classifier still right despite bad crop
    //
    // Global left-eye:   3/4 = 75%
    // Global right-eye:  1/4 = 25%
    // landmark ok only L: 2/2 = 100%   ← fair classifier score on good crops
    // landmark ok only R: 1/2 = 50%
    // Landmark accuracy: 2/4 = 50%
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        faces: [
          face(0, { landmarkCorrect: true, leftEyeCorrect: true, rightEyeCorrect: true }),
          face(1, { landmarkCorrect: true, leftEyeCorrect: true, rightEyeCorrect: false }),
        ],
      }),
      photo(2, {
        faces: [
          face(0, { landmarkCorrect: false, leftEyeCorrect: false, rightEyeCorrect: false }),
          face(1, { landmarkCorrect: false, leftEyeCorrect: true, rightEyeCorrect: false }),
        ],
      }),
    ];
    const s = computeSummary(setOf(photos));
    expect(s.landmark).toEqual({ correct: 2, total: 4 });
    expect(s.leftEye).toEqual({ correct: 3, total: 4 });
    expect(s.rightEye).toEqual({ correct: 1, total: 4 });
    expect(s.leftEyeGivenLandmarkOk).toEqual({ correct: 2, total: 2 });
    expect(s.rightEyeGivenLandmarkOk).toEqual({ correct: 1, total: 2 });
  });

  it("conditional eye accuracy skips faces where landmark is null (not yet judged)", () => {
    // landmarkCorrect=null means "user hasn't judged the landmark" —
    // those faces should not contribute to the conditional accuracy
    // because we can't tell whether the eye result was a classifier
    // win or a lucky landmark.
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {
        faces: [
          face(0, { landmarkCorrect: null, leftEyeCorrect: true }),
          face(1, { landmarkCorrect: true, leftEyeCorrect: false }),
        ],
      }),
    ];
    const s = computeSummary(setOf(photos));
    expect(s.leftEye).toEqual({ correct: 1, total: 2 }); // both face-judgments count
    expect(s.leftEyeGivenLandmarkOk).toEqual({ correct: 0, total: 1 }); // only the one with confirmed landmark
  });

  it("judgedPhotos counts any photo with at least one filled field", () => {
    const photos: BenchmarkPhotoRecord[] = [
      photo(1, {}),
      photo(2, { missedFaceCount: 1 }),
      photo(3, { subjectSharpnessVerdict: "all_sharp" }),
      photo(4, { faces: [face(0, { detectionCorrect: true })] }),
      photo(5, { faces: [face(0, {})] }), // all-null faces → still unjudged
      photo(6, { faces: [face(0, { landmarkCorrect: true })] }), // landmark verdict alone counts
    ];
    const s = computeSummary(setOf(photos));
    expect(s.totalPhotos).toBe(6);
    expect(s.judgedPhotos).toBe(4);
  });
});

describe("benchmarkStore — slugify (mirrors Rust slugifier)", () => {
  it.each([
    ["D750 — smoke test #1", "d750-smoke-test-1"],
    ["  hello  world  ", "hello-world"],
    ["___", "untitled"],
    ["", "untitled"],
    ["Already-OK", "already-ok"],
    ["trailing!!!", "trailing"],
  ])("slugify(%j) === %j", (input, expected) => {
    expect(slugify(input)).toBe(expected);
  });
});

// Compile-time check that BenchmarkSummary stays exported.
const _typecheck: BenchmarkSummary | null = null;
void _typecheck;
