//! Generator and strict schema for the organism-side tokens (`motion`, `engines`, `status`,
//! `organism`), written to `crates/organism/src/tokens.rs`.
//!
//! The organism schema is validated here so an unknown or missing key fails
//! `cargo xtask tokens --check`; the runtime may still load the same JSON with serde.

use std::fmt::Write as _;

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value};

use crate::tokens::{HEADER_RS, float, hex_of, marfil_alphas, num, object, screaming};

pub const OUTPUT: &str = "crates/organism/src/tokens.rs";

/// Shape of a token value.
enum S {
    Num,
    Str,
    /// Numeric array of fixed length.
    Vec(usize),
    /// Array of numeric arrays of fixed inner length.
    Mat(usize),
    /// Array of strings.
    StrVec,
    Obj(&'static [(&'static str, S)]),
}

use S::{Mat, Num, Obj, Str, StrVec, Vec as Vn};

const MOTION: S = Obj(&[
    ("springEase", Str),
    ("springEaseSoft", Str),
    (
        "durations",
        Obj(&[
            ("hover", Num),
            ("stateMorph", Num),
            ("nucleusMorph", Num),
            ("cellBirth", Num),
            ("waveLife", Num),
            ("cellDeath", Num),
            ("removeTerminalAfter", Num),
            ("delegatingOverlay", Num),
        ]),
    ),
    (
        "physics",
        Obj(&[
            ("cellSpringK", Num),
            ("cellSpringKNewborn", Num),
            ("cellDampingPerSecond", Num),
            ("cellPositionGain", Num),
            ("nucleusFollowLerp", Num),
            ("amplitudeLerpPerSecond", Num),
            ("hoverLerpPerSecond", Num),
            ("note", Str),
        ]),
    ),
]);

const PULSE: S = Obj(&[("amount", Num), ("radPerSecond", Num)]);

const ORGANISM: S = Obj(&[
    ("canvasAspect", Str),
    ("habitatRadius", Num),
    ("ringRadius", Num),
    ("bodyRadius", Num),
    (
        "bodyRadiusByState",
        Obj(&[
            ("idle", Num),
            ("listening", Num),
            ("thinking", Num),
            ("speaking", Num),
            ("delegating", Num),
        ]),
    ),
    (
        "membraneNoise",
        Obj(&[
            ("amplitude", Num),
            ("breath", Num),
            ("breathRadPerSecond", Num),
            ("harmonics", Mat(3)),
            ("phaseMultipliers", Vn(3)),
            ("note", Str),
        ]),
    ),
    (
        "voiceRipple",
        Obj(&[("amplitude", Num), ("angular", Vn(2)), ("temporal", Vn(2))]),
    ),
    (
        "bulgeTowardCell",
        Obj(&[
            ("amount", Num),
            ("sigma", Num),
            ("deadFactor", Num),
            ("birthExtra", Num),
            ("birthSeconds", Num),
        ]),
    ),
    ("nucleusRadius", Num),
    ("nucleusFollowMax", Num),
    (
        "nucleusScale",
        Obj(&[
            ("ampGain", Num),
            ("thinkingPulse", PULSE),
            ("otherPulse", PULSE),
            ("note", Str),
        ]),
    ),
    (
        "organelles",
        Obj(&[
            ("count", Num),
            ("sizeMin", Num),
            ("sizeMax", Num),
            (
                "speedByState",
                Obj(&[("idle", Num), ("thinking", Num), ("speaking", Num)]),
            ),
        ]),
    ),
    (
        "pseudopod",
        Obj(&[
            ("samples", Num),
            ("wobble", Num),
            ("wobbleDead", Num),
            ("wobbleVoiceGain", Num),
            ("vesicles", Num),
            (
                "flowSpeed",
                Obj(&[
                    ("working", Num),
                    ("waiting", Num),
                    ("thinkingMultiplier", Num),
                ]),
            ),
        ]),
    ),
    ("cellRadius", Num),
    ("cellHoverScale", Num),
    ("cellNoise", Obj(&[("working", Num), ("other", Num)])),
    ("cellNucleusRatio", Num),
    (
        "cellBeat",
        Obj(&[("radPerSecond", Num), ("amount", Num), ("note", Str)]),
    ),
    (
        "cellBlink",
        Obj(&[("radPerSecond", Num), ("minAlpha", Num), ("note", Str)]),
    ),
    (
        "errorTremor",
        Obj(&[
            ("radPerSecond", Num),
            ("amount", Num),
            ("nucleusRatio", Num),
            ("note", Str),
        ]),
    ),
    ("pendingAlpha", Num),
    (
        "ring",
        Obj(&[
            ("minSlots", Num),
            ("maxCells", Num),
            ("compactAfterSeconds", Num),
            ("slotOffset", Num),
            ("note", Str),
        ]),
    ),
    (
        "scaling",
        Obj(&[
            ("referenceWidth", Num),
            ("minStrokeDevicePx", Num),
            ("minHaloPx", Num),
            ("hideLabelsBelow", Num),
            ("minHitRadiusPx", Num),
            ("note", Str),
        ]),
    ),
    (
        "voiceEnvelope",
        Obj(&[
            ("idle", Num),
            ("thinking", Num),
            ("listening", Str),
            ("speaking", Str),
            ("note", Str),
        ]),
    ),
    (
        "waves",
        Obj(&[
            ("speed", Num),
            ("decayPerSecond", Num),
            ("speakingRatePerSecond", Num),
            ("emitOn", StrVec),
            ("note", Str),
        ]),
    ),
]);

