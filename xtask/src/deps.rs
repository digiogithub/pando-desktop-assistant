//! `cargo xtask check-deps`: dependency-direction and GPUI-pin checks (AGENTS.md section 6).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, MetadataCommand};

/// One package of the resolved graph.
#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub is_member: bool,
    /// Normal and build dependencies actually resolved (name only).
    pub deps: BTreeSet<String>,
    /// Declared (normal/build) dependencies: (name, version requirement).
    pub declared: Vec<(String, String)>,
    /// Cargo features enabled on this package in the resolved graph.
    pub features: BTreeSet<String>,
}

/// Crates the pure layers (`organism`, `decide`) must never reach, even transitively.
const PURE_CRATES: [&str; 2] = ["organism", "decide"];
const PURE_BANNED: [&str; 3] = ["reqwest", "tokio", "cpal"];

/// Allowed edges between workspace crates (normal dependencies only).
fn allowed_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let set = |xs: &[&'static str]| xs.iter().copied().collect::<BTreeSet<_>>();
    BTreeMap::from([
        ("organism", set(&[])),
        ("audio", set(&[])),
        ("needle-sys", set(&[])),
        ("decide", set(&[])),
        ("voice-proto", set(&[])),
        ("pando-client", set(&[])),
        ("claude-bridge", set(&["voice-proto"])),
        ("pando-stt", set(&["voice-proto", "needle-sys"])),
        ("pando-tts", set(&["voice-proto"])),
        (
            "app",
            set(&[
                "organism",
                "decide",
                "pando-client",
                "voice-proto",
                "audio",
                "claude-bridge",
            ]),
        ),
        ("xtask", set(&[])),
    ])
}

/// Features of the sherpa-onnx crates that fetch prebuilt (TTS-enabled) libraries.
const PREBUILT_FEATURES: [&str; 2] = ["download-binaries", "static"];

fn is_sherpa(n: &str) -> bool {
    n == "sherpa-onnx"
        || n.starts_with("sherpa-onnx-")
        || n == "sherpa-rs"
        || n.starts_with("sherpa-rs-")
}

fn is_gpui(n: &str) -> bool {
    n == "gpui" || n.starts_with("gpui-") || n.starts_with("gpui_")
}

/// Run all checks against the real workspace. Returns `Ok(true)` when clean.
pub fn run() -> Result<bool> {
    let metadata = MetadataCommand::new()
        .exec()
        .context("running `cargo metadata`")?;
    let resolve = metadata.resolve.context("metadata has no resolve graph")?;
    let ids: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|p| (p.id.clone(), p))
        .collect();
    let members: BTreeSet<_> = metadata.workspace_members.iter().cloned().collect();

    let mut pkgs = Vec::new();
    for node in &resolve.nodes {
        let Some(pkg) = ids.get(&node.id) else {
            continue;
        };
        let deps = node
            .deps
            .iter()
            .filter(|d| {
                d.dep_kinds
                    .iter()
                    .any(|k| k.kind != DependencyKind::Development)
            })
            .filter_map(|d| ids.get(&d.pkg).map(|p| p.name.to_string()))
            .collect();
        let declared = pkg
            .dependencies
            .iter()
            .filter(|d| d.kind != DependencyKind::Development)
            .map(|d| (d.name.clone(), d.req.to_string()))
            .collect();
        pkgs.push(Pkg {
            name: pkg.name.to_string(),
            is_member: members.contains(&node.id),
            deps,
            declared,
            features: node.features.iter().map(|f| f.to_string()).collect(),
        });
    }

    let errors = check(&pkgs);
    if errors.is_empty() {
        let n = pkgs.iter().filter(|p| p.is_member).count();
        println!("check-deps: OK ({n} workspace crates, dependency direction, GPUI pin)");
        Ok(true)
    } else {
        for e in &errors {
            eprintln!("check-deps: ERROR: {e}");
        }
        eprintln!("check-deps: {} problem(s) found", errors.len());
        Ok(false)
    }
}

