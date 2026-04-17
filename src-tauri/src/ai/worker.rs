use crate::ai::eye::{eye_crop_pixels, EyeStateProvider};
use crate::ai::face::FaceProvider;
use crate::ai::sharpness::{laplacian_variance, normalize_sharpness};
use crate::ai::AiJob;
use crate::db::schema::{Database, FaceRow};
use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use image::GenericImageView;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct WorkerHandle {
    pub sender: Sender<AiJob>,
    pub cancel: Arc<AtomicBool>,
}

/// Process a single job end-to-end. Public for test access.
pub fn process_job(
    db: &mut Database,
    job: &AiJob,
    faces_provider: &dyn FaceProvider,
    eyes_provider: &dyn EyeStateProvider,
) -> Result<()> {
    let preview_path = Path::new(&job.preview_path);
    let img = image::open(preview_path)
        .with_context(|| format!("open preview {}", job.preview_path))?;
    let (img_w, img_h) = img.dimensions();
    let gray = img.to_luma8();

    let faces = faces_provider.detect(&gray)?;

    let mut face_rows = Vec::with_capacity(faces.len());
    let mut open_count = 0;
    for face in &faces {
        let l_crop = eye_crop_pixels(&face.left_eye, &face.bbox, img_w, img_h);
        let r_crop = eye_crop_pixels(&face.right_eye, &face.bbox, img_w, img_h);
        let l_img = image::imageops::crop_imm(&gray, l_crop.x, l_crop.y, l_crop.w, l_crop.h).to_image();
        let r_img = image::imageops::crop_imm(&gray, r_crop.x, r_crop.y, r_crop.w, r_crop.h).to_image();

        let l_open = eyes_provider.classify(&l_img)?;
        let r_open = eyes_provider.classify(&r_img)?;
        open_count += l_open + r_open;

        let l_sharp = normalize_sharpness(laplacian_variance(&l_img));
        let r_sharp = normalize_sharpness(laplacian_variance(&r_img));

        face_rows.push(FaceRow {
            photo_id: job.photo_id,
            bbox_x: face.bbox.x, bbox_y: face.bbox.y,
            bbox_w: face.bbox.w, bbox_h: face.bbox.h,
            left_eye_x: face.left_eye.x, left_eye_y: face.left_eye.y,
            right_eye_x: face.right_eye.x, right_eye_y: face.right_eye.y,
            left_eye_open: l_open, right_eye_open: r_open,
            left_eye_sharpness: l_sharp, right_eye_sharpness: r_sharp,
            detection_confidence: face.confidence,
        });
    }

    // Whole-image sharpness on the already-decoded gray image.
    let whole = normalize_sharpness(laplacian_variance(&gray));

    db.delete_faces_for_photo(job.photo_id)?;
    if !face_rows.is_empty() {
        db.insert_faces_batch(&face_rows)?;
    }
    db.mark_ai_analyzed(
        job.photo_id,
        Some(faces.len() as i32),
        Some(open_count),
        Some(whole),
    )?;
    Ok(())
}

/// Main worker loop. Pulls jobs until the channel closes or the cancel flag
/// flips. On cancel, drops remaining queued jobs.
pub fn run_loop(
    rx: Receiver<AiJob>,
    cancel: Arc<AtomicBool>,
    mut db: Database,
    faces_provider: Box<dyn FaceProvider>,
    eyes_provider: Box<dyn EyeStateProvider>,
    on_progress: impl Fn(&AiJob, Result<()>) + Send,
) {
    while let Ok(job) = rx.recv() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let result = process_job(&mut db, &job, faces_provider.as_ref(), eyes_provider.as_ref());
        on_progress(&job, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::{MockEyeProvider, MockFaceProvider};
    use crate::db::schema::Database;
    use image::{ImageBuffer, Luma};
    use tempfile::tempdir;

    fn write_tiny_preview(path: &Path) {
        // 128x128 checkerboard so sharpness > 0.
        let img: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(128, 128, |x, y| {
            Luma([if (x / 4 + y / 4) % 2 == 0 { 0 } else { 255 }])
        });
        img.save(path).unwrap();
    }

    #[test]
    fn test_process_job_writes_faces_and_aggregates() {
        let dir = tempdir().unwrap();
        let preview = dir.path().join("p.jpg");
        write_tiny_preview(&preview);

        let mut db = Database::open(&dir.path().join("t.db")).unwrap();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db.insert_photos_batch(
            shoot_id,
            &[crate::db::schema::tests::sample_insert_for_test(1, "a.nef")],
        ).unwrap();

        let job = AiJob {
            shoot_id,
            photo_id: ids[0],
            preview_path: preview.to_string_lossy().into_owned(),
        };

        process_job(&mut db, &job, &MockFaceProvider::default(), &MockEyeProvider::default()).unwrap();

        let faces = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(faces.len(), 1);
        // MockEyeProvider returns 0,1 alternating — first face gets left=0, right=1.
        assert_eq!(faces[0].left_eye_open, 0);
        assert_eq!(faces[0].right_eye_open, 1);

        let row: (Option<i32>, Option<i32>, Option<String>) = db.conn.query_row(
            "SELECT face_count, eyes_open_count, ai_analyzed_at FROM photos WHERE id = ?1",
            rusqlite::params![ids[0]],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row.0, Some(1));
        assert_eq!(row.1, Some(1));
        assert!(row.2.is_some());
    }
}