const ENGINE_NAMES: [&str; 4] = ["pando", "acp", "external", "cli"];
const STATUS_NAMES: [&str; 6] = ["pending", "working", "waiting", "paused", "done", "error"];
/// Optional descriptive keys of a status entry (all strings except `cellAlpha`).
const STATUS_KEYS: [&str; 6] = [
    "nucleus",
    "membrane",
    "pseudopod",
    "badge",
    "cellAlpha",
    "badgeColor",
];

fn is_num_array(v: &Value, len: usize) -> bool {
    v.as_array()
        .is_some_and(|a| a.len() == len && a.iter().all(Value::is_number))
}

/// Check `value` against `schema`; unknown and missing keys are errors.
fn validate(schema: &S, value: &Value, path: &str) -> Result<()> {
    match schema {
        Num if value.is_number() => Ok(()),
        Str if value.is_string() => Ok(()),
        StrVec
            if value
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)) =>
        {
            Ok(())
        }
        Vn(n) if is_num_array(value, *n) => Ok(()),
        Mat(n)
            if value.as_array().is_some_and(|rows| {
                !rows.is_empty() && rows.iter().all(|r| is_num_array(r, *n))
            }) =>
        {
            Ok(())
        }
        Obj(fields) => {
            let map = value
                .as_object()
                .ok_or_else(|| anyhow!("`{path}` must be an object"))?;
            for key in map.keys() {
                if !fields.iter().any(|(k, _)| k == key) {
                    bail!("unknown key `{path}.{key}`");
                }
            }
            for (key, sub) in *fields {
                let v = map
                    .get(*key)
                    .ok_or_else(|| anyhow!("missing key `{path}.{key}`"))?;
                validate(sub, v, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => bail!("`{path}` has the wrong type"),
    }
}

fn exact_keys(map: &Map<String, Value>, want: &[&str], path: &str) -> Result<()> {
    for k in map.keys() {
        if !want.contains(&k.as_str()) {
            bail!("unknown key `{path}.{k}`");
        }
    }
    for k in want {
        if !map.contains_key(*k) {
            bail!("missing key `{path}.{k}`");
        }
    }
    Ok(())
}

/// Validate every organism-side group of `tokens`.
pub fn validate_all(tokens: &Value) -> Result<()> {
    for (name, schema) in [("motion", &MOTION), ("organism", &ORGANISM)] {
        validate(
            schema,
            tokens
                .get(name)
                .ok_or_else(|| anyhow!("tokens.json has no `{name}`"))?,
            name,
        )?;
    }
    let colors = object(tokens, "color")?;
    let engines = object(tokens, "engines")?;
    exact_keys(engines, &ENGINE_NAMES, "engines")?;
    for (name, e) in engines {
        let e = e
            .as_object()
            .ok_or_else(|| anyhow!("engines.{name} is not an object"))?;
        let path = format!("engines.{name}");
        exact_keys(
            e,
            &["nucleus", "fill", "membrane", "canTalk", "label"],
            &path,
        )?;
        let nucleus = e["nucleus"].as_str().unwrap_or_default();
        if nucleus == "marfilAlpha" || !colors.contains_key(nucleus) {
            bail!("`{path}.nucleus` is not a colour token: `{nucleus}`");
        }
        if !e["fill"].is_boolean() || !e["canTalk"].is_boolean() || !e["label"].is_string() {
            bail!("`{path}` has fields of the wrong type");
        }
        if !matches!(e["membrane"].as_str(), Some("solid" | "dashed")) {
            bail!("`{path}.membrane` must be `solid` or `dashed`");
        }
    }
    let status = object(tokens, "status")?;
    exact_keys(status, &STATUS_NAMES, "status")?;
    for (name, s) in status {
        let s = s
            .as_object()
            .ok_or_else(|| anyhow!("status.{name} is not an object"))?;
        for (k, v) in s {
            let path = format!("status.{name}.{k}");
            if !STATUS_KEYS.contains(&k.as_str()) {
                bail!("unknown key `{path}`");
            }
            let ok = match k.as_str() {
                "cellAlpha" => v.is_number(),
                "badge" => v.is_string() || v.is_null(),
                _ => v.is_string(),
            };
            if !ok {
                bail!("`{path}` has the wrong type");
            }
        }
        if let Some(c) = s.get("badgeColor").and_then(Value::as_str)
            && !colors.contains_key(c)
        {
            bail!("`status.{name}.badgeColor` is not a colour token: `{c}`");
        }
    }
    Ok(())
}

fn string_lit(s: &str) -> String {
    format!("{s:?}")
}

/// Parse `cubic-bezier(a, b, c, d)`.
fn bezier(s: &str, path: &str) -> Result<[f64; 4]> {
    let inner = s
        .strip_prefix("cubic-bezier(")
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| anyhow!("{path} is not cubic-bezier(...)"))?;
    let v: Vec<f64> = inner
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()?;
    <[f64; 4]>::try_from(v).map_err(|_| anyhow!("{path} needs 4 numbers"))
}

