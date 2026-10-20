//! YuNet face detection, run on the full frame and on overlapping tiles so
//! small background faces (group shots, parties) are not lost to the 640 px
//! network input.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use fast_image_resize::FilterType;
use ort::session::Session;
use ort::value::Tensor;

pub use super::Backend;
use crate::media::render::{resize_with, RgbImage};

pub const MODEL_FILE: &str = "face_detection_yunet_2023mar.onnx";

const SIDE: usize = 640;
const STRIDES: [usize; 3] = [8, 16, 32];
const CONF: f32 = 0.5;
const NMS_IOU: f32 = 0.3;
/// Tiles of the 2x2 grid overlap by this fraction of their own size.
const TILE_OVERLAP: f32 = 0.15;
/// A tile detection this close (fraction of the tile's long side) to an
/// interior tile edge is a face cut in half: the neighbouring view sees it whole.
const EDGE_MARGIN: f32 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// `(x, y, w, h)` normalized to the analysed image, display orientation.
    pub bbox: (f32, f32, f32, f32),
    /// Normalized; order is YuNet's: image-left eye, image-right eye, nose,
    /// image-left mouth corner, image-right mouth corner.
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
}

impl Face {
    /// Landmarks in pixel coordinates of a `w`x`h` image.
    pub fn landmarks_px(&self, w: u32, h: u32) -> [(f32, f32); 5] {
        self.landmarks.map(|(x, y)| (x * w as f32, y * h as f32))
    }
}

pub struct Detector {
    session: Mutex<Session>,
}

/// A candidate in pixel coordinates of the full image.
#[derive(Clone)]
struct Cand {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kps: [(f32, f32); 5],
    score: f32,
}

/// A window onto the image that is letterboxed into one network run.
#[derive(Clone, Copy)]
struct View {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Detector {
    /// `cuda` asks for the GPU; a failure there falls back to CPU (logged).
    pub fn load(models_dir: &Path, cuda: bool) -> Result<(Self, Backend)> {
        let path = models_dir.join(MODEL_FILE);
        if !path.is_file() {
            return Err(anyhow!("{} not found", path.display()));
        }
        let (mut session, backend) = super::load_session(&path, cuda)
            .with_context(|| format!("loading {}", path.display()))?;
        // The first run pays allocator/JIT cost; keep it off the first photo.
        let dummy = Tensor::from_array((
            vec![1i64, 3, SIDE as i64, SIDE as i64],
            vec![0f32; 3 * SIDE * SIDE],
        ))
        .map_err(|e| anyhow!("{e}"))?;
        session
            .run(ort::inputs!["input" => dummy])
            .map_err(|e| anyhow!("warm-up run: {e}"))?;
        Ok((
            Self {
                session: Mutex::new(session),
            },
            backend,
        ))
    }

    /// Faces over the full frame and a 2x2 tile grid, merged by NMS.
    pub fn detect(&self, img: &RgbImage) -> Vec<Face> {
        self.detect_views(img, true)
    }

    /// Single letterboxed pass over the whole frame, for comparison.
    pub fn detect_full_frame(&self, img: &RgbImage) -> Vec<Face> {
        self.detect_views(img, false)
    }

    fn detect_views(&self, img: &RgbImage, tiled: bool) -> Vec<Face> {
        match self.try_detect(img, tiled) {
            Ok(faces) => faces,
            Err(e) => {
                eprintln!("photosift: face detection failed: {e:#}");
                Vec::new()
            }
        }
    }

    fn try_detect(&self, img: &RgbImage, tiled: bool) -> Result<Vec<Face>> {
        let (w, h) = (img.w as usize, img.h as usize);
        if w < 8 || h < 8 {
            return Ok(Vec::new());
        }
        let full = View { x: 0, y: 0, w, h };
        let mut cands = self.run_view(img, full, false)?;
        if tiled {
            for view in tile_views(w, h) {
                cands.extend(self.run_view(img, view, true)?);
            }
        }
        Ok(nms(cands)
            .into_iter()
            .filter_map(|c| to_face(&c, w, h))
            .collect())
    }

