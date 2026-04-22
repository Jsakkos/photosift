use crate::ai::eye::{NormBox, NormPoint};
use crate::ai::AiProviderStatus;
use anyhow::{anyhow, Result};
use image::RgbImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub bbox: NormBox,
    pub left_eye: NormPoint,
    pub right_eye: NormPoint,
    pub confidence: f64,
}

pub trait FaceProvider: Send + Sync {
    /// Detect faces in an RGB image. YuNet was trained on BGR-formatted
    /// OpenCV inputs; real providers are expected to handle the
    /// channel-order conversion internally. The mock ignores the input.
    fn detect(&self, rgb: &RgbImage) -> Result<Vec<DetectedFace>>;
}

/// Real face detector backed by the YuNet 2023mar ONNX model.
///
/// YuNet is an anchor-free SSD-style detector with three feature-pyramid
/// levels (strides 8/16/32). Each stride emits three heads:
/// - `cls_{stride}` — classification score (sigmoid baked in).
/// - `obj_{stride}` — objectness score (sigmoid baked in).
/// - `bbox_{stride}` — 14 channels: 4 for bbox `(dx,dy,log_w,log_h)` +
///   10 for five landmarks `(dx,dy)` each.
///
/// The detection pipeline: letterbox input to 640×640, run inference,
/// decode candidates per anchor with score = cls × obj, filter by
/// confidence, NMS on the remainder, then un-letterbox the surviving
/// detections back to normalized image coordinates.
pub struct YuNetProvider {
    session: Mutex<Session>,
}

const INPUT_SIDE: usize = 640;
const STRIDES: [u32; 3] = [8, 16, 32];
const CONF_THRESHOLD: f32 = 0.5;
const NMS_IOU_THRESHOLD: f32 = 0.3;

/// Emits one `log::info!` the first time `detect()` observes the output
/// tensor ranges — tells us whether cls/obj are post-sigmoid (values in
/// `[0,1]`) or raw logits (can be far outside that range).
static DIAG_LOGGED: AtomicBool = AtomicBool::new(false);

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl YuNetProvider {
    /// Load the ONNX model. Tries CUDA execution provider first when
    /// `try_cuda` is true, falls back to CPU on any error. Returns the
    /// provider along with the status that actually applied so the UI
    /// can reflect the real backend.
    pub fn load(model_path: &Path, try_cuda: bool) -> Result<(Self, AiProviderStatus)> {
        let (mut session, status) = if try_cuda {
            match Self::build_session(model_path, true) {
                Ok(s) => {
                    log::info!("YuNet loaded on CUDA (GPU execution)");
                    (s, AiProviderStatus::Cuda)
                }
                Err(e) => {
                    log::warn!("YuNet CUDA load failed ({}); falling back to CPU", e);
                    let session = Self::build_session(model_path, false)?;
                    log::info!("YuNet loaded on CPU");
                    (session, AiProviderStatus::Cpu)
                }
            }
        } else {
            let session = Self::build_session(model_path, false)?;
            log::info!("YuNet loaded on CPU");
            (session, AiProviderStatus::Cpu)
        };

        // Prime the session with one dummy inference so the first
        // user-visible photo doesn't absorb the JIT / allocator cost.
        // A failure here is non-fatal — we still return the loaded
        // provider; the first real inference just pays the warmup tax.
        if let Err(e) = Self::warmup(&mut session) {
            log::warn!("YuNet warmup failed (continuing anyway): {:#}", e);
        }

        Ok((
            YuNetProvider {
                session: Mutex::new(session),
            },
            status,
        ))
    }

    fn warmup(session: &mut Session) -> Result<()> {
        let t0 = Instant::now();
        let dummy = vec![0.0_f32; 3 * INPUT_SIDE * INPUT_SIDE];
        let tensor = Tensor::from_array((
            vec![1_i64, 3, INPUT_SIDE as i64, INPUT_SIDE as i64],
            dummy,
        ))
        .map_err(|e| anyhow!("warmup tensor: {}", e))?;
        let _ = session
            .run(ort::inputs!["input" => tensor])
            .map_err(|e| anyhow!("warmup run: {}", e))?;
        log::info!(
            "YuNet warmup complete in {:.1}ms",
            t0.elapsed().as_secs_f64() * 1000.0
        );
        Ok(())
    }

    fn build_session(model_path: &Path, cuda: bool) -> Result<Session> {
        let builder = Session::builder()
            .map_err(|e| anyhow!("ort Session::builder failed: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort with_optimization_level failed: {}", e))?;
        let mut builder = if cuda {
            use ort::ep::CUDAExecutionProvider;
            // Force an explicit error if CUDA registration fails. Without
            // this, ORT silently falls back to CPU at session creation —
            // the session still runs, just on CPU, with no way for the
            // caller to tell the difference. error_on_failure makes the
            // fallback in `load()` above work the way it's documented.
            builder
                .with_execution_providers([CUDAExecutionProvider::default().build().error_on_failure()])
                .map_err(|e| anyhow!("ort with_execution_providers(cuda) failed: {}", e))?
        } else {
            use ort::ep::CPUExecutionProvider;
            builder
                .with_execution_providers([CPUExecutionProvider::default().build()])
                .map_err(|e| anyhow!("ort with_execution_providers(cpu) failed: {}", e))?
        };
        builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort commit_from_file failed: {}", e))
    }
}

