//! Throwaway benchmark server (Rust/axum). Mirrors python/server.py.

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::Path as AxPath,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use fast_image_resize::{images::Image, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const PREVIEW_EDGE: u32 = 2560;
const THUMB_EDGE: u32 = 400;
const ANALYSIS_EDGE: u32 = 1024;
const YUNET_SIDE: usize = 640;
const CONF: f32 = 0.5;
const NMS_IOU: f32 = 0.3;
const ARC_DST: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}
fn cache() -> &'static PathBuf {
    static C: OnceLock<PathBuf> = OnceLock::new();
    C.get_or_init(|| {
        std::env::var("CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| root().join("cache-rs"))
    })
}
fn db() -> rusqlite::Connection {
    let con = rusqlite::Connection::open(cache().join("bench.db")).unwrap();
    con.pragma_update(None, "journal_mode", "WAL").unwrap();
    con
}

#[derive(Default)]
struct Stages(Mutex<HashMap<&'static str, f64>>);
impl Stages {
    fn add(&self, name: &'static str, t0: Instant) -> Instant {
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        *self.0.lock().unwrap().entry(name).or_default() += ms;
        Instant::now()
    }
    fn json(&self) -> Value {
        json!(*self.0.lock().unwrap())
    }
}

// ------------------------------------------------------------------ images

struct Rgb {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

fn decode_jpeg(bytes: &[u8]) -> Result<Rgb> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let mut d = zune_jpeg::JpegDecoder::new_with_options(
        bytes,
        DecoderOptions::default()
            .jpeg_set_out_colorspace(ColorSpace::RGB)
            .set_strict_mode(false)
            .set_max_width(16384)
            .set_max_height(16384),
    );
    let px = d.decode().map_err(|e| anyhow!("{e:?}"))?;
    let info = d.info().ok_or_else(|| anyhow!("no info"))?;
    Ok(Rgb { w: info.width as u32, h: info.height as u32, px })
}

fn resize(src: &Rgb, nw: u32, nh: u32, filter: FilterType) -> Result<Rgb> {
    let s = fast_image_resize::images::ImageRef::new(src.w, src.h, &src.px, PixelType::U8x3)?;
    let mut dst = Image::new(nw, nh, PixelType::U8x3);
    Resizer::new().resize(&s, &mut dst, &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter)))?;
    Ok(Rgb { w: nw, h: nh, px: dst.into_vec() })
}

fn fit(src: &Rgb, edge: u32, filter: FilterType) -> Result<Rgb> {
    let s = edge as f64 / src.w.max(src.h) as f64;
    if s >= 1.0 {
        return Ok(Rgb { w: src.w, h: src.h, px: src.px.clone() });
    }
    resize(src, (src.w as f64 * s).round() as u32, (src.h as f64 * s).round() as u32, filter)
}

fn encode_jpeg(img: &Rgb, quality: u8, path: &Path) -> Result<()> {
    let enc = jpeg_encoder::Encoder::new_file(path, quality)?;
    enc.encode(&img.px, img.w as u16, img.h as u16, jpeg_encoder::ColorType::Rgb)?;
    Ok(())
}

fn gray(img: &Rgb) -> Vec<f32> {
    img.px
        .chunks_exact(3)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect()
}

// ------------------------------------------------------------------ ingest

fn find_jpegs(data: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= data.len() {
        if !(data[i] == 0xFF && data[i + 1] == 0xD8 && data[i + 2] == 0xFF) {
            i += 1;
            continue;
        }
        let m = data[i + 3];
        if !matches!(m, 0xC0..=0xCF | 0xDB | 0xDD | 0xE0..=0xEF | 0xFE) {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        let mut end = None;
        while j + 4 <= data.len() && data[j] == 0xFF {
            let marker = data[j + 1];
            if marker == 0xD9 {
                end = Some(j + 2);
                break;
            }
            let seg = u16::from_be_bytes([data[j + 2], data[j + 3]]) as usize;
            if marker == 0xDA {
                let start = j + 2 + seg;
                end = data[start.min(data.len())..]
                    .windows(2)
                    .position(|w| w == [0xFF, 0xD9])
                    .map(|k| start + k + 2);
                break;
            }
            j += 2 + seg;
        }
        match end {
            Some(e) => {
                if e - i > 10_000 {
                    out.push((i, e));
                }
                i = e;
            }
            None => i += 1,
        }
    }
    out
}

fn exif_datetime(jpeg: &[u8], raw: &[u8]) -> Option<String> {
    let r = exif::Reader::new();
    let ex = r
        .read_from_container(&mut std::io::Cursor::new(jpeg))
        .ok()
        .filter(|e| e.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).is_some())
        .or_else(|| r.read_from_container(&mut std::io::Cursor::new(raw)).ok())?;
    ex.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string())
}

