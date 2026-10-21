"""Throwaway benchmark server (Python/FastAPI). Mirrors rust/src/main.rs."""
import hashlib
import io
import os
import sqlite3
import struct
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import cv2
import exifread
import numpy as np
import onnxruntime as ort
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse, HTMLResponse
from PIL import Image
from pydantic import BaseModel

ROOT = Path(__file__).resolve().parent.parent
CACHE = Path(os.environ.get("CACHE_DIR", ROOT / "cache-py"))
WORKERS = int(os.environ.get("WORKERS", os.cpu_count() or 8))
PREVIEW_EDGE, THUMB_EDGE, ANALYSIS_EDGE = 2560, 400, 1024
RAW_EXTS = {".nef", ".raf"}

ort.set_default_logger_severity(3)
# Same CUDA/cuDNN DLL set as the Rust build, so GPU numbers compare like for like.
CUDA_DLL_DIR = Path(os.environ.get("CUDA_DLL_DIR", "C:/Github/photosift/src-tauri/target/debug"))
if CUDA_DLL_DIR.exists():
    # cuDNN 9 loads its sub-libraries via the plain DLL search path, so
    # add_dll_directory alone is not enough — PATH must carry it too.
    os.environ["PATH"] = str(CUDA_DLL_DIR) + os.pathsep + os.environ["PATH"]
    os.add_dll_directory(str(CUDA_DLL_DIR))

app = FastAPI()
CACHE.mkdir(parents=True, exist_ok=True)
(CACHE / "preview").mkdir(exist_ok=True)
(CACHE / "thumb").mkdir(exist_ok=True)
DB_PATH = CACHE / "bench.db"


def db():
    con = sqlite3.connect(DB_PATH)
    con.execute("PRAGMA journal_mode=WAL")
    return con


with db() as con:
    con.executescript(
        """
        CREATE TABLE IF NOT EXISTS photos(id INTEGER PRIMARY KEY, name TEXT, path TEXT, sha256 TEXT,
          taken_at TEXT, w INT, h INT, phash INTEGER, sharpness REAL, mean_luma REAL, clip_hi REAL, clip_lo REAL);
        CREATE TABLE IF NOT EXISTS faces(id INTEGER PRIMARY KEY, photo_id INT, x REAL, y REAL, w REAL, h REAL,
          score REAL, embedding BLOB);
        """
    )


class Stages:
    def __init__(self):
        self.lock = threading.Lock()
        self.ms: dict[str, float] = {}

    def add(self, name, t0):
        dt = (time.perf_counter() - t0) * 1000
        with self.lock:
            self.ms[name] = self.ms.get(name, 0.0) + dt
        return time.perf_counter()


# ---------------------------------------------------------------- ingest

def find_jpegs(data: bytes) -> list[tuple[int, int]]:
    """Every embedded JPEG > 10KB as (start, end_exclusive)."""
    out, pos = [], 0
    while True:
        i = data.find(b"\xff\xd8\xff", pos)
        if i < 0 or i + 4 > len(data):
            return out
        m = data[i + 3]
        if not (0xC0 <= m <= 0xCF or m in (0xDB, 0xDD, 0xFE) or 0xE0 <= m <= 0xEF):
            pos = i + 1
            continue
        # Walk marker segments to SOS, then the first FFD9 ends the scan
        # (entropy-coded data byte-stuffs 0xFF as FF00).
        j = i + 2
        end = -1
        while j + 4 <= len(data):
            if data[j] != 0xFF:
                break
            marker = data[j + 1]
            if marker == 0xD9:
                end = j + 2
                break
            seg_len = struct.unpack_from(">H", data, j + 2)[0]
            if marker == 0xDA:
                k = data.find(b"\xff\xd9", j + 2 + seg_len)
                end = k + 2 if k >= 0 else -1
                break
            j += 2 + seg_len
        if end > 0:
            if end - i > 10_000:
                out.append((i, end))
            pos = end
        else:
            pos = i + 1


def exif_datetime(img: Image.Image, raw: bytes) -> str | None:
    dt = img.getexif().get_ifd(0x8769).get(36867)
    if dt:
        return dt
    tags = exifread.process_file(io.BytesIO(raw), details=False, stop_tag="DateTimeOriginal")
    v = tags.get("EXIF DateTimeOriginal")
    return str(v) if v else None