/// Letterbox-resize an RGB image into a flat f32 NCHW buffer sized
/// 1×3×640×640 with BGR channel order. YuNet 2023mar (opencv_zoo) was
/// trained on BGR-formatted inputs from OpenCV's `cv2.imread`, so we
/// match that convention rather than feeding RGB directly. Passing
/// grayscale-replicated bytes — what this pipeline did before — left
/// the model blind to skin-tone cues and depressed recall.
///
/// `scale` = model-pixels per source-pixel. `pad_x`/`pad_y` are offsets
/// (in model-pixel units) of the source image's top-left corner within
/// the 640×640 canvas.
struct Letterbox {
    data: Vec<f32>,
    scale: f32,
    pad_x: u32,
    pad_y: u32,
}

fn letterbox_to_640(rgb: &RgbImage) -> Letterbox {
    let (src_w, src_h) = rgb.dimensions();
    let side = INPUT_SIDE as f32;
    let scale = (side / src_w as f32).min(side / src_h as f32);
    let new_w = ((src_w as f32) * scale).round() as u32;
    let new_h = ((src_h as f32) * scale).round() as u32;
    let pad_x = ((INPUT_SIDE as u32).saturating_sub(new_w)) / 2;
    let pad_y = ((INPUT_SIDE as u32).saturating_sub(new_h)) / 2;

    // Box-sampling `thumbnail` instead of `resize(..., Triangle)`: on a
    // 4928×3264 input the quality-preserving Triangle filter takes ~1.4s
    // per photo on CPU because its kernel width scales with the ratio.
    // Box sampling averages fixed rectangles of input pixels and is
    // ~30× faster at large downscales; the resulting 640×425 image is
    // more than good enough for YuNet, which was trained on similarly
    // downsampled inputs.
    let resized = image::imageops::thumbnail(rgb, new_w, new_h);

    // Build [1, 3, 640, 640] NCHW as a flat Vec with BGR layout.
    // Channel 0 = B, channel 1 = G, channel 2 = R. Padding stays zero.
    let plane = INPUT_SIDE * INPUT_SIDE;
    let mut data = vec![0.0_f32; 3 * plane];
    for y in 0..new_h {
        for x in 0..new_w {
            let p = resized.get_pixel(x, y).0;
            let ty = (pad_y + y) as usize;
            let tx = (pad_x + x) as usize;
            let pos = ty * INPUT_SIDE + tx;
            data[pos] = p[2] as f32;            // B
            data[plane + pos] = p[1] as f32;    // G
            data[2 * plane + pos] = p[0] as f32; // R
        }
    }
    Letterbox {
        data,
        scale,
        pad_x,
        pad_y,
    }
}

/// Intermediate detection in model-pixel coordinates (pre-NMS).
#[derive(Debug, Clone)]
struct RawDetection {
    // bbox in model-pixel space
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    // 5 landmarks (x,y) in model-pixel space
    landmarks: [(f32, f32); 5],
    score: f32,
}