fn dct32() -> &'static [[f64; 32]; 32] {
    static M: OnceLock<[[f64; 32]; 32]> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = [[0.0; 32]; 32];
        for (u, row) in m.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                let c = if u == 0 { 0.5f64.sqrt() } else { 1.0 };
                *v = ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 64.0).cos() * c * 0.5;
            }
        }
        m
    })
}

fn phash(thumb: &Rgb) -> Result<i64> {
    let small = resize(thumb, 32, 32, FilterType::Bilinear)?;
    let g = gray(&small);
    let m = dct32();
    // D · P · Dᵀ, only the 8×8 low-frequency corner is needed.
    let mut tmp = [[0.0f64; 32]; 8];
    for u in 0..8 {
        for x in 0..32 {
            tmp[u][x] = (0..32).map(|y| m[u][y] * g[y * 32 + x] as f64).sum();
        }
    }
    let mut block = [0.0f64; 64];
    for u in 0..8 {
        for v in 0..8 {
            block[u * 8 + v] = (0..32).map(|x| tmp[u][x] * m[v][x]).sum();
        }
    }
    let mut ac: Vec<f64> = block[1..].to_vec();
    ac.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ac[ac.len() / 2];
    Ok(block.iter().fold(0u64, |bits, &v| (bits << 1) | (v > median) as u64) as i64)
}

struct Row {
    id: i64,
    name: String,
    path: String,
    sha: String,
    taken: Option<String>,
    w: u32,
    h: u32,
    phash: i64,
}

fn is_raw(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("nef" | "raf")
    )
}

fn ingest_one(path: &Path, id: i64, st: &Stages) -> Result<Option<Row>> {
    let t = Instant::now();
    let raw = std::fs::read(path)?;
    let t = st.add("read", t);
    let sha = hex::encode(Sha256::digest(&raw));
    let t = st.add("hash", t);
    if !is_raw(path) {
        return Ok(None);
    }
    let (s, e) = *find_jpegs(&raw).iter().max_by_key(|(s, e)| e - s).context("no jpeg")?;
    let jpeg = &raw[s..e];
    let t = st.add("extract", t);
    let taken = exif_datetime(jpeg, &raw);
    let t = st.add("exif", t);
    let full = decode_jpeg(jpeg)?;
    let t = st.add("decode", t);
    let preview = fit(&full, PREVIEW_EDGE, FilterType::CatmullRom)?;
    let thumb = fit(&preview, THUMB_EDGE, FilterType::CatmullRom)?;
    let t = st.add("resize", t);
    encode_jpeg(&preview, 85, &cache().join(format!("preview/{id}.jpg")))?;
    encode_jpeg(&thumb, 80, &cache().join(format!("thumb/{id}.jpg")))?;
    let t = st.add("encode_write", t);
    let ph = phash(&thumb)?;
    st.add("phash", t);
    Ok(Some(Row {
        id,
        name: path.file_name().unwrap().to_string_lossy().into(),
        path: path.to_string_lossy().into(),
        sha,
        taken,
        w: full.w,
        h: full.h,
        phash: ph,
    }))
}

#[derive(Deserialize)]
struct IngestReq {
    src: String,
}

