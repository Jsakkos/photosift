//! Prompt templates for Stage 1 (shoot summary) and Stage 2 (per-cluster
//! judgment). Plain-string templates assembled by `api.rs` into the full
//! request body. The `PROMPT_VERSION` constant is persisted on every
//! judgment row so the UI can surface a "rerun (prompts updated)" CTA
//! once these strings change meaningfully.

use crate::curator::types::ShootSummary;

/// Bumped any time a meaningful change is made to either the system
/// prompt below or the rubric. New judgments are tagged with the new
/// version; existing rows keep their original version. The Library
/// shoot card surfaces a rerun affordance when any photo's
/// `prompt_version < CURRENT_PROMPT_VERSION`.
pub const CURRENT_PROMPT_VERSION: i32 = 1;

/// Version stamped on every `triage_judgments` row. Bumped independently
/// of `CURRENT_PROMPT_VERSION` when the triage steering text below changes.
/// v2: replaced the editorial Stage-2 rubric with a technical-only triage
/// prompt — the v1 path rejected ~40% of shoots on expression/composition.
pub const TRIAGE_PROMPT_VERSION: i32 = 2;

/// Sentinel `shoot_type` that marks a `ShootSummary` as the triage-stage
/// placeholder. `stage2_system` keys off this to emit the conservative
/// technical-triage prompt instead of the editorial selection rubric —
/// the triage stage shares `run_stage2_cluster` with the selection stage,
/// and the summary is the only value that flows through to distinguish
/// them. A real Stage-1 `shoot_type` never collides with this string.
pub const TRIAGE_SHOOT_TYPE: &str = "__photosift_triage__";

/// Placeholder shoot summary used by the Curator *triage stage*. The
/// triage stage reuses the Stage 2 request path (`run_stage2_cluster`);
/// `stage2_system` detects `TRIAGE_SHOOT_TYPE` and swaps in the triage
/// prompt, so triage needs no extra LLM call and no per-provider code.
pub fn triage_summary() -> ShootSummary {
    ShootSummary {
        shoot_type: TRIAGE_SHOOT_TYPE.to_string(),
        subjects: vec![],
        story: String::new(),
        dominant_style: String::new(),
        watch_for: vec![],
    }
}

/// Rubric inlined into the Stage 2 system prompt. Kept short on purpose:
/// a long rubric crowds out vision tokens at fixed `max_tokens`.
const STAGE2_RUBRIC: &str = "\
Composition (0-10): subject framing, leading lines, balance, distractions in frame.
Aesthetic (0-10): light quality, color, mood, overall keeper-worthiness independent of composition.
cluster_rank: integer 1..=N where 1 is the strongest frame in this cluster. Resolve ties by composition + aesthetic sum.
is_keeper: true ONLY for genuinely strong frames the photographer should likely keep.
suggested_flag: 'pick' (clear keeper), 'reject' (clear cull), 'keep' (worth a second look but not your top choice).
Use the included technical signals (sharpness, faces, eyes_open, smile) as constraints. \
Frames with eyes-closed humans or sharpness < 4/10 are nearly always 'reject' unless the moment is extraordinary.
reason: ONE short sentence (≤ 22 words). Reference the specific image.
";

const STAGE2_ROLE: &str = "\
You are a senior photo editor culling a personal photography shoot. \
You evaluate frames within a near-duplicate cluster and emit a structured \
judgment per frame. You are ranking frames *within their cluster* and also \
giving a standalone keeper assessment. The photographer trusts you to be \
decisive but never punitive: when in doubt, prefer 'keep' over 'reject'. \
Always call the `record_judgments` tool. Never respond with prose.";

const STAGE1_ROLE: &str = "\
You are a senior photo editor previewing a personal photography shoot before \
detailed culling begins. You will see a stratified sample of thumbnails. \
Characterize the shoot so a downstream cull pass can use this context. \
Always call the `record_shoot_summary` tool. Never respond with prose.";