fn decode_level(
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    stride: u32,
    apply_sigmoid: bool,
) -> Vec<RawDetection> {
    let grid_side = INPUT_SIDE as u32 / stride;
    let n_anchors = (grid_side * grid_side) as usize;
    debug_assert_eq!(cls.len(), n_anchors);
    debug_assert_eq!(obj.len(), n_anchors);
    debug_assert_eq!(bbox.len(), n_anchors * 4);
    debug_assert_eq!(kps.len(), n_anchors * 10);

    let mut out = Vec::with_capacity(32);
    let s = stride as f32;
    for row in 0..grid_side {
        for col in 0..grid_side {
            let i = (row * grid_side + col) as usize;
            let (c, o) = if apply_sigmoid {
                (sigmoid(cls[i]), sigmoid(obj[i]))
            } else {
                (cls[i], obj[i])
            };
            let score = c * o;
            if !(score >= CONF_THRESHOLD) {
                // `!(score >= T)` also rejects NaN, unlike `score < T`.
                continue;
            }
            let bbase = i * 4;
            let dx = bbox[bbase];
            let dy = bbox[bbase + 1];
            let log_w = bbox[bbase + 2];
            let log_h = bbox[bbase + 3];
            let cx = (dx + col as f32) * s;
            let cy = (dy + row as f32) * s;
            let w = log_w.exp() * s;
            let h = log_h.exp() * s;

            let kbase = i * 10;
            let mut landmarks = [(0.0_f32, 0.0_f32); 5];
            for k in 0..5 {
                let lx = (kps[kbase + 2 * k] + col as f32) * s;
                let ly = (kps[kbase + 2 * k + 1] + row as f32) * s;
                landmarks[k] = (lx, ly);
            }
            out.push(RawDetection {
                cx,
                cy,
                w,
                h,
                landmarks,
                score,
            });
        }
    }
    out
}

