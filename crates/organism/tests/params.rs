//! `Params` must carry every token of `design-system/tokens.json` used by the port plan.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::approx_constant)]

use organism::{Extras, Params};
use serde_json::Value;

fn tokens() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../design-system/tokens.json"
    );
    let text = std::fs::read_to_string(path).expect("read tokens.json");
    serde_json::from_str(&text).expect("parse tokens.json")
}

fn num(v: &Value, path: &str) -> f64 {
    let mut cur = v;
    for part in path.split('.') {
        cur = match part.parse::<usize>() {
            Ok(i) => &cur[i],
            Err(_) => &cur[part],
        };
    }
    cur.as_f64()
        .unwrap_or_else(|| panic!("token {path} is not a number: {cur}"))
}

#[test]
fn params_match_tokens_json_exactly() {
    let t = tokens();
    let p = Params::from_tokens();
    let o = |k: &str| num(&t, &format!("organism.{k}"));
    let m = |k: &str| num(&t, &format!("motion.physics.{k}"));
    let eq = |name: &str, got: f64, want: f64| assert_eq!(got, want, "{name}");

    eq("habitatRadius", p.habitat_radius, o("habitatRadius"));
    eq("ringRadius", p.ring_radius, o("ringRadius"));
    eq("bodyRadius", p.body_radius, o("bodyRadius"));
    let by = p.body_radius_by_state;
    eq("by.idle", by.idle, o("bodyRadiusByState.idle"));
    eq(
        "by.listening",
        by.listening,
        o("bodyRadiusByState.listening"),
    );
    eq("by.thinking", by.thinking, o("bodyRadiusByState.thinking"));
    eq("by.speaking", by.speaking, o("bodyRadiusByState.speaking"));
    eq(
        "by.delegating",
        by.delegating,
        o("bodyRadiusByState.delegating"),
    );
    let n = p.membrane;
    eq("noise.amp", n.amplitude, o("membraneNoise.amplitude"));
    eq("noise.breath", n.breath, o("membraneNoise.breath"));
    eq(
        "noise.breathRad",
        n.breath_rad_per_second,
        o("membraneNoise.breathRadPerSecond"),
    );
    for i in 0..3 {
        for j in 0..3 {
            eq(
                "harmonic",
                n.harmonics[i][j],
                o(&format!("membraneNoise.harmonics.{i}.{j}")),
            );
        }
        eq(
            "phaseMult",
            n.phase_multipliers[i],
            o(&format!("membraneNoise.phaseMultipliers.{i}")),
        );
    }
    let r = p.voice_ripple;
    eq("ripple.amp", r.amplitude, o("voiceRipple.amplitude"));
    for i in 0..2 {
        eq(
            "ripple.angular",
            r.angular[i],
            o(&format!("voiceRipple.angular.{i}")),
        );
        eq(
            "ripple.temporal",
            r.temporal[i],
            o(&format!("voiceRipple.temporal.{i}")),
        );
    }
    let b = p.bulge;
    eq("bulge.amount", b.amount, o("bulgeTowardCell.amount"));
    eq("bulge.sigma", b.sigma, o("bulgeTowardCell.sigma"));
    eq("bulge.dead", b.dead_factor, o("bulgeTowardCell.deadFactor"));
    eq(
        "bulge.extra",
        b.birth_extra,
        o("bulgeTowardCell.birthExtra"),
    );
    eq(
        "bulge.secs",
        b.birth_seconds,
        o("bulgeTowardCell.birthSeconds"),
    );
    eq("nucleusRadius", p.nucleus_radius, o("nucleusRadius"));
    eq(
        "nucleusFollowMax",
        p.nucleus_follow_max,
        o("nucleusFollowMax"),
    );
    let s = p.nucleus_scale;
    eq("ns.gain", s.amp_gain, o("nucleusScale.ampGain"));
    eq(
        "ns.tp.amount",
        s.thinking_pulse_amount,
        o("nucleusScale.thinkingPulse.amount"),
    );
    eq(
        "ns.tp.rad",
        s.thinking_pulse_rad,
        o("nucleusScale.thinkingPulse.radPerSecond"),
    );
    eq(
        "ns.op.amount",
        s.other_pulse_amount,
        o("nucleusScale.otherPulse.amount"),
    );
    eq(
        "ns.op.rad",
        s.other_pulse_rad,
        o("nucleusScale.otherPulse.radPerSecond"),
    );
    let g = p.organelles;
    eq("org.count", g.count as f64, o("organelles.count"));
    eq("org.min", g.size_min, o("organelles.sizeMin"));
    eq("org.max", g.size_max, o("organelles.sizeMax"));
    eq("org.idle", g.speed_idle, o("organelles.speedByState.idle"));
    eq(
        "org.thinking",
        g.speed_thinking,
        o("organelles.speedByState.thinking"),
    );
    eq(
        "org.speaking",
        g.speed_speaking,
        o("organelles.speedByState.speaking"),
    );
    let ps = p.pseudopod;
    eq("ps.samples", ps.samples as f64, o("pseudopod.samples"));
    eq("ps.wobble", ps.wobble, o("pseudopod.wobble"));
    eq("ps.wobbleDead", ps.wobble_dead, o("pseudopod.wobbleDead"));
    eq(
        "ps.gain",
        ps.wobble_voice_gain,
        o("pseudopod.wobbleVoiceGain"),
    );
    eq("ps.vesicles", ps.vesicles as f64, o("pseudopod.vesicles"));
    eq(
        "ps.working",
        ps.flow_working,
        o("pseudopod.flowSpeed.working"),
    );
    eq(
        "ps.waiting",
        ps.flow_waiting,
        o("pseudopod.flowSpeed.waiting"),
    );
    eq(
        "ps.thinking",
        ps.flow_thinking_multiplier,
        o("pseudopod.flowSpeed.thinkingMultiplier"),
    );
    let c = p.cell;
    eq("cell.radius", c.radius, o("cellRadius"));
    eq("cell.hover", c.hover_scale, o("cellHoverScale"));
    eq("cell.noiseW", c.noise_working, o("cellNoise.working"));
    eq("cell.noiseO", c.noise_other, o("cellNoise.other"));
    eq("cell.ratio", c.nucleus_ratio, o("cellNucleusRatio"));
    eq("cell.beat", c.beat_rad, o("cellBeat.radPerSecond"));
    eq("cell.beatAmt", c.beat_amount, o("cellBeat.amount"));
    eq("cell.blink", c.blink_rad, o("cellBlink.radPerSecond"));
    eq("cell.blinkMin", c.blink_min_alpha, o("cellBlink.minAlpha"));
    eq("cell.tremor", c.tremor_rad, o("errorTremor.radPerSecond"));
    eq("cell.tremorAmt", c.tremor_amount, o("errorTremor.amount"));
    eq(
        "cell.tremorRatio",
        c.tremor_nucleus_ratio,
        o("errorTremor.nucleusRatio"),
    );
    eq("pendingAlpha", c.pending_alpha, o("pendingAlpha"));
    let ring = p.ring;
    eq("ring.min", ring.min_slots as f64, o("ring.minSlots"));
    eq("ring.max", ring.max_cells as f64, o("ring.maxCells"));
    eq(
        "ring.compact",
        ring.compact_after_seconds,
        o("ring.compactAfterSeconds"),
    );
    eq("ring.offset", ring.slot_offset, o("ring.slotOffset"));
    let sc = p.scaling;
    eq("sc.ref", sc.reference_width, o("scaling.referenceWidth"));
    eq(
        "sc.stroke",
        sc.min_stroke_device_px,
        o("scaling.minStrokeDevicePx"),
    );
    eq("sc.halo", sc.min_halo_px, o("scaling.minHaloPx"));
    eq(
        "sc.labels",
        sc.hide_labels_below,
        o("scaling.hideLabelsBelow"),
    );
    eq("sc.hit", sc.min_hit_radius_px, o("scaling.minHitRadiusPx"));
    eq("env.idle", p.envelope.idle, o("voiceEnvelope.idle"));
    eq(
        "env.thinking",
        p.envelope.thinking,
        o("voiceEnvelope.thinking"),
    );
    eq("waves.speed", p.waves.speed, o("waves.speed"));
    eq(
        "waves.decay",
        p.waves.decay_per_second,
        o("waves.decayPerSecond"),
    );
    eq(
        "waves.rate",
        p.waves.speaking_rate_per_second,
        o("waves.speakingRatePerSecond"),
    );
    let ph = p.physics;
    eq("k", ph.cell_spring_k, m("cellSpringK"));
    eq("kNew", ph.cell_spring_k_newborn, m("cellSpringKNewborn"));
    eq(
        "damp",
        ph.cell_damping_per_second,
        m("cellDampingPerSecond"),
    );
    eq("gain", ph.cell_position_gain, m("cellPositionGain"));
    eq("nucLerp", ph.nucleus_follow_lerp, m("nucleusFollowLerp"));
    eq(
        "ampLerp",
        ph.amplitude_lerp_per_second,
        m("amplitudeLerpPerSecond"),
    );
    eq(
        "hoverLerp",
        ph.hover_lerp_per_second,
        m("hoverLerpPerSecond"),
    );
    let du = p.durations;
    eq(
        "d.birth",
        du.cell_birth,
        num(&t, "motion.durations.cellBirth"),
    );
    eq("d.wave", du.wave_life, num(&t, "motion.durations.waveLife"));
    eq(
        "d.death",
        du.cell_death,
        num(&t, "motion.durations.cellDeath"),
    );
    eq(
        "d.terminal",
        du.remove_terminal_after,
        num(&t, "motion.durations.removeTerminalAfter"),
    );
    eq(
        "d.overlay",
        du.delegating_overlay,
        num(&t, "motion.durations.delegatingOverlay"),
    );
}

#[test]
fn extras_hold_the_hidden_reference_constants() {
    let e = Extras::default();
    assert_eq!(e.tau_reference, 6.283);
    assert_eq!(e.drift_amplitude, 6.0);
    assert_eq!((e.organelle_speed_min, e.organelle_speed_span), (0.2, 0.6));
    assert_eq!(e.hover_radius, 34.0);
    assert_eq!(e.click_radius, 40.0);
}
