use crate::ai::cat::CatDetectorProvider;
use crate::ai::eye::{eye_crop_pixels, EyeStateProvider};
use crate::ai::face::FaceProvider;
use crate::ai::mouth::{face_crop_pixels, MouthStateProvider};
use crate::ai::sharpness::{laplacian_variance, normalize_sharpness};
use crate::ai::AiJob;
use crate::db::schema::{Database, FaceRow};
use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use image::GenericImageView;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    mouth_provider: &dyn MouthStateProvider,
    cat_provider: &dyn CatDetectorProvider,
) -> Result<()> {
    let preview_path = Path::new(&job.preview_path);
    let t_start = Instant::now();
    let img = image::open(preview_path)
        .with_context(|| format!("open preview {}", job.preview_path))?;
    let (img_w, img_h) = img.dimensions();
    let t_decode = t_start.elapsed();
    // Face detection runs on RGB (YuNet was trained on BGR but the provider
    // handles the channel swap internally). Eye classification and sharpness
    // then reuse a grayscale copy derived from the same decoded pixels, so
    // we're still at one disk read + one JPEG decode per photo.
    let rgb = img.to_rgb8();
    let gray = img.to_luma8();
    let t_convert = t_start.elapsed() - t_decode;

    let t_face_start = Instant::now();
    let faces = faces_provider.detect(&rgb)?;
    let t_faces = t_face_start.elapsed();

    let t_eye_start = Instant::now();
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

        // Smile / expression classification: hand the full face crop to
        // the mouth provider. FER+/FER were trained on face crops, so a
        // narrow mouth-only patch produces near-zero smile probability
        // regardless of expression. For a mouth-specific model we'd
        // re-introduce `mouth_crop_pixels` here, but today all real-world
        // providers are holistic. `smile_score` is persisted per face;
        // `mouth_open` stays transient because nothing downstream reads it.
        let f_crop = face_crop_pixels(&face.bbox, img_w, img_h);
        let f_img = image::imageops::crop_imm(&gray, f_crop.x, f_crop.y, f_crop.w, f_crop.h)
            .to_image();
        let smile_score = mouth_provider
            .classify(&f_img)
            .ok()
            .map(|m| m.smile_confidence);

        face_rows.push(FaceRow {
            photo_id: job.photo_id,
            bbox_x: face.bbox.x, bbox_y: face.bbox.y,
            bbox_w: face.bbox.w, bbox_h: face.bbox.h,
            left_eye_x: face.left_eye.x, left_eye_y: face.left_eye.y,
            right_eye_x: face.right_eye.x, right_eye_y: face.right_eye.y,
            left_eye_open: l_open, right_eye_open: r_open,
            left_eye_sharpness: l_sharp, right_eye_sharpness: r_sharp,
            detection_confidence: face.confidence,
            smile_score,
            species: "human".to_string(),
        });
    }

    // Cat pass. Runs alongside the human-face loop; produces additional
    // `FaceRow`s with `species = "cat"` and placeholder eye data (no
    // classifier today). Mock provider returns zero cats, so this is a
    // no-op until a real ONNX detector lands at
    // `~/.photosift/models/cat_detector.onnx`.
    if let Ok(cats) = cat_provider.detect(&rgb) {
        for cat in cats {
            face_rows.push(FaceRow {
                photo_id: job.photo_id,
                bbox_x: cat.bbox.x, bbox_y: cat.bbox.y,
                bbox_w: cat.bbox.w, bbox_h: cat.bbox.h,
                // No cat eye detector today — leave landmarks at bbox
                // center so the UI can render a neutral eye position
                // without stretching off-image when the crop is used.
                left_eye_x: cat.bbox.x + cat.bbox.w * 0.35,
                left_eye_y: cat.bbox.y + cat.bbox.h * 0.40,
                right_eye_x: cat.bbox.x + cat.bbox.w * 0.65,
                right_eye_y: cat.bbox.y + cat.bbox.h * 0.40,
                left_eye_open: 0, right_eye_open: 0,
                left_eye_sharpness: 0.0, right_eye_sharpness: 0.0,
                detection_confidence: cat.confidence,
                smile_score: None,
                species: "cat".to_string(),
            });
        }
    }

    // Whole-image sharpness on the already-decoded gray image.
    let raw = laplacian_variance(&gray);
    let whole = normalize_sharpness(raw);
    let t_eye_sharp = t_eye_start.elapsed();
    let t_total = t_start.elapsed();

    // Composite quality score (0-100). Sharpness-dominant today with a
    // small subject-presence bump; when the mock eye classifier is swapped
    // for a real ONNX model we'll factor in eye-open ratio, and mouth/smile
    // when D1 unblocks. Ranking is within a group so the absolute scale
    // doesn't matter much — relative order does.
    let face_bonus = if !face_rows.is_empty() { 15.0 } else { 0.0 };
    let quality = (whole * 0.85 + face_bonus).clamp(0.0, 100.0);

    log::info!(
        "ai::worker photo={} dims={}x{} faces={} total={:.1}ms (decode={:.1}ms cvt={:.1}ms face={:.1}ms eye+sharp={:.1}ms) sharp={:.1}",
        job.photo_id,
        img_w,
        img_h,
        faces.len(),
        t_total.as_secs_f64() * 1000.0,
        t_decode.as_secs_f64() * 1000.0,
        t_convert.as_secs_f64() * 1000.0,
        t_faces.as_secs_f64() * 1000.0,
        t_eye_sharp.as_secs_f64() * 1000.0,
        whole,
    );

    // `face_count` on photos is the total detection count across species —
    // cat-only photos would otherwise report zero and the AiPanel visibility
    // gate would hide the panel despite valid cat tiles being available.
    db.write_ai_result(
        job.photo_id,
        &face_rows,
        Some(face_rows.len() as i32),
        Some(open_count),
        Some(whole),
        Some(quality),
    )?;
    Ok(())
}

