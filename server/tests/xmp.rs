mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::NaiveDate;
use photosift::db::{self, photos};
use photosift::outcome::{xmp_fields, Outcome};
use photosift::xmp;
use serde_json::{json, Value};

const FIXTURE_RAW: &str = include_str!("fixtures/lightroom_sidecar.xmp");

/// The fixture with LF endings, whatever the checkout's autocrlf did to it.
fn fixture() -> String {
    FIXTURE_RAW.replace("\r\n", "\n")
}

const ALL: [Outcome; 5] = [
    Outcome::Unreviewed,
    Outcome::Album,
    Outcome::Library,
    Outcome::Archive,
    Outcome::Delete,
];

/// A dummy RAW in a fresh dir; the sidecar sits beside it.
fn raw_in(dir: &Path) -> PathBuf {
    let raw = dir.join("IMG_0001.NEF");
    std::fs::write(&raw, b"not really a raw").unwrap();
    raw
}

fn sidecar(raw: &Path) -> PathBuf {
    raw.with_extension("xmp")
}

/// Raw + the synthetic Lightroom sidecar.
fn with_fixture(dir: &Path) -> PathBuf {
    let raw = raw_in(dir);
    std::fs::write(sidecar(&raw), fixture()).unwrap();
    raw
}

fn read(raw: &Path) -> String {
    std::fs::read_to_string(sidecar(raw)).unwrap()
}

fn set(raw: &Path, o: Outcome, blooper: bool) {
    xmp::apply(raw, &xmp_fields(o, blooper)).unwrap();
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn writes_rating_for_each_outcome() {
    for (o, stars) in [
        (Outcome::Album, 4),
        (Outcome::Library, 3),
        (Outcome::Archive, 2),
        (Outcome::Delete, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let raw = raw_in(dir.path());
        set(&raw, o, false);
        let xml = read(&raw);
        assert!(
            xml.contains(&format!("xmp:Rating=\"{stars}\"")),
            "{o:?}: {xml}"
        );
        assert_eq!(count(&xml, "xmp:Rating="), 1);
    }
}

#[test]
fn unreviewed_creates_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Unreviewed, false);
    assert!(!sidecar(&raw).exists());
}

#[test]
fn unreviewed_leaves_existing_sidecar_alone() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Unreviewed, false);
    assert_eq!(read(&raw), fixture());
}

#[test]
fn new_sidecar_is_well_formed_and_current_rating_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Album, false);
    set(&raw, Outcome::Archive, false);
    let xml = read(&raw);
    assert!(xml.contains("xmp:Rating=\"2\""));
    assert!(!xml.contains("xmp:Rating=\"4\""));
    assert_eq!(count(&xml, "xmlns:xmp="), 1);
    assert_eq!(count(&xml, "<rdf:Description"), 1);
    assert!(xml.contains("<x:xmpmeta"));
}

#[test]
fn delete_sets_red_and_reverting_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Delete, false);
    let xml = read(&raw);
    assert!(xml.contains("xmp:Label=\"Red\""), "{xml}");
    assert!(xml.contains("xmp:Rating=\"1\""));

    set(&raw, Outcome::Archive, false);
    let xml = read(&raw);
    assert!(!xml.contains("xmp:Label"), "{xml}");
    assert!(!xml.contains("LabelSet"), "{xml}");
    assert!(xml.contains("xmp:Rating=\"2\""));
}

#[test]
fn unreviewing_a_delete_releases_red() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Delete, false);
    set(&raw, Outcome::Unreviewed, false);
    assert!(!read(&raw).contains("xmp:Label"));
}

#[test]
fn red_the_user_set_themselves_is_never_removed() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    std::fs::write(
        sidecar(&raw),
        fixture().replace("xmp:Label=\"Blue\"", "xmp:Label=\"Red\""),
    )
    .unwrap();
    set(&raw, Outcome::Delete, false);
    set(&raw, Outcome::Library, false);
    let xml = read(&raw);
    assert!(xml.contains("xmp:Label=\"Red\""), "{xml}");
    assert!(!xml.contains("LabelSet"));
}

#[test]
fn user_label_untouched() {
    for o in [Outcome::Album, Outcome::Library] {
        let dir = tempfile::tempdir().unwrap();
        let raw = with_fixture(dir.path());
        set(&raw, o, false);
        let xml = read(&raw);
        assert!(xml.contains("xmp:Label=\"Blue\""), "{o:?}: {xml}");
    }
}