fn ingest(req: IngestReq) -> Result<Value> {
    let t0 = Instant::now();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&req.src)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let ids: HashMap<PathBuf, i64> =
        files.iter().filter(|p| is_raw(p)).enumerate().map(|(i, p)| (p.clone(), i as i64 + 1)).collect();
    let st = Stages::default();
    let rows: Vec<Row> = files
        .par_iter()
        .map(|p| ingest_one(p, ids.get(p).copied().unwrap_or(0), &st))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let t = Instant::now();
    let mut con = db();
    let tx = con.transaction()?;
    tx.execute("DELETE FROM photos", [])?;
    tx.execute("DELETE FROM faces", [])?;
    for r in &rows {
        tx.execute(
            "INSERT INTO photos(id,name,path,sha256,taken_at,w,h,phash) VALUES(?,?,?,?,?,?,?,?)",
            rusqlite::params![r.id, r.name, r.path, r.sha, r.taken, r.w, r.h, r.phash],
        )?;
    }
    tx.commit()?;
    st.add("db", t);
    Ok(json!({"files": files.len(), "photos": rows.len(), "wall_ms": t0.elapsed().as_secs_f64() * 1000.0,
              "stage_ms": st.json(), "workers": rayon::current_num_threads()}))
}

// ------------------------------------------------------------------ enrich

struct Models {
    yunet: Mutex<Session>,
    sface: Mutex<Session>,
}

fn session(path: &Path, cuda: bool) -> Result<Session> {
    // ort's builder errors carry non-Send state, so stringify them for anyhow.
    let e = |e: ort::Error<_>| anyhow!("{e}");
    let b = Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3).map_err(e)?;
    let mut b = if cuda {
        b.with_execution_providers([ort::ep::CUDAExecutionProvider::default().build().error_on_failure()])
            .map_err(|e| anyhow!("{e}"))?
    } else {
        b.with_execution_providers([ort::ep::CPUExecutionProvider::default().build()]).map_err(|e| anyhow!("{e}"))?
    };
    b.commit_from_file(path).map_err(|e| anyhow!("{e}"))
}

fn models(device: &str) -> Result<&'static Models> {
    static CPU: OnceLock<Models> = OnceLock::new();
    static GPU: OnceLock<Models> = OnceLock::new();
    let cuda = device == "cuda";
    let cell = if cuda { &GPU } else { &CPU };
    if let Some(m) = cell.get() {
        return Ok(m);
    }
    let mut yunet = session(&root().join("models/yunet.onnx"), cuda)?;
    let mut sface = session(&root().join("models/sface.onnx"), cuda)?;
    yunet.run(ort::inputs!["input" => Tensor::from_array((vec![1i64, 3, 640, 640], vec![0f32; 3 * 640 * 640]))?])?;
    sface.run(ort::inputs!["data" => Tensor::from_array((vec![1i64, 3, 112, 112], vec![0f32; 3 * 112 * 112]))?])?;
    let _ = cell.set(Models { yunet: Mutex::new(yunet), sface: Mutex::new(sface) });
    Ok(cell.get().unwrap())
}

#[derive(Clone)]
struct Face {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kps: [(f32, f32); 5],
    score: f32,
}

fn iou(a: &Face, b: &Face) -> f32 {
    let ix = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0);
    let iy = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let inter = ix * iy;
    inter / (a.w * a.h + b.w * b.h - inter)
}