/// Pure check over a package graph; returns human-readable violations.
pub fn check(pkgs: &[Pkg]) -> Vec<String> {
    let mut errors = Vec::new();
    let by_name: BTreeMap<&str, &Pkg> = pkgs.iter().map(|p| (p.name.as_str(), p)).collect();
    let allowed = allowed_edges();

    // Transitive closure of normal dependencies.
    let closure = |start: &str| -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start.to_string()];
        while let Some(n) = stack.pop() {
            if let Some(p) = by_name.get(n.as_str()) {
                for d in &p.deps {
                    if seen.insert(d.clone()) {
                        stack.push(d.clone());
                    }
                }
            }
        }
        seen
    };

    for p in pkgs.iter().filter(|p| p.is_member) {
        let reach = closure(&p.name);
        // 1. only `app` may reach GPUI.
        if p.name != "app"
            && let Some(g) = reach.iter().find(|d| is_gpui(d))
        {
            errors.push(format!(
                "`{}` depends (directly or transitively) on `{g}`; only `app` may use gpui-kit",
                p.name
            ));
        }
        // 2. pure layers have no network, audio or async-runtime dependencies.
        if PURE_CRATES.contains(&p.name.as_str()) {
            for banned in PURE_BANNED {
                if reach.contains(banned) {
                    errors.push(format!(
                        "pure crate `{}` depends (directly or transitively) on `{banned}`",
                        p.name
                    ));
                }
            }
        }
        // 2b. sherpa-onnx (ADR 0007): never in `app`; `pando-stt` only without prebuilt
        // libraries (which embed espeak-ng); only `pando-tts` may use it with TTS.
        if let Some(sh) = reach.iter().find(|d| is_sherpa(d))
            && p.name != "pando-stt"
            && p.name != "pando-tts"
        {
            errors.push(format!(
                "`{}` depends (directly or transitively) on `{sh}`; only `pando-stt` (source build, TTS off) and `pando-tts` may use sherpa-onnx",
                p.name
            ));
        }
        if p.name == "pando-stt" {
            for d in reach.iter().filter(|d| is_sherpa(d)) {
                let prebuilt = by_name.get(d.as_str()).is_some_and(|q| {
                    q.features
                        .iter()
                        .any(|f| PREBUILT_FEATURES.contains(&f.as_str()))
                });
                if prebuilt {
                    errors.push(format!(
                        "`pando-stt` reaches prebuilt sherpa-onnx (`{d}` with a download/static feature); build from source with TTS off (ADR 0007)"
                    ));
                }
            }
        }
        // 3. no direct `gpui` dependency anywhere; gpui-kit must be pinned with `=`.
        for (dep, req) in &p.declared {
            if dep == "gpui" {
                errors.push(format!(
                    "`{}` declares a direct `gpui` dependency; use `gpui_kit::gpui`",
                    p.name
                ));
            }
            if dep == "gpui-kit" && !req.trim_start().starts_with('=') {
                errors.push(format!(
                    "`{}` requires gpui-kit `{req}`; it must be an exact `=x.y.z` pin",
                    p.name
                ));
            }
        }
        // 4. dependency direction among workspace crates.
        match allowed.get(p.name.as_str()) {
            None => errors.push(format!(
                "workspace crate `{}` is not known to xtask check-deps; add it to the allowed-edge table",
                p.name
            )),
            Some(ok) => {
                for d in &p.deps {
                    let workspace_dep = by_name.get(d.as_str()).is_some_and(|q| q.is_member);
                    if workspace_dep && !ok.contains(d.as_str()) {
                        errors.push(format!("forbidden dependency edge `{}` -> `{d}`", p.name));
                    }
                }
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, member: bool, deps: &[&str]) -> Pkg {
        Pkg {
            name: name.into(),
            is_member: member,
            deps: deps.iter().map(|s| s.to_string()).collect(),
            declared: vec![],
            features: Default::default(),
        }
    }

    fn good() -> Vec<Pkg> {
        let mut app = pkg("app", true, &["organism", "decide", "gpui-kit"]);
        app.declared = vec![("gpui-kit".into(), "=0.7.1".into())];
        vec![
            pkg("organism", true, &[]),
            pkg("decide", true, &[]),
            app,
            pkg("gpui-kit", false, &["gpui-pre"]),
            pkg("gpui-pre", false, &[]),
            pkg("reqwest", false, &[]),
        ]
    }

    fn add_dep(v: &mut [Pkg], to: &str, dep: &str) {
        for p in v.iter_mut().filter(|p| p.name == to) {
            p.deps.insert(dep.into());
        }
    }

    #[test]
    fn clean_graph_passes() {
        assert_eq!(check(&good()), Vec::<String>::new());
    }

    #[test]
    fn reqwest_in_organism_fails() {
        let mut v = good();
        add_dep(&mut v, "organism", "reqwest");
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("`reqwest`")), "{errs:?}");
    }

    #[test]
    fn transitive_tokio_in_decide_fails() {
        let mut v = good();
        v.push(pkg("tokio", false, &[]));
        v.push(pkg("helper", false, &["tokio"]));
        add_dep(&mut v, "decide", "helper");
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("`tokio`")), "{errs:?}");
    }

    #[test]
    fn gpui_kit_outside_app_fails() {
        let mut v = good();
        add_dep(&mut v, "organism", "gpui-kit");
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("only `app`")), "{errs:?}");
    }

    #[test]
    fn unpinned_gpui_kit_fails() {
        let mut v = good();
        for p in v.iter_mut().filter(|p| p.name == "app") {
            p.declared = vec![("gpui-kit".into(), "^0.7.1".into())];
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("exact")), "{errs:?}");
    }

    #[test]
    fn direct_gpui_fails() {
        let mut v = good();
        for p in v.iter_mut().filter(|p| p.name == "app") {
            p.declared.push(("gpui".into(), "0.2".into()));
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("direct `gpui`")), "{errs:?}");
    }

    #[test]
    fn reverse_edge_fails() {
        let mut v = good();
        add_dep(&mut v, "organism", "app");
        let errs = check(&v);
        assert!(
            errs.iter().any(|e| e.contains("`organism` -> `app`")),
            "{errs:?}"
        );
    }

    fn with_workers() -> Vec<Pkg> {
        let mut v = good();
        v.push(pkg("pando-stt", true, &[]));
        v.push(pkg("pando-tts", true, &[]));
        v.push(pkg("sherpa-onnx", false, &["sherpa-onnx-sys"]));
        v.push(pkg("sherpa-onnx-sys", false, &[]));
        v
    }

    #[test]
    fn sherpa_in_app_fails() {
        let mut v = with_workers();
        add_dep(&mut v, "app", "sherpa-onnx");
        let errs = check(&v);
        assert!(
            errs.iter().any(|e| e.contains("only `pando-stt`")),
            "{errs:?}"
        );
    }

    #[test]
    fn sherpa_in_tts_passes() {
        let mut v = with_workers();
        add_dep(&mut v, "pando-tts", "sherpa-onnx");
        assert_eq!(check(&v), Vec::<String>::new());
    }

    #[test]
    fn prebuilt_sherpa_in_stt_fails() {
        let mut v = with_workers();
        add_dep(&mut v, "pando-stt", "sherpa-onnx");
        for p in v.iter_mut().filter(|p| p.name == "sherpa-onnx-sys") {
            p.features.insert("download-binaries".into());
        }
        let errs = check(&v);
        assert!(errs.iter().any(|e| e.contains("prebuilt")), "{errs:?}");
    }

    #[test]
    fn source_built_sherpa_in_stt_passes() {
        let mut v = with_workers();
        add_dep(&mut v, "pando-stt", "sherpa-onnx");
        assert_eq!(check(&v), Vec::<String>::new());
    }

    #[test]
    fn unknown_member_fails() {
        let mut v = good();
        v.push(pkg("mystery", true, &[]));
        assert!(!check(&v).is_empty());
    }
}