#[test]
fn delete_does_not_clobber_a_users_other_label() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Delete, false);
    let xml = read(&raw);
    assert!(xml.contains("xmp:Label=\"Blue\""), "{xml}");
    assert!(!xml.contains("Red"));
    assert!(!xml.contains("LabelSet"));
    set(&raw, Outcome::Album, false);
    assert!(read(&raw).contains("xmp:Label=\"Blue\""));
}

#[test]
fn five_star_is_never_lowered() {
    for o in ALL {
        let dir = tempfile::tempdir().unwrap();
        let raw = with_fixture(dir.path());
        set(&raw, o, false);
        assert!(read(&raw).contains("xmp:Rating=\"5\""), "{o:?}");
    }
}

#[test]
fn lightroom_fields_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Album, true);
    set(&raw, Outcome::Album, true);
    let xml = read(&raw);

    // A new attribute may follow the last one, so compare without the tag's `>`.
    for line in fixture().lines().filter(|l| l.contains("crs:")) {
        let line = line.trim_end_matches('>');
        assert!(xml.contains(line), "lost line {line:?} in\n{xml}");
    }
    assert!(xml.contains("<rdf:li>sample keyword</rdf:li>"));
    assert!(xml.contains("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>"));
    assert!(xml.contains("<?xpacket end=\"w\"?>"));
    assert_eq!(count(&xml, "<rdf:li>PhotoSift|Blooper</rdf:li>"), 1);
    assert_eq!(count(&xml, "<rdf:li>Blooper</rdf:li>"), 1);
    assert_eq!(count(&xml, "<lr:hierarchicalSubject>"), 1);
    assert_eq!(count(&xml, "<dc:subject>"), 1);
    assert_eq!(count(&xml, "xmlns:lr="), 1);
}

#[test]
fn plain_album_on_a_five_star_sidecar_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Album, false);
    assert_eq!(read(&raw), fixture());
}

#[test]
fn blooper_off_removes_only_our_keyword() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Album, true);
    set(&raw, Outcome::Album, false);
    let xml = read(&raw);
    assert!(!xml.contains("Blooper"), "{xml}");
    assert!(xml.contains("<rdf:li>sample keyword</rdf:li>"));
}

#[test]
fn removing_the_last_keyword_drops_the_empty_bag() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Library, true);
    set(&raw, Outcome::Library, false);
    let xml = read(&raw);
    assert!(!xml.contains("hierarchicalSubject"), "{xml}");
    assert!(!xml.contains("dc:subject"), "{xml}");
    assert!(xml.contains("xmp:Rating=\"3\""));
}

#[test]
fn appends_to_existing_hierarchical_bag_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    let doc = fixture()
        .replace(
            "xmlns:dc=",
            "xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\"\n    xmlns:dc=",
        )
        .replace(
            "   <crs:ToneCurvePV2012>",
            "   <lr:hierarchicalSubject>\n    <rdf:Bag>\n     <rdf:li>Trips|Beach</rdf:li>\n     <rdf:li>PhotoSift|Blooper</rdf:li>\n    </rdf:Bag>\n   </lr:hierarchicalSubject>\n   <crs:ToneCurvePV2012>",
        );
    std::fs::write(sidecar(&raw), &doc).unwrap();
    set(&raw, Outcome::Album, true);
    let xml = read(&raw);
    assert_eq!(count(&xml, "PhotoSift|Blooper"), 1, "{xml}");
    assert!(xml.contains("<rdf:li>Trips|Beach</rdf:li>"));
    assert_eq!(count(&xml, "<lr:hierarchicalSubject>"), 1);
    assert_eq!(count(&xml, "xmlns:lr="), 1);
    // The leaf is mirrored in dc:subject, next to the user's keyword.
    assert_eq!(count(&xml, "<rdf:li>Blooper</rdf:li>"), 1);
    assert!(xml.contains("<rdf:li>sample keyword</rdf:li>"));
}

#[test]
fn self_closing_description_gets_children() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    let doc = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:WhiteBalance=\"As Shot\"/></rdf:RDF></x:xmpmeta>";
    std::fs::write(sidecar(&raw), doc).unwrap();
    set(&raw, Outcome::Library, true);
    let xml = read(&raw);
    assert!(xml.contains("crs:WhiteBalance=\"As Shot\""));
    assert!(xml.contains("xmp:Rating=\"3\""));
    assert!(xml.contains("<rdf:li>PhotoSift|Blooper</rdf:li>"));
    assert_eq!(count(&xml, "<rdf:Description"), 1);
    assert_eq!(count(&xml, "</rdf:Description>"), 1);
    set(&raw, Outcome::Library, true);
    assert_eq!(read(&raw), xml);
}