/// Emit `pub const PREFIX_PATH: type = value;` for every leaf of an object (notes skipped).
fn emit_flat(out: &mut String, prefix: &str, map: &Map<String, Value>) -> Result<()> {
    for (k, v) in map {
        if k == "note" {
            continue;
        }
        let name = format!("{prefix}_{}", screaming(k));
        match v {
            Value::Object(m) => emit_flat(out, &name, m)?,
            Value::Number(_) => writeln!(out, "pub const {name}: f32 = {};", float(num(v, k)?))?,
            Value::String(s) => writeln!(out, "pub const {name}: &str = {};", string_lit(s))?,
            Value::Array(a) if a.iter().all(Value::is_string) => {
                let items: Vec<String> = a
                    .iter()
                    .map(|x| string_lit(x.as_str().unwrap_or_default()))
                    .collect();
                writeln!(
                    out,
                    "pub const {name}: [&str; {}] = [{}];",
                    a.len(),
                    items.join(", ")
                )?;
            }
            Value::Array(a) => {
                let row = |r: &Value| -> Result<String> {
                    let items: Result<Vec<String>> = r
                        .as_array()
                        .ok_or_else(|| anyhow!("{name}: expected array"))?
                        .iter()
                        .map(|x| num(x, &name).map(float))
                        .collect();
                    Ok(format!("[{}]", items?.join(", ")))
                };
                if a.first().is_some_and(Value::is_array) {
                    let inner = a[0].as_array().map_or(0, Vec::len);
                    let rows: Result<Vec<String>> = a.iter().map(row).collect();
                    writeln!(
                        out,
                        "pub const {name}: [[f32; {inner}]; {}] = [{}];",
                        a.len(),
                        rows?.join(", ")
                    )?;
                } else {
                    writeln!(out, "pub const {name}: [f32; {}] = {};", a.len(), row(v)?)?;
                }
            }
            _ => bail!("{name}: unsupported value"),
        }
    }
    Ok(())
}