    /// Runs the network on `view` and returns detections in full-image pixels.
    /// With `drop_cut` set, detections touching an interior view edge are
    /// discarded.
    fn run_view(&self, img: &RgbImage, view: View, drop_cut: bool) -> Result<Vec<Cand>> {
        let (data, scale, pad_x, pad_y) = letterbox(img, view)?;
        let input = Tensor::from_array((vec![1i64, 3, SIDE as i64, SIDE as i64], data))
            .map_err(|e| anyhow!("{e}"))?;
        // (stride, cls, obj, bbox, kps)
        type Head = (usize, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>);
        let heads: Vec<Head> = {
            let mut sess = self.session.lock().unwrap_or_else(|e| e.into_inner());
            let out = sess
                .run(ort::inputs!["input" => input])
                .map_err(|e| anyhow!("{e}"))?;
            let get = |name: String| -> Result<Vec<f32>> {
                let value = out
                    .get(name.as_str())
                    .ok_or_else(|| anyhow!("model has no output {name}"))?;
                let (_, data) = value
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("{e}"))?;
                Ok(data.to_vec())
            };
            let mut v = Vec::with_capacity(STRIDES.len());
            for s in STRIDES {
                v.push((
                    s,
                    get(format!("cls_{s}"))?,
                    get(format!("obj_{s}"))?,
                    get(format!("bbox_{s}"))?,
                    get(format!("kps_{s}"))?,
                ));
            }
            v
        };
        // The opencv_zoo export emits logits; a post-sigmoid export would stay
        // within 0-1. Detect rather than hard-code.
        let max_head = heads
            .iter()
            .flat_map(|h| h.1.iter().chain(h.2.iter()))
            .fold(f32::MIN, |a, &b| a.max(b));
        let sig = max_head > 1.5;
        let act = |x: f32| if sig { 1.0 / (1.0 + (-x).exp()) } else { x };

        let (vx, vy) = (view.x as f32, view.y as f32);
        let to_img = |mx: f32, my: f32| {
            (
                (mx - pad_x as f32) / scale + vx,
                (my - pad_y as f32) / scale + vy,
            )
        };
        let margin = EDGE_MARGIN * view.w.max(view.h) as f32;
        let (img_w, img_h) = (img.w as f32, img.h as f32);
        let mut out = Vec::new();
        for (stride, cls, obj, bbox, kps) in &heads {
            let grid = SIDE / stride;
            let sf = *stride as f32;
            for i in 0..cls.len().min(grid * grid) {
                let score = act(cls[i]) * act(obj[i]);
                // `is_nan` first: NaN must not slip through as "not below CONF".
                if score.is_nan() || score < CONF {
                    continue;
                }
                let (col, row) = ((i % grid) as f32, (i / grid) as f32);
                let (cx, cy) = ((bbox[i * 4] + col) * sf, (bbox[i * 4 + 1] + row) * sf);
                let (bw, bh) = (bbox[i * 4 + 2].exp() * sf, bbox[i * 4 + 3].exp() * sf);
                let (x0, y0) = to_img(cx - bw / 2.0, cy - bh / 2.0);
                let (x1, y1) = to_img(cx + bw / 2.0, cy + bh / 2.0);
                if drop_cut {
                    let cut = (view.x > 0 && x0 < vx + margin)
                        || (view.y > 0 && y0 < vy + margin)
                        || (view.x + view.w < img.w as usize && x1 > vx + view.w as f32 - margin)
                        || (view.y + view.h < img.h as usize && y1 > vy + view.h as f32 - margin);
                    if cut {
                        continue;
                    }
                }
                let mut k = [(0f32, 0f32); 5];
                for (j, kp) in k.iter_mut().enumerate() {
                    *kp = to_img(
                        (kps[i * 10 + 2 * j] + col) * sf,
                        (kps[i * 10 + 2 * j + 1] + row) * sf,
                    );
                }
                if x1 <= x0 || y1 <= y0 || x0 >= img_w || y0 >= img_h || x1 <= 0.0 || y1 <= 0.0 {
                    continue;
                }
                out.push(Cand {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                    kps: k,
                    score,
                });
            }
        }
        Ok(out)
    }
}

/// The 2x2 grid; tiles overlap by `TILE_OVERLAP` of their own size.
fn tile_views(w: usize, h: usize) -> Vec<View> {
    let tw = ((w as f32) / (2.0 - TILE_OVERLAP)).ceil() as usize;
    let th = ((h as f32) / (2.0 - TILE_OVERLAP)).ceil() as usize;
    let (tw, th) = (tw.min(w), th.min(h));
    let mut v = Vec::with_capacity(4);
    for y in [0, h - th] {
        for x in [0, w - tw] {
            v.push(View { x, y, w: tw, h: th });
        }
    }
    v
}

