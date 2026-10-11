//! PDA-T-0024: GPUI `paint_path` cost of an organism-like scene, plus a tiny-skia CPU baseline.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::gpui::{
    Bounds, Path, PathBuilder, Pixels, SharedString, TextAlign, TextRun, Window, canvas, point, px,
    rgba,
};
use gpui_kit::{
    App, AppContext as _, Context, Font, IntoElement, ParentElement as _, Render, Styled as _,
    WindowBounds, WindowOptions, div, size,
};

use crate::scene::{self, Poly};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Rebuild,
    Cached,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Order {
    TextLast,
    Interleaved,
}

struct Opts {
    mode: Mode,
    order: Order,
    scale: usize,
    frames: usize,
    warmup: usize,
}

fn parse(args: &[String]) -> Opts {
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    Opts {
        mode: if get("--mode") == Some("cached") {
            Mode::Cached
        } else {
            Mode::Rebuild
        },
        order: if get("--order") == Some("interleaved") {
            Order::Interleaved
        } else {
            Order::TextLast
        },
        scale: get("--scale").and_then(|s| s.parse().ok()).unwrap_or(1),
        frames: get("--frames").and_then(|s| s.parse().ok()).unwrap_or(600),
        warmup: get("--warmup").and_then(|s| s.parse().ok()).unwrap_or(60),
    }
}

#[derive(Default)]
struct Stats {
    build_us: Vec<f64>,
    paint_us: Vec<f64>,
    interval_us: Vec<f64>,
    last_start: Option<Instant>,
    frame: usize,
    started: Option<Instant>,
    points: usize,
    paths: usize,
    gpu: String,
}

fn build_path(p: &Poly, width: f32, dash: Option<(f32, f32)>) -> Option<Path<Pixels>> {
    let mut b = PathBuilder::stroke(px(width));
    if let Some((on, off)) = dash {
        b = b.dash_array(&[px(on), px(off)]);
    }
    let mut it = p.pts.iter();
    let (x, y) = *it.next()?;
    b.move_to(point(px(x), px(y)));
    for &(x, y) in it {
        b.line_to(point(px(x), px(y)));
    }
    if p.closed {
        b.close();
    }
    b.build().ok()
}

struct Item {
    halo: Option<(Path<Pixels>, [u8; 4])>,
    core: Option<(Path<Pixels>, [u8; 4])>,
}

fn build_items(polys: &[Poly]) -> Vec<Item> {
    polys
        .iter()
        .map(|p| Item {
            halo: p.halo.and_then(|(w, a)| {
                build_path(p, w, None).map(|path| (path, [p.rgba[0], p.rgba[1], p.rgba[2], a]))
            }),
            core: build_path(p, p.width, p.dash).map(|path| (path, p.rgba)),
        })
        .collect()
}

fn color(c: [u8; 4]) -> gpui_kit::Rgba {
    rgba(u32::from_be_bytes(c))
}

struct Perf {
    opts: Rc<Opts>,
    stats: Rc<RefCell<Stats>>,
    cache: Rc<RefCell<Option<Vec<Item>>>>,
}

impl Render for Perf {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (opts, stats, cache) = (self.opts.clone(), self.stats.clone(), self.cache.clone());
        div().size_full().bg(rgba(0x0F2A20FF)).child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, (), window, cx| {
                    let start = Instant::now();
                    {
                        let mut s = stats.borrow_mut();
                        if let Some(prev) = s.last_start {
                            s.interval_us.push((start - prev).as_secs_f64() * 1e6);
                        }
                        s.last_start = Some(start);
                        s.started.get_or_insert(start);
                    }
                    let t = start.duration_since(stats.borrow().started.unwrap_or(start)).as_secs_f32();
                    let polys = scene::build(opts.scale, t);
                    let origin = bounds.origin;
                    let sc = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 1000.0;
                    // Geometry is built in canvas units; the scene is scaled by transforming the
                    // points once so the cached variant measures pure repaint cost.
                    let polys: Vec<Poly> = polys
                        .into_iter()
                        .map(|mut p| {
                            for pt in &mut p.pts {
                                pt.0 = f32::from(origin.x) + pt.0 * sc;
                                pt.1 = f32::from(origin.y) + pt.1 * sc;
                            }
                            p
                        })
                        .collect();

                    let tb = Instant::now();
                    let fresh;
                    let mut guard = cache.borrow_mut();
                    let items: &Vec<Item> = if opts.mode == Mode::Cached {
                        guard.get_or_insert_with(|| build_items(&polys))
                    } else {
                        fresh = build_items(&polys);
                        &fresh
                    };
                    let build_us = if opts.mode == Mode::Cached && stats.borrow().frame > 0 {
                        0.0
                    } else {
                        tb.elapsed().as_secs_f64() * 1e6
                    };

                    let tp = Instant::now();
                    let mut paths = 0;
                    let run = TextRun {
                        len: 0,
                        font: Font::default(),
                        color: rgba(0xF4F1E8CC).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let mut labels: Vec<(SharedString, gpui_kit::Point<Pixels>)> = Vec::new();
                    for (i, item) in items.iter().enumerate() {
                        if let Some((p, c)) = &item.halo {
                            window.paint_path(p.clone(), color(*c));
                            paths += 1;
                        }
                        if let Some((p, c)) = &item.core {
                            window.paint_path(p.clone(), color(*c));
                            paths += 1;
                        }
                        if i % 2 == 1 && i < 100 {
                            let q = &polys[i].pts[0];
                            let label = (SharedString::from(format!("agent-{i}")), point(px(q.0), px(q.1)));
                            if opts.order == Order::Interleaved {
                                draw_label(window, cx, &label, &run);
                            } else {
                                labels.push(label);
                            }
                        }
                    }
                    for label in &labels {
                        draw_label(window, cx, label, &run);
                    }
                    let paint_us = tp.elapsed().as_secs_f64() * 1e6;

                    let mut s = stats.borrow_mut();
                    s.frame += 1;
                    s.points = scene::point_count(&polys);
                    s.paths = paths;
                    if s.gpu.is_empty() {
                        s.gpu = window
                            .gpu_specs()
                            .map(|g| format!("{} / {} / software={}", g.device_name, g.driver_name, g.is_software_emulated))
                            .unwrap_or_default();
                    }
                    if s.frame > opts.warmup {
                        s.build_us.push(build_us);
                        s.paint_us.push(paint_us);
                    }
                    let done = s.frame >= opts.frames + opts.warmup;
                    drop(s);
                    if done {
                        report(&opts, &stats.borrow());
                        cx.quit();
                    } else {
                        window.request_animation_frame();
                    }
                },
            )
            .size_full(),
        )
    }
}