/// Build the Stage 1 system prompt. A small static block plus a tool-use
/// reminder. No `cache_control` here — Stage 1 runs once per shoot, no
/// reuse to amortize.
pub fn stage1_system() -> String {
    format!(
        "{role}\n\n\
        Output schema (call `record_shoot_summary` with):\n\
        - shoot_type: short kebab/snake string, e.g. 'casual_outdoor_portrait', 'wildlife_birds', 'urban_street'\n\
        - subjects: array of short strings naming the actual subjects (people, animals, objects)\n\
        - story: 1-2 sentences. What is happening across the set?\n\
        - dominant_style: 1 phrase capturing light/mood/style\n\
        - watch_for: array of 2-5 specific things a culler should reject when they see them in this shoot\n",
        role = STAGE1_ROLE
    )
}

/// Triage-stage system prompt. The triage stage shares `run_stage2_cluster`
/// with the selection stage, so it arrives here too — but it must NOT use
/// the editorial rubric above. Triage rejects ONLY frames with a hard,
/// visible technical defect; everything subjective (expression, closed
/// eyes, composition, "best of a group") is the photographer's call in
/// the later Triage/Select passes, not this one.
const TRIAGE_SYSTEM: &str = "\
You are a first-pass technical triage assistant for a photo cull. You see a \
batch of frames from one shoot. Your ONLY job is to flag frames that are \
TECHNICALLY UNUSABLE so the photographer never has to look at them.

Set suggested_flag='reject' ONLY when the frame has a hard technical defect \
you can clearly SEE in the image:
- severe motion blur or camera shake across the whole subject
- the intended subject grossly out of focus
- exposure so blown out or so black that detail is unrecoverable
- an obvious misfire — lens cap, the floor, the ceiling, a wild blurred pan

Set suggested_flag='keep' for EVERYTHING ELSE. Do NOT reject for any of these:
- closed or blinking eyes, looking away, or any facial expression
- ordinary, repetitive, or imperfect composition; an unconventional angle
- a busy or distracting background
- a face turned away, partly hidden, or small in the frame
- mild softness — many perfectly good frames are slightly soft
- simply being weaker than a similar frame — that is the later Select pass

When in doubt, KEEP. Wrongly rejecting a usable frame is far worse than \
keeping a weak one. A shoot of competent photos should have almost no rejects.

A technical-signals line may include `sharpness=N/10`. That number is a \
RELATIVE rank within this shoot, not an absolute quality score — a low value \
does NOT by itself justify a reject. Judge blur from the image itself.

Always call the `record_judgments` tool, one entry per frame. Only \
`suggested_flag` ('reject' or 'keep') and `reason` are used. `reason`: ONE \
short sentence — name the specific visible defect for a reject, or say \
'no technical defect' for a keep.";

/// Build the Stage 2 system prompt. Inlines the Stage 1 summary so each
/// per-cluster call shares the shoot context. The full system block is
/// returned; the caller wraps it with `cache_control: ephemeral` so
/// subsequent Stage 2 calls in the same shoot hit the prompt cache.
///
/// When `summary.shoot_type` is the `TRIAGE_SHOOT_TYPE` sentinel this is a
/// triage-stage call — return the conservative technical-only prompt
/// instead of the editorial selection rubric.
pub fn stage2_system(summary: &ShootSummary) -> String {
    if summary.shoot_type == TRIAGE_SHOOT_TYPE {
        return TRIAGE_SYSTEM.to_string();
    }
    format!(
        "{role}\n\n\
        SHOOT CONTEXT (apply to every cluster you evaluate):\n\
        - Type: {shoot_type}\n\
        - Subjects: {subjects}\n\
        - Story: {story}\n\
        - Style: {style}\n\
        - Watch for: {watch_for}\n\n\
        RUBRIC:\n{rubric}\n\
        Always call the `record_judgments` tool. Return one entry per photo \
        in the cluster. The 'judgments' array length MUST equal the number \
        of photos in the user message.",
        role = STAGE2_ROLE,
        shoot_type = summary.shoot_type,
        subjects = summary.subjects.join(", "),
        story = summary.story,
        style = summary.dominant_style,
        watch_for = summary.watch_for.join("; "),
        rubric = STAGE2_RUBRIC,
    )
}

