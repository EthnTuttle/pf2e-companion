//! Imagery v1: bundled-art serving — fixture round-trip, traversal guard,
//! and the v1 bundle-size ceiling (docs/imagery/DEDUCTION.md §4.6.3).

#[path = "../src/images.rs"]
mod images;

use include_dir::{include_dir, Dir};

static FIXTURE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/imagery");

const FIXTURE_REL: &str = "lewisian/art/test/fixture.png";

/// Reference decoder (tests the encoder by inverse round-trip).
fn base64_decode(s: &str) -> Vec<u8> {
    fn val(b: u8) -> u32 {
        match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => 0,
            _ => panic!("bad base64 byte {b}"),
        }
    }
    let bytes: Vec<u8> = s.bytes().collect();
    assert_eq!(bytes.len() % 4, 0, "padded base64");
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&b| b == b'=').count();
        let n = val(chunk[0]) << 18 | val(chunk[1]) << 12 | val(chunk[2]) << 6 | val(chunk[3]);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    out
}

#[test]
fn fixture_image_round_trips_through_data_url() {
    let url = images::serve(&FIXTURE_DIR, FIXTURE_REL).expect("fixture serves");
    let payload = url
        .strip_prefix("data:image/png;base64,")
        .expect("png data-url prefix");
    let decoded = base64_decode(payload);
    let on_disk = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/imagery")
            .join(FIXTURE_REL),
    )
    .unwrap();
    assert_eq!(decoded, on_disk, "base64 round-trip must be byte-exact");
}

#[test]
fn traversal_and_non_image_paths_rejected() {
    for bad in [
        "../secret",
        "lewisian/../../etc/passwd",
        "/abs/path.png",
        "lewisian\\art\\x.png",
        "lewisian//art/x.png",
        "",
        "   ",
        "lewisian/art/test/fixture.gif",
        "lewisian/deities/yhwh.md",
    ] {
        assert!(
            images::serve(&FIXTURE_DIR, bad).is_err(),
            "expected rejection: {bad:?}"
        );
    }
}

#[test]
fn missing_image_is_an_error_not_a_panic() {
    assert!(images::serve(&FIXTURE_DIR, "lewisian/art/nope.webp").is_err());
}

/// The token-stamping probe reports `false` for unbundled/malformed paths
/// (fixture art lives outside `data/content`, so it never registers).
#[test]
fn existence_probe_only_sees_bundled_art() {
    for p in [
        FIXTURE_REL,
        "lewisian/art/nope.webp",
        "../secret",
        "",
        "lewisian/art/test/fixture.gif",
    ] {
        assert!(!images::bundled_image_exists(p), "unexpected hit: {p:?}");
    }
}

/// DEDUCTION §4.1/§4.6.3: promoted v1 art totals < 1 MiB inside the bundle.
#[test]
fn promoted_art_stays_under_v1_ceiling() {
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/content");
    let mut total: u64 = 0;
    let mut stack = vec![content];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "webp" || e == "png")
                && path.components().any(|c| c.as_os_str() == "art")
            {
                total += path.metadata().unwrap().len();
            }
        }
    }
    assert!(
        total <= 1024 * 1024,
        "bundled art tree is {total} B — over the 1 MiB v1 ceiling"
    );
}
