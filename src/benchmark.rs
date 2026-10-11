//! Opt-in synthetic benchmark: 10 ms UI heartbeat, for native responsiveness measurements.
use crate::view::Conversation;
use gpui_kit::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
fn cpu_ticks() -> u64 {
    let s = std::fs::read_to_string("/proc/self/stat").unwrap();
    let parts: Vec<_> = s.rsplit_once(')').unwrap().1.split_whitespace().collect();
    parts[11].parse::<u64>().unwrap() + parts[12].parse::<u64>().unwrap()
}
fn rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}
async fn wait(view: &WeakEntity<Conversation>, cx: &mut AsyncWindowContext) {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let done = view
            .update_in(cx, |v, _, _| {
                assert!(v.error.is_empty(), "Benchmark protocol error: {}", v.error);
                v.ready && !v.loading
            })
            .unwrap();
        if done {
            return;
        }
        assert!(Instant::now() < end, "Benchmark load timed out");
        cx.background_executor()
            .timer(Duration::from_millis(2))
            .await;
    }
}
pub fn start(
    view: &Entity<Conversation>,
    room: String,
    output: String,
    window: &mut Window,
    cx: &mut App,
) {
    let samples = Rc::new(RefCell::new(Vec::<f64>::new()));
    let intervals = samples.clone();
    let executor = cx.background_executor().clone();
    let heartbeat = cx.spawn(async move |_| {
        let mut last = Instant::now();
        loop {
            executor.timer(Duration::from_millis(10)).await;
            let now = Instant::now();
            intervals
                .borrow_mut()
                .push(now.duration_since(last).as_secs_f64() * 1000.);
            last = now;
        }
    });
    view.update(cx, |_, cx| {
        cx.spawn_in(window, async move |view, cx| {
            run(view, room, output, samples, heartbeat, cx).await
        })
        .detach();
    });
}
async fn run(
    view: WeakEntity<Conversation>,
    room: String,
    output: String,
    samples: Rc<RefCell<Vec<f64>>>,
    heartbeat: Task<()>,
    cx: &mut AsyncWindowContext,
) {
    let start = Instant::now();
    wait(&view, cx).await;
    let startup = start.elapsed().as_secs_f64() * 1000.;
    let t = Instant::now();
    view.update_in(cx, |v, w, cx| v.select(room, w, cx))
        .unwrap();
    wait(&view, cx).await;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    let latest = t.elapsed().as_secs_f64() * 1000. - 100.;
    let mut page_times = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        view.update_in(cx, |v, _, cx| v.older(cx)).unwrap();
        wait(&view, cx).await;
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
        page_times.push(t.elapsed().as_secs_f64() * 1000. - 50.);
    }
    let loading_gap = samples.borrow().iter().copied().fold(0., f64::max);
    // Exercise variable-height list away from the tail, then return to the end.
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| {
            s.scroll_to_item(40, cx);
        });
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    let following = view
        .update_in(cx, |v, _, cx| v.scroller.read(cx).is_following_tail())
        .unwrap();
    assert!(!following, "Scroll-away should leave the tail");
    view.update_in(cx, |v, _, cx| {
        v.scroller.update(cx, |s, cx| s.scroll_to_end(cx))
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    drop(heartbeat);
    let ticks_per_second = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .unwrap();
    let hz = String::from_utf8_lossy(&ticks_per_second.stdout)
        .trim()
        .parse::<f64>()
        .unwrap();
    let idle_rows_start = view.update_in(cx, |v, _, _| v.rendered_rows.get()).unwrap();
    let cpu = cpu_ticks();
    let idle = Instant::now();
    cx.background_executor()
        .timer(Duration::from_secs(10))
        .await;
    let elapsed = idle.elapsed().as_secs_f64();
    let cpu_seconds = (cpu_ticks() - cpu) as f64 / hz;
    let idle_rows_end = view.update_in(cx, |v, _, _| v.rendered_rows.get()).unwrap();
    let settled_cpu = cpu_ticks();
    let settled_start = Instant::now();
    cx.background_executor()
        .timer(Duration::from_secs(10))
        .await;
    let settled_elapsed = settled_start.elapsed().as_secs_f64();
    let settled_cpu_seconds = (cpu_ticks() - settled_cpu) as f64 / hz;
    let settled_rows_end = view.update_in(cx, |v, _, _| v.rendered_rows.get()).unwrap();
    view.update_in(cx, |v, _, _| {
        assert!(
            v.rows.len() >= 500,
            "Benchmark must load at least 500 records"
        )
    })
    .unwrap();
    let report=view.update_in(cx,|v,_,_|serde_json::json!({
   "renderer":std::env::var("APRON_BENCH_RENDERER").unwrap_or_else(|_|"native Wayland / Vulkan (driver not detected by harness)".into()), "profile":if cfg!(debug_assertions){"debug"}else{"release"}, "seed_messages":1000,"seed_text_bytes_each":1045,
   "startup_to_first_history_ms":startup,"benchmark_room_latest_page_ms":latest,"older_page_ms":page_times,
   "max_10ms_heartbeat_interval_during_load_ms":loading_gap,"idle_seconds":elapsed,"idle_cpu_seconds":cpu_seconds,"idle_percent_one_core":cpu_seconds/elapsed*100.,
   "settled_idle_seconds":settled_elapsed,"settled_idle_cpu_seconds":settled_cpu_seconds,"settled_idle_percent_one_core":settled_cpu_seconds/settled_elapsed*100.,"idle_row_renderer_calls":idle_rows_end-idle_rows_start,"settled_idle_row_renderer_calls":settled_rows_end-idle_rows_end,
   "rss_mib":rss()as f64/1024.,"cached_messages":v.store.len(),"cached_serialized_bytes":v.store.bytes(),"transcript_records":v.rows.len(),"total_row_renderer_calls":v.rendered_rows.get(),"max_view_description_us":v.max_render_us,
   "scope":"UI heartbeat includes scheduling/layout/rendering delay, not GPU presentation latency. View-description time excludes layout/paint. Single warm run; no hardware-GPU claim."
  })).unwrap();
    std::fs::write(output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("{report}");
    cx.update(|_, cx| cx.quit()).unwrap();
}
