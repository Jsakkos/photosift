import { afterEach, describe, expect, it, vi } from "vitest";

import {
  ApiError,
  getScan,
  listShoots,
  regroupEvents,
  scanThumbUrl,
  setOutcome,
  slugFor,
  trashDeletes,
} from "./client";
import type { OutcomeResponse } from "./types";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function mockFetch(response: Response) {
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("setOutcome", () => {
  const row: OutcomeResponse = {
    id: 7,
    shoot_id: 1,
    raw_path: null,
    jpeg_path: "C:\\lib\\a.jpg",
    video_path: null,
    sha256: "abc",
    camera: "Cam",
    capture_time_orig: "2024-05-01T10:00:00",
    capture_time: "2024-05-01T10:00:00",
    time_source: "exif",
    orientation: 1,
    width: 640,
    height: 480,
    phash: "18446744073709551615",
    sharpness: null,
    face_sharpness: null,
    mean_luma: null,
    clip_hi: null,
    clip_lo: null,
    moment_idx: null,
    burst_idx: null,
    signal_score: null,
    outcome: "album",
    blooper: false,
    enriched_at: null,
    trashed_at: null,
    xmp_error: null,
  };

  it("posts the outcome to the photo's endpoint and returns the parsed row", async () => {
    const fetchMock = mockFetch(jsonResponse(row));

    const result = await setOutcome(7, "album");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("/api/photos/7/outcome");
    expect(init?.method).toBe("POST");
    expect(JSON.parse(String(init?.body))).toEqual({ outcome: "album" });
    expect(result).toEqual(row);
    // phash stays a string: a 64-bit hash would lose bits as a JS number.
    expect(result.phash).toBe("18446744073709551615");
  });

  it("sends the blooper flag only when given", async () => {
    const fetchMock = mockFetch(jsonResponse(row));

    await setOutcome(7, "library", true);

    const [, init] = fetchMock.mock.calls[0] ?? [];
    expect(JSON.parse(String(init?.body))).toEqual({
      outcome: "library",
      blooper: true,
    });
  });
});

describe("errors", () => {
  it("surfaces the server's error text and status", async () => {
    mockFetch(jsonResponse({ error: "no photo 9" }, { status: 404 }));

    const err = await setOutcome(9, "album").catch((e: unknown) => e);

    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(404);
    expect((err as ApiError).message).toBe("no photo 9");
  });

  it("keeps a plain-text body, such as the Host guard's 403", async () => {
    mockFetch(new Response("unexpected Host header", { status: 403 }));

    await expect(listShoots()).rejects.toMatchObject({
      status: 403,
      message: "unexpected Host header",
    });
  });
});

describe("trashDeletes", () => {
  it("posts to the shoot's trash endpoint without a body", async () => {
    const fetchMock = mockFetch(jsonResponse({ trashed: 2, failed: [] }));

    const report = await trashDeletes(3);

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("/api/shoots/3/trash");
    expect(init?.method).toBe("POST");
    expect(init?.body).toBeUndefined();
    expect(report).toEqual({ trashed: 2, failed: [] });
  });
});

describe("import screen calls", () => {
  it("posts camera offsets in seconds to the scan's events endpoint", async () => {
    const fetchMock = mockFetch(jsonResponse([]));

    await regroupEvents("scan 1", { "Cam A": 10800 });

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("/api/scan/scan%201/events");
    expect(init?.method).toBe("POST");
    expect(JSON.parse(String(init?.body))).toEqual({ camera_offsets: { "Cam A": 10800 } });
  });

  it("unwraps the slug and reports an evicted scan as a 404", async () => {
    mockFetch(jsonResponse({ slug: "Babys-First-Meal" }));
    expect(await slugFor("Baby's First Meal")).toBe("Babys-First-Meal");

    mockFetch(jsonResponse({ error: "unknown scan; scan the card again" }, { status: 404 }));
    await expect(getScan("old")).rejects.toMatchObject({ status: 404 });
  });

  it("addresses scan thumbnails by scan and item", () => {
    expect(scanThumbUrl("abc", 7)).toBe("/media/scan/abc/7");
  });
});