/// Letterboxes `view` of `img` into a 640x640 BGR NCHW buffer (YuNet was
/// trained on OpenCV's BGR). Returns `(data, scale, pad_x, pad_y)` where
/// `scale` is network px per image px.
fn letterbox(img: &RgbImage, view: View) -> Result<(Vec<f32>, f32, usize, usize)> {
    let crop = if view.x == 0 && view.y == 0 && view.w == img.w as usize && view.h == img.h as usize
    {
        None
    } else {
        let mut px = Vec::with_capacity(view.w * view.h * 3);
        for y in view.y..view.y + view.h {
            let start = (y * img.w as usize + view.x) * 3;
            px.extend_from_slice(&img.px[start..start + view.w * 3]);
        }
        Some(RgbImage {
            w: view.w as u32,
            h: view.h as u32,
            px,
        })
    };
    let src = crop.as_ref().unwrap_or(img);
    let scale = (SIDE as f32 / src.w as f32).min(SIDE as f32 / src.h as f32);
    let nw = ((src.w as f32 * scale).round() as usize).clamp(1, SIDE);
    let nh = ((src.h as f32 * scale).round() as usize).clamp(1, SIDE);
    let (pad_x, pad_y) = ((SIDE - nw) / 2, (SIDE - nh) / 2);
    let small =
        resize_with(src, nw as u32, nh as u32, FilterType::Box).map_err(|e| anyhow!("{e}"))?;
    let plane = SIDE * SIDE;
    let mut data = vec![0f32; 3 * plane];
    for y in 0..nh {
        for x in 0..nw {
            let s = (y * nw + x) * 3;
            let d = (y + pad_y) * SIDE + x + pad_x;
            data[d] = f32::from(small.px[s + 2]);
            data[plane + d] = f32::from(small.px[s + 1]);
            data[2 * plane + d] = f32::from(small.px[s]);
        }
    }
    Ok((data, scale, pad_x, pad_y))
}

fn iou(a: &Cand, b: &Cand) -> f32 {
    let ix = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0);
    let iy = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let inter = ix * iy;
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn nms(mut cands: Vec<Cand>) -> Vec<Cand> {
    cands.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Cand> = Vec::new();
    for c in cands {
        if kept.iter().all(|k| iou(k, &c) <= NMS_IOU) {
            kept.push(c);
        }
    }
    kept
}

fn to_face(c: &Cand, w: usize, h: usize) -> Option<Face> {
    let (fw, fh) = (w as f32, h as f32);
    let nx = |v: f32| (v / fw).clamp(0.0, 1.0);
    let ny = |v: f32| (v / fh).clamp(0.0, 1.0);
    let (x0, y0, x1, y1) = (nx(c.x), ny(c.y), nx(c.x + c.w), ny(c.y + c.h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Face {
        bbox: (x0, y0, x1 - x0, y1 - y0),
        landmarks: c.kps.map(|(x, y)| (nx(x), ny(y))),
        score: c.score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_frame_with_the_stated_overlap() {
        let (w, h) = (2560, 1707);
        let tiles = tile_views(w, h);
        assert_eq!(tiles.len(), 4);
        // Every pixel column/row is inside some tile; neighbours overlap ~15%.
        let t = tiles[0];
        let overlap = (t.x + t.w) as f32 - tiles[1].x as f32;
        assert!(
            ((overlap / t.w as f32) - TILE_OVERLAP).abs() < 0.01,
            "{overlap}"
        );
        assert_eq!(tiles[1].x + tiles[1].w, w);
        assert_eq!(tiles[2].y + tiles[2].h, h);
        assert!(tiles.iter().all(|t| t.w < w && t.h < h));
    }

    #[test]
    fn nms_keeps_the_higher_score_of_overlapping_boxes() {
        let c = |x: f32, score: f32| Cand {
            x,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            kps: [(0.0, 0.0); 5],
            score,
        };
        let kept = nms(vec![c(0.0, 0.6), c(1.0, 0.9), c(50.0, 0.7)]);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].score - 0.9).abs() < 1e-6);
    }
}