#[test]
fn rating_in_element_form_is_replaced_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    let doc = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"><xmp:Rating>2</xmp:Rating></rdf:Description></rdf:RDF></x:xmpmeta>";
    std::fs::write(sidecar(&raw), doc).unwrap();
    set(&raw, Outcome::Album, false);
    let xml = read(&raw);
    assert!(xml.contains("<xmp:Rating>4</xmp:Rating>"), "{xml}");
    assert_eq!(count(&xml, "Rating"), 2);
}

#[test]
fn no_write_when_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    set(&raw, Outcome::Delete, true);
    let mtime = |p: &Path| std::fs::metadata(p).unwrap().modified().unwrap();
    let first = mtime(&sidecar(&raw));
    let bytes = read(&raw);
    std::thread::sleep(Duration::from_millis(50));
    set(&raw, Outcome::Delete, true);
    assert_eq!(mtime(&sidecar(&raw)), first);
    assert_eq!(read(&raw), bytes);
}

#[test]
fn malformed_sidecar_is_an_error_and_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    for junk in ["<x:xmpmeta><rdf:RDF>", "not xml at all", ""] {
        std::fs::write(sidecar(&raw), junk).unwrap();
        assert!(xmp::apply(&raw, &xmp_fields(Outcome::Album, false)).is_err());
        assert_eq!(read(&raw), junk);
    }
}

#[test]
fn preserves_bom_and_crlf() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    let doc = format!(
        "\u{feff}{}",
        fixture()
            .replace("xmp:Rating=\"5\"", "xmp:Rating=\"1\"")
            .replace('\n', "\r\n")
    );
    std::fs::write(sidecar(&raw), &doc).unwrap();
    set(&raw, Outcome::Album, true);
    let bytes = std::fs::read(sidecar(&raw)).unwrap();
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
    let xml = String::from_utf8(bytes).unwrap();
    assert!(xml.contains("xmp:Rating=\"4\""));
    assert!(!xml.replace("\r\n", "").contains('\n'), "bare LF inserted");
}

// ---- endpoint -------------------------------------------------------------

struct Seeded {
    server: common::TestServer,
    files: tempfile::TempDir,
    id: i64,
}

fn open_db(server: &common::TestServer) -> db::DbPool {
    db::open(&server.data_root.path().join("photosift.db")).unwrap()
}

async fn seeded(configure: impl FnOnce(&Path) -> photos::NewPhoto) -> Seeded {
    let server = common::spawn_server().await;
    let files = tempfile::tempdir().unwrap();
    let pool = open_db(&server);
    let day = NaiveDate::from_ymd_opt(2026, 3, 14).unwrap();
    let shoot_id = photos::insert_shoot(
        &pool,
        &photos::NewShoot {
            kind: photos::ShootKind::Named,
            name: Some("Park Day".into()),
            folder: files.path().to_string_lossy().into_owned(),
            date_start: Some(day),
            date_end: Some(day),
        },
    )
    .unwrap();
    let mut photo = configure(files.path());
    photo.shoot_id = shoot_id;
    photo.sha256 = "ab".repeat(32);
    let id = photos::insert_photo(&pool, &photo).unwrap();
    Seeded { server, files, id }
}

async fn post_outcome(s: &Seeded, id: i64, body: Value) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/photos/{id}/outcome", s.server.base_url))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(Value::Null))
}

fn raw_photo(dir: &Path) -> photos::NewPhoto {
    let raw = raw_in(dir);
    photos::NewPhoto {
        raw_path: Some(raw.to_string_lossy().into_owned()),
        ..Default::default()
    }
}

fn stored_outcome(s: &Seeded) -> Outcome {
    photos::get_photo(&open_db(&s.server), s.id)
        .unwrap()
        .unwrap()
        .outcome
}

#[tokio::test]
async fn endpoint_persists_then_writes_xmp() {
    let s = seeded(raw_photo).await;
    let (status, body) = post_outcome(&s, s.id, json!({"outcome": "album", "blooper": true})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["outcome"], "album");
    assert_eq!(body["blooper"], true);
    assert_eq!(body["id"], s.id);
    assert!(body["xmp_error"].is_null());
    let xml = read(&s.files.path().join("IMG_0001.NEF"));
    assert!(xml.contains("xmp:Rating=\"4\""));
    assert!(xml.contains("PhotoSift|Blooper"));

    // Omitted blooper leaves the tag as is.
    let (_, body) = post_outcome(&s, s.id, json!({"outcome": "library"})).await;
    assert_eq!(body["blooper"], true);
    assert_eq!(stored_outcome(&s), Outcome::Library);
}

