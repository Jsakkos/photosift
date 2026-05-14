use crate::ai::cat::{approximate_cat_face, CatDetectorProvider};
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
///
/// Every classifier argument is optional. The worker degrades
/// gracefully: a `None` face provider skips face detection entirely
/// (still records whole-image sharpness); a `None` eye / mouth / cat
/// provider writes NULL into the corresponding face-row columns. No
/// mock fallbacks — see `EyeProviderKind::Absent` for context.
pub fn process_job(
    db: &mut Database,
    job: &AiJob,
    faces_provider: Option<&dyn FaceProvider>,
    eyes_provider: Option<&dyn EyeStateProvider>,
    mouth_provider: Option<&dyn MouthStateProvider>,
    cat_provider: Option<&dyn CatDetectorProvider>,
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
    // Face provider absent → skip detection entirely. Whole-image
    // sharpness below still runs so photos get *some* AI signal.
    let faces = match faces_provider {
        Some(p) => p.detect(&rgb)?,
        None => Vec::new(),
    };
    let t_faces = t_face_start.elapsed();

    let t_eye_start = Instant::now();
    let mut face_rows = Vec::with_capacity(faces.len());
    let mut open_count = 0;
    for face in &faces {
        let l_crop = eye_crop_pixels(&face.left_eye, &face.bbox, img_w, img_h);
        let r_crop = eye_crop_pixels(&face.right_eye, &face.bbox, img_w, img_h);
        let l_img = image::imageops::crop_imm(&gray, l_crop.x, l_crop.y, l_crop.w, l_crop.h).to_image();
        let r_img = image::imageops::crop_imm(&gray, r_crop.x, r_crop.y, r_crop.w, r_crop.h).to_image();

        // Eye classification: optional. Eye-crop sharpness below is
        // computed unconditionally — it's pixel statistics, not model
        // output, so it's honest signal even without a classifier.
        let (l_open, r_open) = match eyes_provider {
            Some(eye) => {
                let l = eye.classify(&l_img)?;
                let r = eye.classify(&r_img)?;
                open_count += l + r;
                (Some(l), Some(r))
            }
            None => (None, None),
        };

        let l_sharp = normalize_sharpness(laplacian_variance(&l_img));
        let r_sharp = normalize_sharpness(laplacian_variance(&r_img));

        // Smile / expression classification: hand the full face crop to
        // the mouth provider. FER+/FER were trained on face crops, so a
        // narrow mouth-only patch produces near-zero smile probability
        // regardless of expression. For a mouth-specific model we'd
        // re-introduce `mouth_crop_pixels` here, but today all real-world
        // providers are holistic. `smile_score` is persisted per face;
        // `mouth_open` stays transient because nothing downstream reads it.
        let smile_score = mouth_provider.and_then(|m| {
            let f_crop = face_crop_pixels(&face.bbox, img_w, img_h);
            let f_img =
                image::imageops::crop_imm(&gray, f_crop.x, f_crop.y, f_crop.w, f_crop.h)
                    .to_image();
            m.classify(&f_img).ok().map(|s| s.smile_confidence)
        });

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

    // Cat pass. Produces additional `FaceRow`s with `species = "cat"`.
    // Cats have no eye / smile classifier today, so those columns are
    // always NULL on cat rows — distinct from "classifier ran and said
    // closed". Skipped entirely when the cat detector is absent.
    if let Some(cat) = cat_provider {
        if let Ok(cats) = cat.detect(&rgb) {
            for c in cats {
                // Tiny-YOLOv3 gives us the whole cat body. Shrink to the
                // approximate face region so the AiPanel tile crops to the
                // head rather than the full animal; see doc on
                // `approximate_cat_face` for the heuristic rationale.
                let face = approximate_cat_face(&c.bbox);
                face_rows.push(FaceRow {
                    photo_id: job.photo_id,
                    bbox_x: face.x, bbox_y: face.y,
                    bbox_w: face.w, bbox_h: face.h,
                    // No cat eye detector today — land the landmarks on
                    // the upper-third of the face crop so a future
                    // per-cat-eye classifier receives a reasonable
                    // starting box even before its real implementation.
                    left_eye_x: face.x + face.w * 0.30,
                    left_eye_y: face.y + face.h * 0.35,
                    right_eye_x: face.x + face.w * 0.70,
                    right_eye_y: face.y + face.h * 0.35,
                    left_eye_open: None, right_eye_open: None,
                    left_eye_sharpness: 0.0, right_eye_sharpness: 0.0,
                    detection_confidence: c.confidence,
                    smile_score: None,
                    species: "cat".to_string(),
                });
            }
        }
    }

    // Whole-image sharpness on the already-decoded gray image.
    let raw = laplacian_variance(&gray);
    let whole = normalize_sharpness(raw);
    let t_eye_sharp = t_eye_start.elapsed();
    let t_total = t_start.elapsed();

    // Composite quality score (0-100). Blends sharpness with subject
    // presence, eye-open rate, and smile confidence. Calibrated so:
    //   - bare sharp-100 landscape (no subject)       → 70
    //   - sharp-100 portrait, eyes open, big smile    → 100
    //   - sharp-100 portrait, eyes closed, flat face  → 80
    //   - sharp-100 cat photo (no per-eye/smile data) → 80
    //   - blurry (sharp=30) portrait w/ smile+eyes    → ~51
    // Keeps sharpness as the dominant term (70 %) so AI pick inside a
    // group still privileges focus over expression when they conflict,
    // while letting subject engagement break ties in landscapes/portraits
    // that are equally sharp.
    let human_count = face_rows.iter().filter(|f| f.species == "human").count();
    let cat_count = face_rows.iter().filter(|f| f.species == "cat").count();

    // True only when the eye classifier actually ran — protects the
    // quality score and the photos.eyes_open_count aggregate from
    // misinterpreting "no classifier" as "all eyes closed". Without
    // this guard, photos analyzed before installing the ONNX
    // classifier would silently score lower than identical photos
    // analyzed after.
    let eye_classifier_ran = eyes_provider.is_some();

    let mut q = whole * 0.70;
    if human_count > 0 {
        q += 10.0; // subject-presence bonus
        // Eye-open ratio: contributes only when the classifier actually
        // produced data. Otherwise, skip the term entirely so missing-
        // classifier photos and eyes-closed photos rank differently.
        if eye_classifier_ran {
            let max_eyes = (human_count as i32) * 2;
            if max_eyes > 0 {
                let eye_ratio = (open_count as f64 / max_eyes as f64).clamp(0.0, 1.0);
                q += eye_ratio * 10.0;
            }
        }
        // Smile: max across human faces (null → 0, so neutral faces
        // and absent-classifier faces both contribute nothing rather
        // than penalizing).
        let max_smile = face_rows
            .iter()
            .filter(|f| f.species == "human")
            .filter_map(|f| f.smile_score)
            .fold(0.0_f64, f64::max);
        q += max_smile * 10.0;
    } else if cat_count > 0 {
        // Cats don't (yet) have eye/smile classifiers, so they contribute
        // the same flat subject bonus a human face does but no engagement
        // term. Rationale: a sharp cat portrait should still rank above a
        // sharp empty-room shot.
        q += 10.0;
    }
    let quality = q.clamp(0.0, 100.0);

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
    // `eyes_open_count` is None when no classifier ran, so the UI can
    // distinguish "all eyes closed" from "didn't measure".
    let eyes_open_count_for_db = if eye_classifier_ran {
        Some(open_count)
    } else {
        None
    };
    db.write_ai_result(
        job.photo_id,
        &face_rows,
        Some(face_rows.len() as i32),
        eyes_open_count_for_db,
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
    faces_provider: Option<Box<dyn FaceProvider>>,
    eyes_provider: Option<Box<dyn EyeStateProvider>>,
    mouth_provider: Option<Box<dyn MouthStateProvider>>,
    cat_provider: Option<Box<dyn CatDetectorProvider>>,
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
        // Re-borrow each Option<Box<_>> into Option<&dyn _> per job so
        // ownership stays in run_loop across iterations.
        let result = process_job(
            &mut db,
            &job,
            faces_provider.as_deref(),
            eyes_provider.as_deref(),
            mouth_provider.as_deref(),
            cat_provider.as_deref(),
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

    // The Mock* provider types are kept around purely as drop-in test
    // doubles — never registered at app runtime. Wiring them through
    // `Some(&...)` here exercises the new Option-shaped provider
    // signatures rather than the production "no classifier" path.

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
            Some(&MockFaceProvider::default()),
            Some(&MockEyeProvider::default()),
            Some(&MockMouthProvider::default()),
            Some(&MockCatDetector::default()),
        )
        .unwrap();

        let faces = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(faces.len(), 1);
        // MockEyeProvider returns 0,1 alternating — first face gets left=0, right=1.
        // Wrapped in Some(..) since the column is now nullable.
        assert_eq!(faces[0].left_eye_open, Some(0));
        assert_eq!(faces[0].right_eye_open, Some(1));

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
    fn test_process_job_with_no_eye_or_mouth_classifier_writes_nulls() {
        // Production-shaped path: face detection runs (Some(face)),
        // eye / mouth / cat are absent (None). Verify we get the face
        // row with NULL eye_open / smile_score, and `eyes_open_count`
        // on the photo aggregate is also NULL (not 0, which would
        // mean "all eyes closed").
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
            shoot_id, photo_id: ids[0],
            preview_path: preview.to_string_lossy().into_owned(),
        };

        process_job(
            &mut db,
            &job,
            Some(&MockFaceProvider::default()),
            None, // no eye classifier
            None, // no mouth classifier
            None, // no cat detector
        ).unwrap();

        let faces = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].left_eye_open, None);
        assert_eq!(faces[0].right_eye_open, None);
        assert_eq!(faces[0].smile_score, None);
        // Per-eye sharpness IS computed even without a classifier —
        // it's pixel statistics, not model output.
        assert!(faces[0].left_eye_sharpness >= 0.0);

        let row: (Option<i32>, Option<i32>) = db.conn.query_row(
            "SELECT face_count, eyes_open_count FROM photos WHERE id = ?1",
            rusqlite::params![ids[0]],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(row.0, Some(1)); // face detected
        assert_eq!(row.1, None);    // but no eye-open count without a classifier
    }

    #[test]
    fn test_process_job_with_no_face_provider_still_writes_sharpness() {
        // When even face detection is unavailable, the worker should
        // still record whole-image sharpness so the photo has *some*
        // AI signal rather than the entire ai_analyzed_at row staying
        // null.
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
            shoot_id, photo_id: ids[0],
            preview_path: preview.to_string_lossy().into_owned(),
        };

        process_job(&mut db, &job, None, None, None, None).unwrap();

        let faces = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(faces.len(), 0);
        let row: (Option<i32>, Option<i32>, Option<f64>) = db.conn.query_row(
            "SELECT face_count, eyes_open_count, sharpness_score FROM photos WHERE id = ?1",
            rusqlite::params![ids[0]],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(row.0, Some(0));
        assert_eq!(row.1, None);
        assert!(row.2.is_some(), "sharpness_score still written without classifiers");
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
                Some(Box::new(crate::ai::mock::MockFaceProvider::default())),
                Some(Box::new(crate::ai::mock::MockEyeProvider::default())),
                Some(Box::new(crate::ai::mouth::MockMouthProvider::default())),
                Some(Box::new(crate::ai::cat::MockCatDetector::default())),
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