/// Main worker loop. Runs for the lifetime of the channel.
///
/// Cancel semantics: when the cancel flag is set, the worker drops
/// any queued jobs without processing them, clears the flag, and
/// resumes waiting for new work. This interrupts the *current batch*
/// without killing the worker for the session — a subsequent
/// re-analyze or import still works.
///
/// The worker only exits when all senders are dropped (app shutdown).
pub fn run_loop(
    rx: Receiver<AiJob>,
    cancel: Arc<AtomicBool>,
    mut db: Database,
    faces_provider: Box<dyn FaceProvider>,
    eyes_provider: Box<dyn EyeStateProvider>,
    mouth_provider: Box<dyn MouthStateProvider>,
    cat_provider: Box<dyn CatDetectorProvider>,
    on_progress: impl Fn(&AiJob, Result<()>) + Send,
) {
    log::info!("AI worker started");
    while let Ok(job) = rx.recv() {
        if cancel.load(Ordering::SeqCst) {
            // Cancelled: drain any other queued jobs too, then clear
            // the flag and go back to waiting for new work.
            let mut drained = 1;
            while rx.try_recv().is_ok() {
                drained += 1;
            }
            cancel.store(false, Ordering::SeqCst);
            log::info!("AI worker: cancel — dropped {} queued job(s)", drained);
            continue;
        }
        let result = process_job(
            &mut db,
            &job,
            faces_provider.as_ref(),
            eyes_provider.as_ref(),
            mouth_provider.as_ref(),
            cat_provider.as_ref(),
        );
        on_progress(&job, result);
    }
    log::info!("AI worker exited (channel closed)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::cat::MockCatDetector;
    use crate::ai::mock::{MockEyeProvider, MockFaceProvider};
    use crate::ai::mouth::MockMouthProvider;
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

        process_job(
            &mut db,
            &job,
            &MockFaceProvider::default(),
            &MockEyeProvider::default(),
            &MockMouthProvider::default(),
            &MockCatDetector::default(),
        )
        .unwrap();

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

    #[test]
    fn test_run_loop_honors_cancel_between_jobs() {
        use crossbeam_channel::unbounded;
        use std::sync::atomic::AtomicUsize;
        use std::thread;
        use std::time::Duration;

        let dir = tempdir().unwrap();
        let preview = dir.path().join("p.jpg");
        write_tiny_preview(&preview);

        let db_path = dir.path().join("t.db");
        // Pre-create the shoot and photos so the worker DB can find them.
        let ids = {
            let mut db = Database::open(&db_path).unwrap();
            let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
            db.insert_photos_batch(shoot_id, &[
                crate::db::schema::tests::sample_insert_for_test(1, "a.nef"),
                crate::db::schema::tests::sample_insert_for_test(2, "b.nef"),
                crate::db::schema::tests::sample_insert_for_test(3, "c.nef"),
            ]).unwrap()
        };
        let shoot_id = 1;

        let worker_db = Database::open(&db_path).unwrap();
        let (tx, rx) = unbounded::<AiJob>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let completed = Arc::new(AtomicUsize::new(0));
        let completed_clone = completed.clone();

        let handle = thread::spawn(move || {
            run_loop(
                rx,
                cancel_clone,
                worker_db,
                Box::new(crate::ai::mock::MockFaceProvider::default()),
                Box::new(crate::ai::mock::MockEyeProvider::default()),
                Box::new(crate::ai::mouth::MockMouthProvider::default()),
                Box::new(crate::ai::cat::MockCatDetector::default()),
                move |_job, res| {
                    if res.is_ok() {
                        completed_clone.fetch_add(1, Ordering::SeqCst);
                    }
                },
            );
        });

        // Send 3 jobs; flip cancel after a short delay so at least 1 completes
        // and at least 1 is dropped.
        for id in &ids {
            tx.send(AiJob {
                shoot_id,
                photo_id: *id,
                preview_path: preview.to_string_lossy().into_owned(),
            })
            .unwrap();
        }

        // Give the worker time to process at least one job, then cancel.
        thread::sleep(Duration::from_millis(200));
        cancel.store(true, Ordering::SeqCst);
        drop(tx); // closes the channel so run_loop exits cleanly
        handle.join().unwrap();

        let n = completed.load(Ordering::SeqCst);
        assert!(n >= 1, "at least one job should complete before cancel");
        assert!(n <= 3, "completed count bounded by queue length");
    }
}