#[tokio::test]
async fn endpoint_uses_jpeg_when_there_is_no_raw() {
    let s = seeded(|dir| {
        let jpg = dir.join("IMG_0002.JPG");
        std::fs::write(&jpg, b"jpeg").unwrap();
        photos::NewPhoto {
            jpeg_path: Some(jpg.to_string_lossy().into_owned()),
            ..Default::default()
        }
    })
    .await;
    let (status, _) = post_outcome(&s, s.id, json!({"outcome": "archive"})).await;
    assert_eq!(status, 200);
    let xml = std::fs::read_to_string(s.files.path().join("IMG_0002.xmp")).unwrap();
    assert!(xml.contains("xmp:Rating=\"2\""));
}

#[tokio::test]
async fn endpoint_skips_xmp_for_video() {
    let s = seeded(|dir| photos::NewPhoto {
        video_path: Some(dir.join("MOV_0001.MP4").to_string_lossy().into_owned()),
        ..Default::default()
    })
    .await;
    let (status, body) = post_outcome(&s, s.id, json!({"outcome": "album"})).await;
    assert_eq!(status, 200);
    assert!(body["xmp_error"].is_null());
    assert_eq!(std::fs::read_dir(s.files.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn xmp_failure_is_reported_and_db_is_kept() {
    let s = seeded(|dir| photos::NewPhoto {
        raw_path: Some(
            dir.join("gone")
                .join("IMG_0003.NEF")
                .to_string_lossy()
                .into_owned(),
        ),
        ..Default::default()
    })
    .await;
    let (status, body) = post_outcome(&s, s.id, json!({"outcome": "delete"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["outcome"], "delete");
    assert!(body["xmp_error"].as_str().is_some_and(|e| !e.is_empty()));
    assert_eq!(stored_outcome(&s), Outcome::Delete);
}

#[tokio::test]
async fn endpoint_rejects_bad_requests() {
    let s = seeded(raw_photo).await;
    let (status, _) = post_outcome(&s, 9999, json!({"outcome": "album"})).await;
    assert_eq!(status, 404);
    let (status, _) = post_outcome(&s, s.id, json!({"outcome": "pick"})).await;
    assert!(status == 400 || status == 422, "{status}");
    assert!(!sidecar(&s.files.path().join("IMG_0001.NEF")).exists());
    assert_eq!(stored_outcome(&s), Outcome::Unreviewed);
}

// ---- integrity ------------------------------------------------------------

const LR_DECL: &str = "xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\"";
const PS_DECL: &str = "xmlns:photosift=\"http://photosift.app/ns/1.0/\"";

/// The fixture after Album + blooper: its bytes, plus exactly these insertions.
fn fixture_with_blooper() -> String {
    fixture()
        .replace(
            "    crs:HasSettings=\"True\">",
            &format!(
                "    crs:HasSettings=\"True\"\n    {LR_DECL}\n    {PS_DECL}\n    photosift:BlooperSet=\"1\">"
            ),
        )
        .replace(
            "     <rdf:li>Fish &amp; Chips caf\u{e9}</rdf:li>\n",
            "     <rdf:li>Fish &amp; Chips caf\u{e9}</rdf:li>\n     <rdf:li>Blooper</rdf:li>\n",
        )
        .replace(
            "  </rdf:Description>",
            "   <lr:hierarchicalSubject>\n    <rdf:Bag>\n     <rdf:li>PhotoSift|Blooper</rdf:li>\n    </rdf:Bag>\n   </lr:hierarchicalSubject>\n  </rdf:Description>",
        )
}

#[test]
fn album_with_blooper_changes_only_the_expected_spans() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Album, true);
    assert_eq!(read(&raw), fixture_with_blooper());
    // Escaped text, non-ASCII, the padded xpacket wrapper and the tone curve
    // list all come through byte for byte.
    let xml = read(&raw);
    assert!(xml.contains("Fish &amp; Chips caf\u{e9}"));
    assert!(xml.contains("<rdf:li>255, 255</rdf:li>"));
    assert!(xml.contains(&format!("{}\n<?xpacket end=\"w\"?>", " ".repeat(99))));
}

#[test]
fn blooper_on_then_off_leaves_only_namespace_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let raw = with_fixture(dir.path());
    set(&raw, Outcome::Album, true);
    set(&raw, Outcome::Album, false);
    let expected = fixture().replace(
        "    crs:HasSettings=\"True\">",
        &format!("    crs:HasSettings=\"True\"\n    {LR_DECL}\n    {PS_DECL}>"),
    );
    assert_eq!(read(&raw), expected);
}

const SPLIT_HEAD: &str = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n<rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:WhiteBalance=\"As Shot\"/>\n";
const SPLIT_TAIL: &str = "</rdf:RDF>\n</x:xmpmeta>\n";

#[test]
fn fields_in_a_second_description_are_refused() {
    let seconds = [
        "<rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmp:Rating=\"3\" xmp:Label=\"Blue\"/>\n",
        "<rdf:Description rdf:about=\"\" xmlns:x2=\"http://ns.adobe.com/xap/1.0/\" x2:Label=\"Blue\"/>\n",
        "<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:subject><rdf:Bag><rdf:li>a</rdf:li></rdf:Bag></dc:subject></rdf:Description>\n",
        "<rdf:Description rdf:about=\"\" xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\"><lr:hierarchicalSubject><rdf:Bag/></lr:hierarchicalSubject></rdf:Description>\n",
    ];
    for second in seconds {
        let dir = tempfile::tempdir().unwrap();
        let raw = raw_in(dir.path());
        let doc = format!("{SPLIT_HEAD}{second}{SPLIT_TAIL}");
        std::fs::write(sidecar(&raw), &doc).unwrap();
        for (o, blooper) in [(Outcome::Album, false), (Outcome::Delete, true)] {
            let err = xmp::apply(&raw, &xmp_fields(o, blooper)).unwrap_err();
            assert!(err.to_string().contains("second rdf:Description"), "{err}");
            assert_eq!(read(&raw), doc);
        }
    }
}

#[test]
fn unrelated_second_description_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    let second = "<rdf:Description rdf:about=\"\" xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\" tiff:Make=\"Synthetic\"/>\n";
    std::fs::write(sidecar(&raw), format!("{SPLIT_HEAD}{second}{SPLIT_TAIL}")).unwrap();
    set(&raw, Outcome::Album, false);
    let xml = read(&raw);
    assert!(xml.contains("xmp:Rating=\"4\""));
    assert!(xml.contains(second));
}

// ---- blooper ownership ----------------------------------------------------

fn doc_with(dc_items: &[&str], hier_items: &[&str]) -> String {
    let bag = |q: &str, items: &[&str]| {
        if items.is_empty() {
            return String::new();
        }
        let lis: String = items
            .iter()
            .map(|i| format!("<rdf:li>{i}</rdf:li>"))
            .collect();
        format!("<{q}><rdf:Bag>{lis}</rdf:Bag></{q}>")
    };
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\">{}{}</rdf:Description></rdf:RDF></x:xmpmeta>",
        bag("dc:subject", dc_items),
        bag("lr:hierarchicalSubject", hier_items)
    )
}