fn yunet_detect(m: &Models, img: &Rgb) -> Result<Vec<Face>> {
    let scale = (YUNET_SIDE as f32 / img.w as f32).min(YUNET_SIDE as f32 / img.h as f32);
    let (nw, nh) = ((img.w as f32 * scale).round() as u32, (img.h as f32 * scale).round() as u32);
    let (px, py) = ((YUNET_SIDE as u32 - nw) / 2, (YUNET_SIDE as u32 - nh) / 2);
    let small = resize(img, nw, nh, FilterType::Box)?;
    let plane = YUNET_SIDE * YUNET_SIDE;
    let mut data = vec![0f32; 3 * plane];
    for y in 0..nh as usize {
        for x in 0..nw as usize {
            let s = (y * nw as usize + x) * 3;
            let d = (y + py as usize) * YUNET_SIDE + x + px as usize;
            data[d] = small.px[s + 2] as f32;
            data[plane + d] = small.px[s + 1] as f32;
            data[2 * plane + d] = small.px[s] as f32;
        }
    }
    let input = Tensor::from_array((vec![1i64, 3, YUNET_SIDE as i64, YUNET_SIDE as i64], data))?;
    let heads: Vec<(usize, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>)> = {
        let mut sess = m.yunet.lock().unwrap();
        let out = sess.run(ort::inputs!["input" => input])?;
        let mut v = Vec::new();
        for s in [8usize, 16, 32] {
            let get = |n: String| -> Result<Vec<f32>> {
                Ok(out.get(n.as_str()).ok_or_else(|| anyhow!("missing {n}"))?.try_extract_tensor::<f32>()?.1.to_vec())
            };
            v.push((s, get(format!("cls_{s}"))?, get(format!("obj_{s}"))?, get(format!("bbox_{s}"))?, get(format!("kps_{s}"))?));
        }
        v
    };
    let apply_sig = heads.iter().flat_map(|h| h.1.iter()).fold(f32::MIN, |a, &b| a.max(b)) > 1.5;
    let sig = |x: f32| if apply_sig { 1.0 / (1.0 + (-x).exp()) } else { x };
    let mut cands = Vec::new();
    for (s, cls, obj, bbox, kps) in &heads {
        let g = YUNET_SIDE / s;
        let sf = *s as f32;
        for i in 0..cls.len() {
            let score = sig(cls[i]) * sig(obj[i]);
            if !(score >= CONF) {
                continue;
            }
            let (col, row) = ((i % g) as f32, (i / g) as f32);
            let (cx, cy) = ((bbox[i * 4] + col) * sf, (bbox[i * 4 + 1] + row) * sf);
            let (bw, bh) = (bbox[i * 4 + 2].exp() * sf, bbox[i * 4 + 3].exp() * sf);
            let mut k = [(0f32, 0f32); 5];
            for (j, kp) in k.iter_mut().enumerate() {
                *kp = ((kps[i * 10 + 2 * j] + col) * sf, (kps[i * 10 + 2 * j + 1] + row) * sf);
            }
            cands.push(Face { x: cx - bw / 2.0, y: cy - bh / 2.0, w: bw, h: bh, kps: k, score });
        }
    }
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut kept: Vec<Face> = Vec::new();
    for c in cands {
        if kept.iter().all(|k| iou(k, &c) <= NMS_IOU) {
            kept.push(c);
        }
    }
    let (fx, fy) = (px as f32, py as f32);
    Ok(kept
        .into_iter()
        .map(|f| Face {
            x: (f.x - fx) / scale,
            y: (f.y - fy) / scale,
            w: f.w / scale,
            h: f.h / scale,
            kps: f.kps.map(|(x, y)| ((x - fx) / scale, (y - fy) / scale)),
            score: f.score,
        })
        .collect())
}

/// Least-squares similarity transform (Umeyama, 2D closed form via complex
/// numbers): dst ≈ a·src + b with a = scale·e^{iθ}. Returns [m00 m01 m02; m10 m11 m12].
fn similarity(src: &[(f32, f32); 5]) -> [f32; 6] {
    let n = 5.0;
    let (msx, msy) = src.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    let (mdx, mdy) = ARC_DST.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    let (mut re, mut im, mut den) = (0.0f32, 0.0f32, 0.0f32);
    for (s, d) in src.iter().zip(ARC_DST.iter()) {
        let (sx, sy, dx, dy) = (s.0 - msx, s.1 - msy, d.0 - mdx, d.1 - mdy);
        re += dx * sx + dy * sy;
        im += dy * sx - dx * sy;
        den += sx * sx + sy * sy;
    }
    let (a, b) = (re / den, im / den);
    [a, -b, mdx - (a * msx - b * msy), b, a, mdy - (b * msx + a * msy)]
}

