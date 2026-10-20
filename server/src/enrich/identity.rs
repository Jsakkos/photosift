//! SFace identity embeddings: align a detected face to the ArcFace 112x112
//! template, then run the network.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use ort::session::Session;
use ort::value::Tensor;

use super::Backend;
use crate::media::render::RgbImage;

pub const MODEL_FILE: &str = "face_recognition_sface_2021dec.onnx";
pub const EMBEDDING_LEN: usize = 128;
const CROP: usize = 112;

/// Where the five landmarks (eyes, nose, mouth corners) land in the aligned
/// 112x112 crop; the ArcFace reference points SFace was trained against.
pub const ARCFACE_TEMPLATE: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

pub struct Embedder {
    session: Mutex<Session>,
}

impl Embedder {
    /// Like `Detector::load`: CUDA if asked and working, else CPU; the backend
    /// that actually runs is returned.
    pub fn load(models_dir: &Path, cuda: bool) -> Result<(Self, Backend)> {
        let path = models_dir.join(MODEL_FILE);
        if !path.is_file() {
            return Err(anyhow!("{} not found", path.display()));
        }
        let (session, backend) = super::load_session(&path, cuda, |session| {
            let dummy = Tensor::from_array((
                vec![1i64, 3, CROP as i64, CROP as i64],
                vec![0f32; 3 * CROP * CROP],
            ))
            .map_err(|e| anyhow!("{e}"))?;
            session
                .run(ort::inputs!["data" => dummy])
                .map_err(|e| anyhow!("warm-up run: {e}"))?;
            Ok(())
        })
        .with_context(|| format!("loading {}", path.display()))?;
        Ok((
            Self {
                session: Mutex::new(session),
            },
            backend,
        ))
    }

    /// `landmarks` are in pixel coordinates of `img`, in YuNet's order.
    pub fn embed(
        &self,
        img: &RgbImage,
        landmarks: &[(f32, f32); 5],
    ) -> Result<[f32; EMBEDDING_LEN]> {
        let crop = warp112(img, &similarity(landmarks));
        let input = Tensor::from_array((vec![1i64, 3, CROP as i64, CROP as i64], crop))
            .map_err(|e| anyhow!("{e}"))?;
        let mut sess = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let out = sess
            .run(ort::inputs!["data" => input])
            .map_err(|e| anyhow!("{e}"))?;
        let (_, data) = out["fc1"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("{e}"))?;
        data.try_into().map_err(|_| {
            anyhow!(
                "embedding has {} values, expected {EMBEDDING_LEN}",
                data.len()
            )
        })
    }
}

/// Least-squares similarity transform (Umeyama, 2D closed form via complex
/// numbers): `template ~ a*src + b` with `a = scale*e^(i*theta)`. Returns the
/// affine `[m00 m01 m02; m10 m11 m12]` row-major, mapping source pixels to the
/// 112x112 crop.
pub fn similarity(src: &[(f32, f32); 5]) -> [f32; 6] {
    let n = 5.0;
    let mean = |pts: &[(f32, f32); 5]| {
        pts.iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n))
    };
    let (msx, msy) = mean(src);
    let (mdx, mdy) = mean(&ARCFACE_TEMPLATE);
    let (mut re, mut im, mut den) = (0.0f32, 0.0f32, 0.0f32);
    for (s, d) in src.iter().zip(ARCFACE_TEMPLATE.iter()) {
        let (sx, sy, dx, dy) = (s.0 - msx, s.1 - msy, d.0 - mdx, d.1 - mdy);
        re += dx * sx + dy * sy;
        im += dy * sx - dx * sy;
        den += sx * sx + sy * sy;
    }
    // Degenerate (all landmarks coincide): identity scale keeps the crop sane.
    let (a, b) = if den > f32::EPSILON {
        (re / den, im / den)
    } else {
        (1.0, 0.0)
    };
    [
        a,
        -b,
        mdx - (a * msx - b * msy),
        b,
        a,
        mdy - (b * msx + a * msy),
    ]
}

/// Bilinear-samples the aligned 112x112 crop as RGB planes, raw 0-255 floats
/// (SFace takes unnormalized input). Samples outside the image read as black.
fn warp112(img: &RgbImage, m: &[f32; 6]) -> Vec<f32> {
    let det = m[0] * m[4] - m[1] * m[3];
    let (ia, ib, ic, id) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    let (w, h) = (img.w as f32, img.h as f32);
    let plane = CROP * CROP;
    let mut out = vec![0f32; 3 * plane];
    for y in 0..CROP {
        for x in 0..CROP {
            let (dx, dy) = (x as f32 - m[2], y as f32 - m[5]);
            let (sx, sy) = (ia * dx + ib * dy, ic * dx + id * dy);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            for c in 0..3 {
                let px = |xx: f32, yy: f32| -> f32 {
                    if xx < 0.0 || yy < 0.0 || xx >= w || yy >= h {
                        0.0
                    } else {
                        f32::from(img.px[(yy as usize * img.w as usize + xx as usize) * 3 + c])
                    }
                };
                out[c * plane + y * CROP + x] = px(x0, y0) * (1.0 - fx) * (1.0 - fy)
                    + px(x0 + 1.0, y0) * fx * (1.0 - fy)
                    + px(x0, y0 + 1.0) * (1.0 - fx) * fy
                    + px(x0 + 1.0, y0 + 1.0) * fx * fy;
            }
        }
    }
    out
}