/// One frame's technical-context line. Concise (one line per frame) so
/// the LLM doesn't drown in numerics.
pub fn tech_line(
    photo_id: i64,
    sharpness_1_10: Option<i32>,
    face_count: Option<i32>,
    eyes_open_count: Option<i32>,
    smile_score: Option<f64>,
) -> String {
    let mut parts = vec![format!("[{}]", photo_id)];
    if let Some(s) = sharpness_1_10 {
        parts.push(format!("sharpness={}/10", s));
    }
    if let Some(f) = face_count {
        if f > 0 {
            let eye = eyes_open_count.unwrap_or(0);
            parts.push(format!("faces={} eyes_open={}/{}", f, eye, f * 2));
        }
    }
    if let Some(sm) = smile_score {
        parts.push(format!("smile={:.2}", sm));
    }
    parts.join(" ")
}

/// Render the user-message text portion for a Stage 2 cluster call.
/// Image blocks are appended separately by the caller.
pub fn stage2_user_text(cluster_size: usize, tech_lines: &[String]) -> String {
    format!(
        "Cluster of {n} frames. Technical signals (from local AI):\n{tech}\n\n\
        Evaluate every frame. Call `record_judgments` with one entry per frame, ordered by photo_id ascending.",
        n = cluster_size,
        tech = tech_lines.join("\n"),
    )
}

/// User-message text for Stage 1. Image blocks appended separately.
pub fn stage1_user_text(photo_count: i64, cluster_count: i64) -> String {
    format!(
        "Shoot has {photo_count} photos in {cluster_count} clusters (plus singletons). \
        The thumbnails below are a stratified sample. Call `record_shoot_summary`.",
        photo_count = photo_count,
        cluster_count = cluster_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_line_drops_face_block_when_no_faces() {
        let s = tech_line(1, Some(8), Some(0), Some(0), None);
        assert!(s.contains("[1]"));
        assert!(s.contains("sharpness=8/10"));
        assert!(!s.contains("faces="));
    }

    #[test]
    fn tech_line_includes_eyes_when_faces_present() {
        let s = tech_line(2, Some(7), Some(2), Some(3), Some(0.71));
        assert!(s.contains("faces=2"));
        assert!(s.contains("eyes_open=3/4"));
        assert!(s.contains("smile=0.71"));
    }

    #[test]
    fn stage2_system_inlines_summary() {
        let sum = ShootSummary {
            shoot_type: "park_walk".to_string(),
            subjects: vec!["dog".to_string()],
            story: "afternoon walk".to_string(),
            dominant_style: "natural light".to_string(),
            watch_for: vec!["leash visible".to_string()],
        };
        let p = stage2_system(&sum);
        assert!(p.contains("park_walk"));
        assert!(p.contains("leash visible"));
        assert!(p.contains("RUBRIC"));
    }

    #[test]
    fn stage1_user_text_has_counts() {
        let s = stage1_user_text(1247, 152);
        assert!(s.contains("1247"));
        assert!(s.contains("152"));
    }

    #[test]
    fn triage_summary_routes_to_conservative_prompt() {
        let p = stage2_system(&triage_summary());
        // It's the triage prompt, not the editorial selection rubric.
        assert!(p.contains("technical triage"));
        assert!(!p.contains("Composition (0-10)"));
        assert!(!p.contains("RUBRIC"));
        // Triage must not reject on closed eyes / expression / composition.
        assert!(p.contains("Do NOT reject"));
        assert!(p.to_lowercase().contains("closed or blinking eyes"));
    }

    #[test]
    fn selection_summary_still_uses_editorial_rubric() {
        let sum = ShootSummary {
            shoot_type: "park_walk".to_string(),
            subjects: vec![],
            story: "x".to_string(),
            dominant_style: "y".to_string(),
            watch_for: vec![],
        };
        let p = stage2_system(&sum);
        assert!(p.contains("Composition (0-10)"));
        assert!(p.contains("RUBRIC"));
    }
}