fn warp112(img: &Rgb, m: &[f32; 6]) -> Vec<f32> {
    // Invert the 2×3 affine, then bilinear-sample each destination pixel.
    let det = m[0] * m[4] - m[1] * m[3];
    let (ia, ib, ic, id) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    let plane = 112 * 112;
    let mut out = vec![0f32; 3 * plane];
    for y in 0..112 {
        for x in 0..112 {
            let (dx, dy) = (x as f32 - m[2], y as f32 - m[5]);
            let (sx, sy) = (ia * dx + ib * dy, ic * dx + id * dy);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            for c in 0..3 {
                let px = |xx: f32, yy: f32| -> f32 {
                    if xx < 0.0 || yy < 0.0 || xx >= img.w as f32 || yy >= img.h as f32 {
                        0.0
                    } else {
                        img.px[(yy as usize * img.w as usize + xx as usize) * 3 + c] as f32
                    }
                };
                let v = px(x0, y0) * (1.0 - fx) * (1.0 - fy)
                    + px(x0 + 1.0, y0) * fx * (1.0 - fy)
                    + px(x0, y0 + 1.0) * (1.0 - fx) * fy
                    + px(x0 + 1.0, y0 + 1.0) * fx * fy;
                out[c * plane + y * 112 + x] = v; // RGB planes, as SFace expects
            }
        }
    }
    out
}

fn laplacian_var(g: &[f32], w: usize, h: usize) -> f64 {
    let (mut sum, mut sq, mut n) = (0f64, 0f64, 0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let v = (g[i - 1] + g[i + 1] + g[i - w] + g[i + w] - 4.0 * g[i]) as f64;
            sum += v;
            sq += v * v;
            n += 1.0;
        }
    }
    sq / n - (sum / n).powi(2)
}

struct Enriched {
    id: i64,
    w: u32,
    h: u32,
    faces: Vec<Face>,
    embs: Vec<Vec<f32>>,
    sharp: f64,
    expo: (f64, f64, f64),
}

fn enrich_one(id: i64, m: &Models, st: &Stages) -> Result<Enriched> {
    let t = Instant::now();
    let img = decode_jpeg(&std::fs::read(cache().join(format!("preview/{id}.jpg")))?)?;
    let t = st.add("decode", t);
    let faces = yunet_detect(m, &img)?;
    let t = st.add("detect", t);
    let mut embs = Vec::new();
    for f in &faces {
        let crop = warp112(&img, &similarity(&f.kps));
        let input = Tensor::from_array((vec![1i64, 3, 112, 112], crop))?;
        let mut sess = m.sface.lock().unwrap();
        let out = sess.run(ort::inputs!["data" => input])?;
        embs.push(out["fc1"].try_extract_tensor::<f32>()?.1.to_vec());
    }
    let t = st.add("embed", t);
    let small = fit(&img, ANALYSIS_EDGE, FilterType::Box)?;
    let g = gray(&small);
    let sharp = laplacian_var(&g, small.w as usize, small.h as usize);
    let n = g.len() as f64;
    let mean = g.iter().map(|&v| v as f64).sum::<f64>() / n;
    let hi = g.iter().filter(|&&v| v > 250.0).count() as f64 / n;
    let lo = g.iter().filter(|&&v| v < 5.0).count() as f64 / n;
    st.add("sharp_expo", t);
    Ok(Enriched { id, w: img.w, h: img.h, faces, embs, sharp, expo: (mean, hi, lo) })
}

#[derive(Deserialize)]
struct EnrichReq {
    device: String,
}