pub fn generate(tokens: &Value) -> Result<String> {
    validate_all(tokens)?;
    let mut rs = String::from(HEADER_RS);
    rs.push_str("//! Design tokens for the organism: colours, motion, engines, status and geometry.\n//!\n//! Times are seconds, radii are fractions of the canvas width.\n\n");

    rs.push_str("/// A colour with alpha: `rgb` is `0xRRGGBB`, `a` is 0-255.\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub struct Color {\n    pub rgb: u32,\n    pub a: u8,\n}\n\n");
    rs.push_str("impl Color {\n    pub const fn opaque(rgb: u32) -> Self {\n        Self { rgb, a: 255 }\n    }\n}\n\n");

    // Colours.
    let colors = object(tokens, "color")?;
    for name in colors.keys().filter(|k| *k != "marfilAlpha") {
        writeln!(
            rs,
            "pub const {}: Color = Color::opaque(0x{});",
            screaming(name),
            hex_of(tokens, name)?
        )?;
    }
    let marfil = hex_of(tokens, "marfil")?;
    for (step, a) in marfil_alphas(tokens)? {
        writeln!(
            rs,
            "pub const MARFIL_ALPHA_{step}: Color = Color {{\n    rgb: 0x{marfil},\n    a: {a},\n}};"
        )?;
    }

    // Motion.
    rs.push_str("\n// Motion\n");
    let motion = object(tokens, "motion")?;
    for (key, label) in [
        ("springEase", "SPRING_EASE"),
        ("springEaseSoft", "SPRING_EASE_SOFT"),
    ] {
        let b = bezier(motion[key].as_str().unwrap_or_default(), key)?;
        writeln!(
            rs,
            "/// `cubic-bezier` control points (x1, y1, x2, y2).\npub const {label}: [f32; 4] = [{}, {}, {}, {}];",
            float(b[0]),
            float(b[1]),
            float(b[2]),
            float(b[3])
        )?;
    }
    for group in ["durations", "physics"] {
        let prefix = if group == "durations" {
            "DURATION"
        } else {
            "PHYSICS"
        };
        let map = motion[group]
            .as_object()
            .ok_or_else(|| anyhow!("motion.{group} is not an object"))?;
        emit_flat(&mut rs, prefix, map)?;
    }

    // Engines.
    rs.push_str("\n// Engines\n/// Membrane stroke style of a cell.\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum Membrane {\n    Solid,\n    Dashed,\n}\n\n");
    rs.push_str("/// How one engine kind is drawn. Rule: fill = you can address it; colour = who it is.\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub struct EngineToken {\n    pub id: &'static str,\n    pub nucleus: Color,\n    pub fill: bool,\n    pub membrane: Membrane,\n    pub can_talk: bool,\n    pub label: &'static str,\n}\n\n");
    let engines = object(tokens, "engines")?;
    writeln!(
        rs,
        "pub const ENGINES: [EngineToken; {}] = [",
        engines.len()
    )?;
    for (id, e) in engines {
        let membrane = if e["membrane"] == "dashed" {
            "Dashed"
        } else {
            "Solid"
        };
        writeln!(
            rs,
            "    EngineToken {{\n        id: {},\n        nucleus: {},\n        fill: {},\n        membrane: Membrane::{membrane},\n        can_talk: {},\n        label: {},\n    }},",
            string_lit(id),
            screaming(e["nucleus"].as_str().unwrap_or_default()),
            e["fill"],
            e["canTalk"],
            string_lit(e["label"].as_str().unwrap_or_default()),
        )?;
    }
    rs.push_str("];\n");

    // Status.
    rs.push_str("\n// Status\n/// How one task status moves. Descriptive strings; the simulation owns the behaviour.\n#[derive(Clone, Copy, Debug, PartialEq)]\npub struct StatusToken {\n    pub id: &'static str,\n    pub nucleus: Option<&'static str>,\n    pub membrane: Option<&'static str>,\n    pub pseudopod: Option<&'static str>,\n    pub badge: Option<&'static str>,\n    pub badge_color: Option<Color>,\n    pub cell_alpha: Option<f32>,\n}\n\n");
    let status = object(tokens, "status")?;
    writeln!(
        rs,
        "pub const STATUSES: [StatusToken; {}] = [",
        status.len()
    )?;
    for (id, s) in status {
        let opt_str = |k: &str| match s.get(k).and_then(Value::as_str) {
            Some(v) => format!("Some({})", string_lit(v)),
            None => "None".to_owned(),
        };
        let badge_color = match s.get("badgeColor").and_then(Value::as_str) {
            Some(c) => format!("Some({})", screaming(c)),
            None => "None".to_owned(),
        };
        let cell_alpha = match s.get("cellAlpha").and_then(Value::as_f64) {
            Some(a) => format!("Some({})", float(a)),
            None => "None".to_owned(),
        };
        writeln!(
            rs,
            "    StatusToken {{\n        id: {},\n        nucleus: {},\n        membrane: {},\n        pseudopod: {},\n        badge: {},\n        badge_color: {badge_color},\n        cell_alpha: {cell_alpha},\n    }},",
            string_lit(id),
            opt_str("nucleus"),
            opt_str("membrane"),
            opt_str("pseudopod"),
            opt_str("badge"),
        )?;
    }
    rs.push_str("];\n");

    // Organism geometry and physics.
    rs.push_str(
        "\n// Organism geometry (fractions of the canvas width) and simulation constants\n",
    );
    emit_flat(&mut rs, "ORGANISM", object(tokens, "organism")?)?;
    crate::tokens::rustfmt(&rs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{read_tokens, repo_root};

    fn tokens() -> Value {
        read_tokens(&repo_root().expect("root")).expect("tokens")
    }

    #[test]
    fn vendored_tokens_validate() {
        validate_all(&tokens()).expect("schema");
    }

    #[test]
    fn unknown_organism_key_fails() {
        let mut t = tokens();
        t["organism"]["cellBeat"]["bogus"] = Value::from(1);
        let e = validate_all(&t).expect_err("unknown key");
        assert!(e.to_string().contains("organism.cellBeat.bogus"), "{e}");
    }

    #[test]
    fn missing_organism_key_fails() {
        let mut t = tokens();
        t["organism"]
            .as_object_mut()
            .expect("obj")
            .remove("ringRadius");
        let e = validate_all(&t).expect_err("missing key");
        assert!(e.to_string().contains("organism.ringRadius"), "{e}");
    }

    #[test]
    fn wrong_type_fails() {
        let mut t = tokens();
        t["organism"]["cellRadius"] = Value::from("big");
        assert!(validate_all(&t).is_err());
    }

    #[test]
    fn engine_with_unknown_colour_fails() {
        let mut t = tokens();
        t["engines"]["acp"]["nucleus"] = Value::from("magenta");
        assert!(validate_all(&t).is_err());
    }

    #[test]
    fn generate_rejects_invalid_tokens() {
        let mut t = tokens();
        t["motion"]["durations"]["extra"] = Value::from(1);
        assert!(generate(&t).is_err());
    }

    #[test]
    fn bezier_parses() {
        assert_eq!(
            bezier("cubic-bezier(0.34, 1.9, 0.5, 1)", "x").expect("b"),
            [0.34, 1.9, 0.5, 1.0]
        );
    }
}