_DCT32 = np.array([[np.cos((2 * x + 1) * u * np.pi / 64) * (np.sqrt(0.5) if u == 0 else 1.0)
                    for x in range(32)] for u in range(32)]) * 0.5


def phash(img: Image.Image) -> int:
    px = np.asarray(img.convert("L").resize((32, 32), Image.Resampling.BILINEAR), dtype=np.float64)
    dct = _DCT32 @ px @ _DCT32.T
    block = dct[:8, :8].flatten()
    median = np.median(block[1:])
    bits = 0
    for v in block:
        bits = (bits << 1) | int(v > median)
    return bits - (1 << 64) if bits >= (1 << 63) else bits


def ingest_one(path: Path, pid: int, st: Stages):
    t = time.perf_counter()
    raw = path.read_bytes()
    t = st.add("read", t)
    sha = hashlib.sha256(raw).hexdigest()
    t = st.add("hash", t)
    if path.suffix.lower() not in RAW_EXTS:
        return None  # sibling camera JPEG: copied + hashed only
    spans = find_jpegs(raw)
    s, e = max(spans, key=lambda se: se[1] - se[0])
    jpeg = raw[s:e]
    t = st.add("extract", t)
    img = Image.open(io.BytesIO(jpeg))
    taken = exif_datetime(img, raw)
    t = st.add("exif", t)
    w, h = img.size
    img.draft("RGB", (PREVIEW_EDGE, PREVIEW_EDGE))  # libjpeg DCT-domain downscale
    img = img.convert("RGB")
    t = st.add("decode", t)
    img.thumbnail((PREVIEW_EDGE, PREVIEW_EDGE), Image.Resampling.BICUBIC)
    thumb = img.copy()
    thumb.thumbnail((THUMB_EDGE, THUMB_EDGE), Image.Resampling.BICUBIC)
    t = st.add("resize", t)
    img.save(CACHE / "preview" / f"{pid}.jpg", quality=85)
    thumb.save(CACHE / "thumb" / f"{pid}.jpg", quality=80)
    t = st.add("encode_write", t)
    ph = phash(thumb)
    st.add("phash", t)
    return (pid, path.name, str(path), sha, taken, w, h, ph)


class IngestReq(BaseModel):
    src: str


@app.post("/api/ingest")
def ingest(req: IngestReq):
    t0 = time.perf_counter()
    files = sorted(p for p in Path(req.src).iterdir() if p.is_file())
    st = Stages()
    raws = [p for p in files if p.suffix.lower() in RAW_EXTS]
    ids = {p: i + 1 for i, p in enumerate(raws)}
    with ThreadPoolExecutor(WORKERS) as ex:
        rows = [r for r in ex.map(lambda p: ingest_one(p, ids.get(p, 0), st), files) if r]
    t = time.perf_counter()
    with db() as con:
        con.execute("DELETE FROM photos")
        con.execute("DELETE FROM faces")
        con.executemany(
            "INSERT INTO photos(id,name,path,sha256,taken_at,w,h,phash) VALUES(?,?,?,?,?,?,?,?)", rows)
    st.add("db", t)
    return {"files": len(files), "photos": len(rows), "wall_ms": (time.perf_counter() - t0) * 1000,
            "stage_ms": st.ms, "workers": WORKERS}


# ---------------------------------------------------------------- enrich

YUNET_SIDE, CONF, NMS_IOU = 640, 0.5, 0.3
ARC_DST = np.array([[38.2946, 51.6963], [73.5318, 51.5014], [56.0252, 71.7366],
                    [41.5493, 92.3655], [70.7299, 92.2041]], dtype=np.float64)
_sessions: dict[str, tuple] = {}
_infer_lock = threading.Lock()