fn enrich(req: EnrichReq) -> Result<Value> {
    let tl = Instant::now();
    let m = models(&req.device)?;
    let load_ms = tl.elapsed().as_secs_f64() * 1000.0;
    let t0 = Instant::now();
    let st = Stages::default();
    let ids: Vec<i64> = {
        let con = db();
        let mut q = con.prepare("SELECT id FROM photos ORDER BY id")?;
        let v = q.query_map([], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
        v
    };
    let results = ids.par_iter().map(|&id| enrich_one(id, m, &st)).collect::<Result<Vec<_>>>()?;
    let t = Instant::now();
    let mut con = db();
    let tx = con.transaction()?;
    tx.execute("DELETE FROM faces", [])?;
    let mut n_faces = 0;
    for r in &results {
        tx.execute(
            "UPDATE photos SET sharpness=?, mean_luma=?, clip_hi=?, clip_lo=? WHERE id=?",
            rusqlite::params![r.sharp, r.expo.0, r.expo.1, r.expo.2, r.id],
        )?;
        for (f, e) in r.faces.iter().zip(&r.embs) {
            let blob: Vec<u8> = e.iter().flat_map(|v| v.to_le_bytes()).collect();
            tx.execute(
                "INSERT INTO faces(photo_id,x,y,w,h,score,embedding) VALUES(?,?,?,?,?,?,?)",
                rusqlite::params![r.id, f.x / r.w as f32, f.y / r.h as f32, f.w / r.w as f32, f.h / r.h as f32, f.score, blob],
            )?;
            n_faces += 1;
        }
    }
    tx.commit()?;
    st.add("db", t);
    Ok(json!({"device": req.device, "photos": ids.len(), "faces": n_faces, "model_load_ms": load_ms,
              "wall_ms": t0.elapsed().as_secs_f64() * 1000.0, "stage_ms": st.json()}))
}

// ------------------------------------------------------------------ serve

fn err500(e: anyhow::Error) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await?
}

async fn cached(kind: &'static str, id: i64) -> Response {
    match tokio::fs::read(cache().join(format!("{kind}/{id}.jpg"))).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "public, max-age=3600")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn query_json(sql: &str, cols: &[&str]) -> Result<Value> {
    let con = db();
    let mut q = con.prepare(sql)?;
    let rows = q
        .query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v: rusqlite::types::Value = r.get(i)?;
                o.insert(
                    c.to_string(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(n) => json!(n),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(s) => json!(s),
                        rusqlite::types::Value::Blob(_) => Value::Null,
                    },
                );
            }
            Ok(Value::Object(o))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(rows))
}

#[tokio::main]
async fn main() -> Result<()> {
    std::fs::create_dir_all(cache().join("preview"))?;
    std::fs::create_dir_all(cache().join("thumb"))?;
    db().execute_batch(
        "CREATE TABLE IF NOT EXISTS photos(id INTEGER PRIMARY KEY, name TEXT, path TEXT, sha256 TEXT,
           taken_at TEXT, w INT, h INT, phash INTEGER, sharpness REAL, mean_luma REAL, clip_hi REAL, clip_lo REAL);
         CREATE TABLE IF NOT EXISTS faces(id INTEGER PRIMARY KEY, photo_id INT, x REAL, y REAL, w REAL, h REAL,
           score REAL, embedding BLOB);",
    )?;
    let app = Router::new()
        .route("/", get(|| async { Html(std::fs::read_to_string(root().join("viewer/index.html")).unwrap()) }))
        .route("/api/health", get(|| async { Json(json!({"impl": "rust", "pid": std::process::id()})) }))
        .route(
            "/api/ingest",
            post(|Json(r): Json<IngestReq>| async move { blocking(move || ingest(r)).await.map(Json).map_err(err500) }),
        )
        .route(
            "/api/enrich",
            post(|Json(r): Json<EnrichReq>| async move { blocking(move || enrich(r)).await.map(Json).map_err(err500) }),
        )
        .route(
            "/api/photos",
            get(|| async {
                blocking(|| {
                    query_json(
                        "SELECT id,name,taken_at,w,h,phash,sharpness FROM photos ORDER BY taken_at, id",
                        &["id", "name", "taken_at", "w", "h", "phash", "sharpness"],
                    )
                })
                .await
                .map(Json)
                .map_err(err500)
            }),
        )
        .route(
            "/api/faces",
            get(|| async {
                blocking(|| {
                    query_json(
                        "SELECT photo_id,x,y,w,h,score FROM faces ORDER BY photo_id",
                        &["photo_id", "x", "y", "w", "h", "score"],
                    )
                })
                .await
                .map(Json)
                .map_err(err500)
            }),
        )
        .route("/preview/{id}", get(|AxPath(id): AxPath<i64>| cached("preview", id)))
        .route("/thumb/{id}", get(|AxPath(id): AxPath<i64>| cached("thumb", id)));
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8701);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