fn iou(a: &RawDetection, b: &RawDetection) -> f32 {
    let ax1 = a.cx - a.w * 0.5;
    let ay1 = a.cy - a.h * 0.5;
    let ax2 = ax1 + a.w;
    let ay2 = ay1 + a.h;
    let bx1 = b.cx - b.w * 0.5;
    let by1 = b.cy - b.h * 0.5;
    let bx2 = bx1 + b.w;
    let by2 = by1 + b.h;

    let ix1 = ax1.max(bx1);
    let iy1 = ay1.max(by1);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn nms(mut candidates: Vec<RawDetection>, iou_threshold: f32) -> Vec<RawDetection> {
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<RawDetection> = Vec::with_capacity(candidates.len());
    for det in candidates {
        if kept.iter().any(|k| iou(k, &det) > iou_threshold) {
            continue;
        }
        kept.push(det);
    }
    kept
}

/// Convert a model-space RawDetection into a DetectedFace with
/// normalized (0-1) coordinates on the ORIGINAL input image.
fn finalize(det: RawDetection, lb: &Letterbox, src_w: u32, src_h: u32) -> DetectedFace {
    let inv_scale = 1.0 / lb.scale;
    // Un-letterbox: subtract pad, then scale back to source pixels.
    let to_src_x = |mx: f32| ((mx - lb.pad_x as f32) * inv_scale).clamp(0.0, (src_w as f32) - 1.0);
    let to_src_y = |my: f32| ((my - lb.pad_y as f32) * inv_scale).clamp(0.0, (src_h as f32) - 1.0);

    let x1 = to_src_x(det.cx - det.w * 0.5);
    let y1 = to_src_y(det.cy - det.h * 0.5);
    let x2 = to_src_x(det.cx + det.w * 0.5);
    let y2 = to_src_y(det.cy + det.h * 0.5);
    let bbox = NormBox {
        x: (x1 / src_w as f32) as f64,
        y: (y1 / src_h as f32) as f64,
        w: ((x2 - x1) / src_w as f32) as f64,
        h: ((y2 - y1) / src_h as f32) as f64,
    };

    // Landmarks: YuNet 2023mar emits 5 points. Indices 0 and 1 are the
    // eyes (order depends on model convention — right_eye then left_eye
    // from subject's perspective). Rather than rely on that, sort by x
    // so our `left_eye` is always the one on the viewer's left.
    let e0 = (
        to_src_x(det.landmarks[0].0) / src_w as f32,
        to_src_y(det.landmarks[0].1) / src_h as f32,
    );
    let e1 = (
        to_src_x(det.landmarks[1].0) / src_w as f32,
        to_src_y(det.landmarks[1].1) / src_h as f32,
    );
    let (le, re) = if e0.0 <= e1.0 { (e0, e1) } else { (e1, e0) };

    DetectedFace {
        bbox,
        left_eye: NormPoint {
            x: le.0 as f64,
            y: le.1 as f64,
        },
        right_eye: NormPoint {
            x: re.0 as f64,
            y: re.1 as f64,
        },
        confidence: det.score as f64,
    }
}

impl FaceProvider for YuNetProvider {
    fn detect(&self, rgb: &RgbImage) -> Result<Vec<DetectedFace>> {
        let (src_w, src_h) = rgb.dimensions();
        if src_w == 0 || src_h == 0 {
            return Ok(Vec::new());
        }

        let lb = letterbox_to_640(rgb);
        // ort 2.x accepts `(shape, Vec<T>)` for Tensor construction;
        // avoids the ndarray-version mismatch between the ort and
        // workspace copies of the crate.
        let input_tensor = Tensor::from_array((
            vec![1_i64, 3, INPUT_SIDE as i64, INPUT_SIDE as i64],
            lb.data.clone(),
        ))
        .map_err(|e| anyhow!("ort Tensor::from_array failed: {}", e))?;

        // Extract tensor data inside the lock scope (SessionOutputs
        // borrows from the session), then process candidates after the
        // guard drops. Each extracted slice is cloned into a Vec so
        // the decoder can own its inputs.
        //
        // YuNet 2023mar outputs 12 tensors total: per-stride
        // {cls, obj, bbox, kps}. bbox head is 4-channel (dx/dy/log_w/log_h);
        // landmark deltas live in the separate kps head (10-channel).
        let head_data: Vec<(u32, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>)> = {
            let mut session = self.session.lock().unwrap_or_else(|p| p.into_inner());
            let outputs = session
                .run(ort::inputs!["input" => input_tensor])
                .map_err(|e| anyhow!("ort session.run failed: {}", e))?;

            let mut heads: Vec<(u32, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>)> =
                Vec::with_capacity(STRIDES.len());
            for &stride in &STRIDES {
                let cls_name = format!("cls_{}", stride);
                let obj_name = format!("obj_{}", stride);
                let bbox_name = format!("bbox_{}", stride);
                let kps_name = format!("kps_{}", stride);

                let (_, cls) = outputs
                    .get(cls_name.as_str())
                    .ok_or_else(|| anyhow!("missing output {}", cls_name))?
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("extract {}: {}", cls_name, e))?;
                let (_, obj) = outputs
                    .get(obj_name.as_str())
                    .ok_or_else(|| anyhow!("missing output {}", obj_name))?
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("extract {}: {}", obj_name, e))?;
                let (_, bbox) = outputs
                    .get(bbox_name.as_str())
                    .ok_or_else(|| anyhow!("missing output {}", bbox_name))?
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("extract {}: {}", bbox_name, e))?;
                let (_, kps) = outputs
                    .get(kps_name.as_str())
                    .ok_or_else(|| anyhow!("missing output {}", kps_name))?
                    .try_extract_tensor::<f32>()
                    .map_err(|e| anyhow!("extract {}: {}", kps_name, e))?;

                heads.push((stride, cls.to_vec(), obj.to_vec(), bbox.to_vec(), kps.to_vec()));
            }
            heads
        };

        // YuNet exports vary: some bake sigmoid into the cls/obj heads
        // (values in `[0,1]`), others emit raw logits. The opencv_zoo
        // 2023mar model we bundle is the logit variant — confirmed by
        // reading OpenCV's `FaceDetectorYN::Impl::postProcess_*`, which
        // applies sigmoid before multiplying cls × obj. We auto-detect
        // instead of hard-coding so a swap to a post-sigmoid export
        // keeps working without another fix.
        let global_max_cls = head_data
            .iter()
            .flat_map(|(_, cls, _, _, _)| cls.iter().copied())
            .fold(f32::NEG_INFINITY, f32::max);
        let global_max_obj = head_data
            .iter()
            .flat_map(|(_, _, obj, _, _)| obj.iter().copied())
            .fold(f32::NEG_INFINITY, f32::max);
        let apply_sigmoid = global_max_cls > 1.5 || global_max_obj > 1.5;

        if !DIAG_LOGGED.swap(true, Ordering::SeqCst) {
            log::info!(
                "ai::face YuNet diag: max_cls={:.4} max_obj={:.4} apply_sigmoid={} conf_threshold={}",
                global_max_cls,
                global_max_obj,
                apply_sigmoid,
                CONF_THRESHOLD
            );
        }

        let mut candidates: Vec<RawDetection> = Vec::new();
        for (stride, cls, obj, bbox, kps) in &head_data {
            candidates.extend(decode_level(cls, obj, bbox, kps, *stride, apply_sigmoid));
        }
        let kept = nms(candidates, NMS_IOU_THRESHOLD);
        log::debug!(
            "ai::face {}x{} -> {} faces (sigmoid={})",
            src_w,
            src_h,
            kept.len(),
            apply_sigmoid
        );
        Ok(kept
            .into_iter()
            .map(|d| finalize(d, &lb, src_w, src_h))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letterbox_preserves_aspect() {
        use image::{ImageBuffer, Rgb, RgbImage};
        // Square source → zero padding.
        let sq: RgbImage = ImageBuffer::from_fn(100, 100, |_, _| Rgb([128, 128, 128]));
        let lb = letterbox_to_640(&sq);
        assert_eq!(lb.pad_x, 0);
        assert_eq!(lb.pad_y, 0);
        // Wide source → vertical padding.
        let wide: RgbImage = ImageBuffer::from_fn(200, 100, |_, _| Rgb([128, 128, 128]));
        let lb2 = letterbox_to_640(&wide);
        assert_eq!(lb2.pad_x, 0);
        assert!(lb2.pad_y > 0);
    }

    #[test]
    fn test_letterbox_feeds_bgr_channel_order() {
        use image::{ImageBuffer, Rgb, RgbImage};
        // Pure R=200, G=50, B=20 so each channel is distinguishable.
        // Square input so the content fills the 640×640 canvas with no padding
        // and we can sample any interior pixel without hitting pad=0.
        let src: RgbImage = ImageBuffer::from_fn(320, 320, |_, _| Rgb([200, 50, 20]));
        let lb = letterbox_to_640(&src);

        let plane = INPUT_SIDE * INPUT_SIDE;
        let center = (INPUT_SIDE / 2) * INPUT_SIDE + (INPUT_SIDE / 2);

        // YuNet was trained via OpenCV's imread (BGR). Tensor channel 0 = B,
        // channel 1 = G, channel 2 = R. Filter::Triangle can round integer
        // samples by a pixel so allow ±2.
        assert!(
            (lb.data[center] - 20.0).abs() < 2.0,
            "channel 0 should be B≈20, got {}",
            lb.data[center]
        );
        assert!(
            (lb.data[plane + center] - 50.0).abs() < 2.0,
            "channel 1 should be G≈50, got {}",
            lb.data[plane + center]
        );
        assert!(
            (lb.data[2 * plane + center] - 200.0).abs() < 2.0,
            "channel 2 should be R≈200, got {}",
            lb.data[2 * plane + center]
        );
    }

    #[test]
    fn test_decode_level_empty_below_threshold() {
        // All anchors have score 0 → no detections.
        let n = (INPUT_SIDE / 8) * (INPUT_SIDE / 8);
        let cls = vec![0.0_f32; n];
        let obj = vec![0.0_f32; n];
        let bbox = vec![0.0_f32; n * 4];
        let kps = vec![0.0_f32; n * 10];
        let dets = decode_level(&cls, &obj, &bbox, &kps, 8, false);
        assert!(dets.is_empty());
    }

    #[test]
    fn test_decode_level_accepts_strong_sigmoid_anchor() {
        // One anchor with cls=obj=1.0 (post-sigmoid) → score 1.0 > 0.6.
        let n = (INPUT_SIDE / 8) * (INPUT_SIDE / 8);
        let mut cls = vec![0.0_f32; n];
        let mut obj = vec![0.0_f32; n];
        cls[0] = 1.0;
        obj[0] = 1.0;
        let bbox = vec![0.0_f32; n * 4];
        let kps = vec![0.0_f32; n * 10];
        let dets = decode_level(&cls, &obj, &bbox, &kps, 8, false);
        assert_eq!(dets.len(), 1);
    }

    #[test]
    fn test_decode_level_applies_sigmoid_to_logits() {
        // Strong positive logits → sigmoid drives product close to 1.
        let n = (INPUT_SIDE / 8) * (INPUT_SIDE / 8);
        let mut cls = vec![-10.0_f32; n];
        let mut obj = vec![-10.0_f32; n];
        cls[0] = 6.0;
        obj[0] = 6.0;
        let bbox = vec![0.0_f32; n * 4];
        let kps = vec![0.0_f32; n * 10];
        let dets = decode_level(&cls, &obj, &bbox, &kps, 8, true);
        assert_eq!(dets.len(), 1, "anchor with logit=6 should pass after sigmoid");

        let dets_raw = decode_level(&cls, &obj, &bbox, &kps, 8, false);
        // Without sigmoid, logit=6 × logit=6 = 36 (passes), but most
        // anchors are cls=-10, obj=-10, product=100 (also passes).
        // This demonstrates why the raw-logit path requires sigmoid.
        assert!(dets_raw.len() > 100, "raw logits produce junk — most anchors pass");
    }

    #[test]
    fn test_nms_drops_overlap() {
        let mk = |cx, cy, score| RawDetection {
            cx,
            cy,
            w: 100.0,
            h: 100.0,
            landmarks: [(0.0, 0.0); 5],
            score,
        };
        let dets = vec![mk(100.0, 100.0, 0.9), mk(110.0, 110.0, 0.8), mk(500.0, 500.0, 0.85)];
        let kept = nms(dets, 0.3);
        // First and third survive; second overlaps first.
        assert_eq!(kept.len(), 2);
        assert!((kept[0].score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_iou_self_is_one() {
        let d = RawDetection {
            cx: 50.0,
            cy: 50.0,
            w: 40.0,
            h: 40.0,
            landmarks: [(0.0, 0.0); 5],
            score: 1.0,
        };
        assert!((iou(&d, &d) - 1.0).abs() < 1e-6);
    }

    /// Live-model smoke test: loads the bundled YuNet from the source
    /// tree, runs inference on a synthetic uniform image (no faces
    /// expected). Confirms the end-to-end path — letterbox, tensor
    /// construction, session.run, output extraction, and decoder —
    /// executes without panicking and returns an empty vec.
    ///
    /// Gated with `#[ignore]` because the `load-dynamic` ort feature
    /// needs the onnxruntime DLL discoverable at runtime (via
    /// `ORT_DYLIB_PATH` or system PATH). Run manually with:
    ///   cargo test --lib ai::face::tests::test_yunet_runs_inference -- --ignored --nocapture
    ///
    /// This does NOT assert face detection works on real images (that
    /// requires a fixture with a known face, still a follow-up). It
    /// DOES catch ABI breakage between ort version updates.
    #[test]
    #[ignore]
    fn test_yunet_runs_inference_on_blank_image() {
        use image::{ImageBuffer, Rgb};

        let model_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/ai/models/yunet.onnx");
        if !model_path.exists() {
            eprintln!("skipping: bundled yunet.onnx not present at {:?}", model_path);
            return;
        }

        let (provider, _status) = match YuNetProvider::load(&model_path, false) {
            Ok(v) => v,
            Err(e) => {
                // If ORT runtime DLLs aren't available in the test env
                // this will fail — not a regression, just a missing
                // test-env dep. Log and skip.
                eprintln!("skipping: YuNet load failed ({})", e);
                return;
            }
        };

        // Uniform mid-gray RGB — no faces possible.
        let img: RgbImage = ImageBuffer::from_fn(300, 400, |_, _| Rgb([128, 128, 128]));
        let faces = provider.detect(&img).expect("detect should not error");
        assert!(faces.is_empty(), "expected 0 faces on uniform gray, got {}", faces.len());
    }
}