fn draw_label(
    window: &mut Window,
    cx: &mut App,
    (text, at): &(SharedString, gpui_kit::Point<Pixels>),
    run: &TextRun,
) {
    let run = TextRun { len: text.len(), ..run.clone() };
    let line = window
        .text_system()
        .shape_line(text.clone(), px(11.0), &[run], None);
    let _ = line.paint(*at, px(14.0), TextAlign::Left, None, window, cx);
}

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[((s.len() - 1) as f64 * p) as usize]
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn report(o: &Opts, s: &Stats) {
    let skip = o.warmup.min(s.interval_us.len());
    let iv = &s.interval_us[skip..];
    let j = serde_json::json!({
        "spike": "paths",
        "gpu": s.gpu,
        "mode": format!("{:?}", o.mode),
        "order": format!("{:?}", o.order),
        "scale": o.scale,
        "points": s.points,
        "paint_path_calls": s.paths,
        "frames": s.build_us.len(),
        "build_us": {"mean": mean(&s.build_us), "p50": pct(&s.build_us, 0.5), "p95": pct(&s.build_us, 0.95)},
        "paint_us": {"mean": mean(&s.paint_us), "p50": pct(&s.paint_us, 0.5), "p95": pct(&s.paint_us, 0.95)},
        "interval_us": {"mean": mean(iv), "p50": pct(iv, 0.5), "p95": pct(iv, 0.95), "p99": pct(iv, 0.99)},
        "fps": if mean(iv) > 0.0 { 1e6 / mean(iv) } else { 0.0 },
    });
    println!("{j}");
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let opts = Rc::new(parse(args));
    gpui_kit::application().run(move |cx: &mut App| {
        let bounds = gpui_kit::Bounds::centered(None, size(px(1000.0), px(1000.0)), cx);
        let stats = Rc::new(RefCell::new(Stats::default()));
        let cache = Rc::new(RefCell::new(None));
        let watchdog_stats = stats.clone();
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| Perf { opts, stats, cache }),
        );
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(120)).await;
            eprintln!("watchdog: only {} frames", watchdog_stats.borrow().frame);
            std::process::exit(2);
        })
        .detach();
    });
    Ok(())
}

// ---- tiny-skia CPU baseline -------------------------------------------------------------

pub fn run_skia(args: &[String]) -> anyhow::Result<()> {
    let opts = parse(args);
    let size_px = 1000u32;
    let mut pm = tiny_skia::Pixmap::new(size_px, size_px).ok_or_else(|| anyhow::anyhow!("pixmap"))?;
    let mut times = Vec::new();
    let mut points = 0;
    for f in 0..(opts.frames + opts.warmup) {
        let polys = scene::build(opts.scale, f as f32 / 60.0);
        points = scene::point_count(&polys);
        let t0 = Instant::now();
        pm.fill(tiny_skia::Color::from_rgba8(0x0F, 0x2A, 0x20, 255));
        for p in &polys {
            let mut pb = tiny_skia::PathBuilder::new();
            let mut it = p.pts.iter();
            if let Some(&(x, y)) = it.next() {
                pb.move_to(x, y);
            }
            for &(x, y) in it {
                pb.line_to(x, y);
            }
            if p.closed {
                pb.close();
            }
            let Some(path) = pb.finish() else { continue };
            let draw = |pm: &mut tiny_skia::Pixmap, w: f32, c: [u8; 4], dash: Option<(f32, f32)>| {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
                paint.anti_alias = true;
                let mut stroke = tiny_skia::Stroke { width: w, ..Default::default() };
                if let Some((on, off)) = dash {
                    stroke.dash = tiny_skia::StrokeDash::new(vec![on, off], 0.0);
                }
                pm.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
            };
            if let Some((w, a)) = p.halo {
                draw(&mut pm, w, [p.rgba[0], p.rgba[1], p.rgba[2], a], None);
            }
            draw(&mut pm, p.width, p.rgba, p.dash);
        }
        if f >= opts.warmup {
            times.push(t0.elapsed().as_secs_f64() * 1e6);
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "spike": "skia", "scale": opts.scale, "points": points, "size": size_px,
            "frame_us": {"mean": mean(&times), "p50": pct(&times, 0.5), "p95": pct(&times, 0.95), "p99": pct(&times, 0.99)},
        })
    );
    Ok(())
}
