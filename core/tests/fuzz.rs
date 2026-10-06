//! Property-based fuzzing of everything that parses peer-supplied data.
//! Runs thousands of random and adversarial inputs on every `cargo test`.

use std::path::{Component, Path};

use proptest::prelude::*;
use vegamshare_core::pathsafe::{self, sanitize_component, validate};
use vegamshare_core::proto::{Offer, PairConfirm, Request, Response};

/// Path-ish strings biased towards the characters attackers use.
fn evil_path() -> impl Strategy<Value = String> {
    // Weighted so a good share of results are valid paths: the interesting
    // cases are hostile pieces mixed into otherwise plausible ones.
    let component = prop_oneof![
        40 => "[a-zA-Z0-9_-]{1,8}(\\.[a-z]{1,3})?",
        1 => Just("/".to_string()),
        1 => Just("".to_string()),
        1 => Just("..".to_string()),
        1 => Just(".".to_string()),
        1 => Just("\\".to_string()),
        1 => Just(":".to_string()),
        1 => Just("C:".to_string()),
        1 => Just("\\\\?\\".to_string()),
        1 => Just("CON".to_string()),
        1 => Just("nul.txt".to_string()),
        1 => Just(" ".to_string()),
        1 => Just("\u{0}".to_string()),
        1 => Just("\u{202e}".to_string()),
        1 => Just("ファイル".to_string()),
        1 => any::<String>(),
    ];
    prop_oneof![
        9 => prop::collection::vec(component, 1..6).prop_map(|v| v.join("/")),
        1 => any::<String>(),
    ]
}

fn assert_contained(p: &str) {
    for c in Path::new(&p.replace('/', std::path::MAIN_SEPARATOR_STR)).components() {
        assert!(
            matches!(c, Component::Normal(_)),
            "accepted path {p:?} has non-normal component {c:?}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 4000, ..ProptestConfig::default() })]

    #[test]
    fn accepted_paths_never_escape(p in evil_path()) {
        if let Ok(parts) = validate(&p) {
            assert_contained(&p);
            for c in parts {
                prop_assert!(!c.is_empty() && c != "." && c != "..");
                prop_assert!(!c.contains(['/', '\\', ':', '\0']));
                prop_assert!(!c.ends_with('.') && !c.ends_with(' '));
            }
            let root = Path::new("root");
            let joined = pathsafe::join_under(root, &validate(&p).unwrap()).unwrap();
            prop_assert!(joined.starts_with(root));
        }
    }

    #[test]
    fn sanitized_names_always_validate(name in any::<String>()) {
        let s = sanitize_component(&name);
        prop_assert!(validate(&s).is_ok(), "{name:?} -> {s:?}");
        prop_assert_eq!(validate(&s).unwrap().len(), 1);
    }

    #[test]
    fn sanitize_is_identity_on_valid_names(name in "[a-zA-Z0-9_-][a-zA-Z0-9 ._-]{0,30}[a-zA-Z0-9_-]") {
        prop_assume!(validate(&name).is_ok());
        prop_assert_eq!(sanitize_component(&name), name);
    }

    #[test]
    fn parsers_never_panic_on_bytes(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = serde_json::from_slice::<Request>(&bytes);
        let _ = serde_json::from_slice::<Response>(&bytes);
        let _ = serde_json::from_slice::<Offer>(&bytes);
        let _ = serde_json::from_slice::<PairConfirm>(&bytes);
    }

    #[test]
    fn parsers_never_panic_on_json_shaped_input(
        t in prop_oneof![Just("ping"), Just("pair_start"), Just("offer"), Just("file"), Just("resume"), Just("x")],
        n in any::<i64>(),
        s in any::<String>(),
    ) {
        let docs = [
            format!(r#"{{"type":"{t}","transfer_id":{s:?},"index":{n},"offset":{n}}}"#),
            format!(r#"{{"type":"{t}","info":{{"device_id":{s:?},"name":{s:?},"platform":"x","version":{n}}},"spake":{s:?}}}"#),
            format!(r#"{{"type":"{t}","offer":{{"transfer_id":{s:?},"entries":[{{"path":{s:?},"is_dir":false,"size":{n},"mtime":{n}}}]}}}}"#),
        ];
        for d in docs {
            let _ = serde_json::from_str::<Request>(&d);
        }
    }
}

/// The path property must actually see accepted paths, not just rejects.
#[test]
fn path_fuzz_is_not_vacuous() {
    use proptest::test_runner::{Config, TestRunner};
    let mut runner = TestRunner::new(Config { cases: 4000, ..Config::default() });
    let ok = std::cell::Cell::new(0u32);
    let total = std::cell::Cell::new(0u32);
    runner
        .run(&evil_path(), |p| {
            total.set(total.get() + 1);
            if validate(&p).is_ok() {
                ok.set(ok.get() + 1);
            }
            Ok(())
        })
        .unwrap();
    let (ok, total) = (ok.get(), total.get());
    assert!(ok >= total / 5, "only {ok} of {total} generated paths were accepted");
    assert!(ok < total * 9 / 10, "generator produced too few hostile paths");
    eprintln!("path fuzz: {ok}/{total} accepted");
}