def sessions(device: str):
    if device not in _sessions:
        so = ort.SessionOptions()
        so.log_severity_level = 3
        prov = ["CUDAExecutionProvider"] if device == "cuda" else ["CPUExecutionProvider"]
        yn = ort.InferenceSession(str(ROOT / "models/yunet.onnx"), so, providers=prov)
        sf = ort.InferenceSession(str(ROOT / "models/sface.onnx"), so, providers=prov)
        assert yn.get_providers()[0] == prov[0], yn.get_providers()
        yn.run(None, {"input": np.zeros((1, 3, YUNET_SIDE, YUNET_SIDE), np.float32)})
        sf.run(None, {"data": np.zeros((1, 3, 112, 112), np.float32)})
        _sessions[device] = (yn, sf)
    return _sessions[device]


def yunet_detect(sess, bgr: np.ndarray):
    h, w = bgr.shape[:2]
    scale = min(YUNET_SIDE / w, YUNET_SIDE / h)
    nw, nh = round(w * scale), round(h * scale)
    px, py = (YUNET_SIDE - nw) // 2, (YUNET_SIDE - nh) // 2
    canvas = np.zeros((YUNET_SIDE, YUNET_SIDE, 3), np.uint8)
    canvas[py:py + nh, px:px + nw] = cv2.resize(bgr, (nw, nh), interpolation=cv2.INTER_AREA)
    blob = canvas.transpose(2, 0, 1)[None].astype(np.float32)
    with _infer_lock:
        outs = dict(zip([o.name for o in sess.get_outputs()], sess.run(None, {"input": blob})))
    boxes, scores, kps_all = [], [], []
    apply_sig = max(outs[f"cls_{s}"].max() for s in (8, 16, 32)) > 1.5
    for s in (8, 16, 32):
        g = YUNET_SIDE // s
        cls, obj = outs[f"cls_{s}"].reshape(-1), outs[f"obj_{s}"].reshape(-1)
        if apply_sig:
            cls, obj = 1 / (1 + np.exp(-cls)), 1 / (1 + np.exp(-obj))
        score = cls * obj
        idx = np.nonzero(score >= CONF)[0]
        if not len(idx):
            continue
        col, row = (idx % g).astype(np.float32), (idx // g).astype(np.float32)
        bb, kp = outs[f"bbox_{s}"].reshape(-1, 4)[idx], outs[f"kps_{s}"].reshape(-1, 10)[idx]
        cx, cy = (bb[:, 0] + col) * s, (bb[:, 1] + row) * s
        bw, bh = np.exp(bb[:, 2]) * s, np.exp(bb[:, 3]) * s
        boxes.append(np.stack([cx - bw / 2, cy - bh / 2, bw, bh], 1))
        scores.append(score[idx])
        kx = (kp[:, 0::2] + col[:, None]) * s
        ky = (kp[:, 1::2] + row[:, None]) * s
        kps_all.append(np.stack([kx, ky], 2))
    if not boxes:
        return []
    boxes, scores, kps_all = np.concatenate(boxes), np.concatenate(scores), np.concatenate(kps_all)
    keep = cv2.dnn.NMSBoxes(boxes.tolist(), scores.tolist(), CONF, NMS_IOU)
    faces = []
    for k in np.array(keep).reshape(-1):
        b = boxes[k]
        to_src = lambda xs, pad: (xs - pad) / scale
        faces.append({
            "box": (to_src(b[0], px), to_src(b[1], py), b[2] / scale, b[3] / scale),
            "kps": np.stack([to_src(kps_all[k][:, 0], px), to_src(kps_all[k][:, 1], py)], 1),
            "score": float(scores[k]),
        })
    return faces


def umeyama(src: np.ndarray, dst: np.ndarray) -> np.ndarray:
    mu_s, mu_d = src.mean(0), dst.mean(0)
    sc, dc = src - mu_s, dst - mu_d
    cov = dc.T @ sc / len(src)
    u, d, vt = np.linalg.svd(cov)
    sgn = np.diag([1.0, np.sign(np.linalg.det(u @ vt))])
    r = u @ sgn @ vt
    scale = np.trace(np.diag(d) @ sgn) / sc.var(0).sum()
    m = np.zeros((2, 3))
    m[:, :2] = scale * r
    m[:, 2] = mu_d - scale * r @ mu_s
    return m


def enrich_one(row, device, st: Stages):
    pid = row[0]
    t = time.perf_counter()
    bgr = cv2.imread(str(CACHE / "preview" / f"{pid}.jpg"), cv2.IMREAD_COLOR)
    t = st.add("decode", t)
    yn, sf = sessions(device)
    faces = yunet_detect(yn, bgr)
    t = st.add("detect", t)
    embs = []
    if faces:
        crops = [cv2.warpAffine(bgr, umeyama(f["kps"], ARC_DST), (112, 112)) for f in faces]
        blob = np.stack([c[:, :, ::-1].transpose(2, 0, 1) for c in crops]).astype(np.float32)
        with _infer_lock:
            embs = [sf.run(None, {"data": blob[i:i + 1]})[0][0] for i in range(len(crops))]
    t = st.add("embed", t)
    h, w = bgr.shape[:2]
    sc = ANALYSIS_EDGE / max(w, h)
    gray = cv2.cvtColor(cv2.resize(bgr, (round(w * sc), round(h * sc)), interpolation=cv2.INTER_AREA),
                        cv2.COLOR_BGR2GRAY)
    sharp = float(cv2.Laplacian(gray, cv2.CV_64F).var())
    expo = (float(gray.mean()), float((gray > 250).mean()), float((gray < 5).mean()))
    st.add("sharp_expo", t)
    return pid, faces, embs, sharp, expo, (w, h)


class EnrichReq(BaseModel):
    device: str = "cpu"


@app.post("/api/enrich")
def enrich(req: EnrichReq):
    t_load = time.perf_counter()
    sessions(req.device)
    load_ms = (time.perf_counter() - t_load) * 1000
    t0 = time.perf_counter()
    st = Stages()
    with db() as con:
        rows = con.execute("SELECT id FROM photos ORDER BY id").fetchall()
    with ThreadPoolExecutor(WORKERS) as ex:
        results = list(ex.map(lambda r: enrich_one(r, req.device, st), rows))
    t = time.perf_counter()
    n_faces = 0
    with db() as con:
        con.execute("DELETE FROM faces")
        for pid, faces, embs, sharp, expo, (w, h) in results:
            con.execute("UPDATE photos SET sharpness=?, mean_luma=?, clip_hi=?, clip_lo=? WHERE id=?",
                        (sharp, *expo, pid))
            for f, e in zip(faces, embs):
                x, y, bw, bh = f["box"]
                con.execute("INSERT INTO faces(photo_id,x,y,w,h,score,embedding) VALUES(?,?,?,?,?,?,?)",
                            (pid, float(x / w), float(y / h), float(bw / w), float(bh / h), f["score"], np.asarray(e, np.float32).tobytes()))
                n_faces += 1
    st.add("db", t)
    return {"device": req.device, "photos": len(rows), "faces": n_faces, "model_load_ms": load_ms,
            "wall_ms": (time.perf_counter() - t0) * 1000, "stage_ms": st.ms}


# ---------------------------------------------------------------- serve

CACHE_HDR = {"Cache-Control": "public, max-age=3600"}


@app.get("/api/health")
def health():
    return {"impl": "python", "pid": os.getpid()}


@app.get("/api/photos")
def photos():
    with db() as con:
        rows = con.execute("SELECT id,name,taken_at,w,h,phash,sharpness FROM photos ORDER BY taken_at, id").fetchall()
    return [dict(zip(("id", "name", "taken_at", "w", "h", "phash", "sharpness"), r)) for r in rows]


@app.get("/api/faces")
def faces():
    with db() as con:
        rows = con.execute("SELECT photo_id,x,y,w,h,score FROM faces ORDER BY photo_id").fetchall()
    return [dict(zip(("photo_id", "x", "y", "w", "h", "score"), r)) for r in rows]


def _cached(kind: str, pid: int):
    p = CACHE / kind / f"{pid}.jpg"
    if not p.exists():
        raise HTTPException(404)
    return FileResponse(p, media_type="image/jpeg", headers=CACHE_HDR)


@app.get("/preview/{pid}")
def preview(pid: int):
    return _cached("preview", pid)


@app.get("/thumb/{pid}")
def thumb(pid: int):
    return _cached("thumb", pid)


@app.get("/")
def index():
    return HTMLResponse((ROOT / "viewer/index.html").read_text(encoding="utf-8"))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=int(os.environ.get("PORT", 8702)), log_level="warning")