#[test]
fn users_own_blooper_leaf_survives_blooper_on_then_off() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    std::fs::write(sidecar(&raw), doc_with(&["Blooper"], &[])).unwrap();
    set(&raw, Outcome::Album, true);
    let xml = read(&raw);
    assert_eq!(count(&xml, "<rdf:li>Blooper</rdf:li>"), 1, "{xml}");
    assert!(xml.contains("<rdf:li>PhotoSift|Blooper</rdf:li>"));
    assert!(!xml.contains("BlooperSet"), "{xml}");
    set(&raw, Outcome::Album, false);
    let xml = read(&raw);
    assert_eq!(count(&xml, "<rdf:li>Blooper</rdf:li>"), 1, "{xml}");
    assert!(!xml.contains("PhotoSift|Blooper"), "{xml}");
}

#[test]
fn users_funny_blooper_hierarchy_keeps_its_leaf() {
    let dir = tempfile::tempdir().unwrap();
    let raw = raw_in(dir.path());
    std::fs::write(sidecar(&raw), doc_with(&[], &["Funny|Blooper"])).unwrap();
    set(&raw, Outcome::Album, true);
    assert!(read(&raw).contains("BlooperSet=\"1\""));
    assert_eq!(count(&read(&raw), "<rdf:li>Blooper</rdf:li>"), 1);
    set(&raw, Outcome::Album, false);
    let xml = read(&raw);
    assert!(xml.contains("<rdf:li>Funny|Blooper</rdf:li>"), "{xml}");
    assert_eq!(count(&xml, "<rdf:li>Blooper</rdf:li>"), 1, "{xml}");
    assert!(!xml.contains("PhotoSift|Blooper"));
    assert!(!xml.contains("BlooperSet"), "ownership is released: {xml}");
}
