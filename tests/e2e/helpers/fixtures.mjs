// Named fixture builders that produce SeedRequest payloads.
//
// Each fn returns a plain JSON object the Rust seeder
// (src-tauri/src/commands/testing.rs) consumes. Adding a fixture is
// purely a JS change — the Rust side never grows.

const IMG = (n) => `sample_${String(n).padStart(2, "0")}.jpg`;
const DATE = (i) => {
  const h = 10 + Math.floor(i / 6);
  const m = (i % 6) * 10;
  return `2026-04-15T${String(h % 24).padStart(2, "0")}:${String(m).padStart(2, "0")}:00`;
};

const photo = (i, overrides = {}) => ({
  fixture: IMG(((i - 1) % 12) + 1),
  exifDate: DATE(i),
  ...overrides,
});

// --------- Library --------------------------------------------------

export const libraryEmpty = () => ({
  shoots: [],
  settings: { onboardedWizard: true },
});

export const libraryOneShoot = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => photo(i + 1)),
    },
  ],
  settings: { onboardedWizard: true },
});

export const libraryThreeShoots = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 8 }, (_, i) => photo(i + 1)),
    },
    {
      slug: "studio-portraits",
      date: "2026-04-10",
      photos: Array.from({ length: 4 }, (_, i) => photo(i + 1)),
    },
    {
      slug: "concert-night",
      date: "2026-03-28",
      photos: Array.from({ length: 12 }, (_, i) => photo(i + 1)),
    },
  ],
  settings: { onboardedWizard: true },
});

// --------- Triage ---------------------------------------------------

export const triageCold = () => ({
  ...libraryOneShoot(),
  settings: { onboardedWizard: true, onboardedTriage: true },
});

export const triageColdWithFaces = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => {
        const base = photo(i + 1);
        if (i < 4) {
          return {
            ...base,
            faceCount: 1,
            eyesOpenCount: 2,
            faces: [
              {
                bboxX: 0.35,
                bboxY: 0.3,
                bboxW: 0.3,
                bboxH: 0.35,
                leftEyeOpen: 1,
                rightEyeOpen: 1,
              },
            ],
          };
        }
        return base;
      }),
    },
  ],
  settings: { onboardedWizard: true, onboardedTriage: true },
});

export const triageColdWithSharpness = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => ({
        ...photo(i + 1),
        sharpnessScore: 50 + i * 10,
        qualityScore: 40 + i * 5,
      })),
    },
  ],
  settings: { onboardedWizard: true, onboardedTriage: true },
});

export const triageWithAiJudgments = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => ({
        ...photo(i + 1),
        curatorJudgment: {
          composition: 4 + (i % 7),
          aesthetic: 3 + (i % 8),
          isKeeper: i % 3 !== 0,
          suggestedFlag: i % 3 === 0 ? "reject" : "keep",
          reason: i % 3 === 0 ? "Soft focus." : "Strong composition.",
        },
      })),
    },
  ],
  settings: { onboardedWizard: true, onboardedTriage: true },
});

export const triageComplete = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => ({
        ...photo(i + 1),
        flag: i % 2 === 0 ? "pick" : "reject",
        starRating: i % 2 === 0 ? 3 : 0,
      })),
    },
  ],
  settings: { onboardedWizard: true, onboardedTriage: true },
});

export const triageCompleteAllRejects = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => ({
        ...photo(i + 1),
        flag: "reject",
      })),
    },
  ],
  settings: { onboardedWizard: true, onboardedTriage: true, onboardedSelect: true },
});

// --------- Select ---------------------------------------------------

export const selectBasic = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 4 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: [3, 2, 4, 1][i],
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
  },
});

export const selectBasicWithAi = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 4 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: [3, 2, 4, 1][i],
        faceCount: 1,
        eyesOpenCount: 2,
        sharpnessScore: 60 + i * 10,
        qualityScore: 50 + i * 8,
        faces: [
          {
            bboxX: 0.35,
            bboxY: 0.3,
            bboxW: 0.3,
            bboxH: 0.35,
            leftEyeOpen: 1,
            rightEyeOpen: 1,
            smileScore: 0.55 + i * 0.05,
          },
        ],
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
  },
});

export const selectMixedRatings = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 12 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: Math.min(5, Math.floor(i / 2)),
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
  },
});

export const selectAllRouted = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 4 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 4,
        destination: i % 2 === 0 ? "edit" : "export",
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
  },
});

// --------- Route ----------------------------------------------------

export const routeLowRated = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 6 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 1,
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
    routeMinStar: 3,
  },
});

export const routeMixed = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 8 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 3 + (i % 3),
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
  },
});

export const routeMixedWithDests = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 8 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 3 + (i % 3),
        destination:
          i % 3 === 0 ? "edit" : i % 3 === 1 ? "export" : "unrouted",
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
  },
});

export const routeAllEdit = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 6 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 4,
        destination: "edit",
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
  },
});

export const routeAllExport = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 6 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 4,
        destination: "export",
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: true,
  },
});

// --------- First-run modal not-onboarded variants -------------------

export const triageColdNotOnboarded = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 6 }, (_, i) => photo(i + 1)),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: false,
  },
});

export const selectFirstRunNotOnboarded = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 4 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 3,
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: false,
  },
});

export const routeFirstRunNotOnboarded = () => ({
  shoots: [
    {
      slug: "weekend-walk",
      date: "2026-04-15",
      photos: Array.from({ length: 4 }, (_, i) => ({
        ...photo(i + 1),
        flag: "pick",
        starRating: 4,
      })),
    },
  ],
  settings: {
    onboardedWizard: true,
    onboardedTriage: true,
    onboardedSelect: true,
    onboardedRoute: false,
  },
});

// --------- Onboarding wizard ----------------------------------------

export const wizardFirstRun = () => ({
  shoots: [],
  settings: { onboardedWizard: false },
});
