mod benchmark;
mod smoke;
mod view;
use gpui_kit::*;
fn main() {
    let mut bench = None;
    let mut smoke = None;
    let mut args = std::env::args().skip(1);
    let mut url = "ws://127.0.0.1:18765/ws".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--server" => url = args.next().expect("--server URL"),
            "--benchmark" => {
                let room = args.next().expect("--benchmark ROOM OUTPUT");
                let output = args.next().expect("--benchmark ROOM OUTPUT");
                bench = Some((room, output));
            }
            "--smoke" => smoke = Some(args.next().expect("--smoke OUTPUT")),
            "--help" => {
                println!(
                    "apron-gpui [--server wss://host/ws] [--smoke OUTPUT] [--benchmark ROOM OUTPUT]\nTest modes require a synthetic loopback server. Defaults to the synthetic loopback fixture; guest auth only."
                );
                return;
            }
            _ => {
                eprintln!("Unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    if smoke.is_some() && url != "ws://127.0.0.1:18765/ws" {
        panic!("Smoke mode is restricted to the synthetic loopback fixture");
    }
    if bench.is_some() && url != "ws://127.0.0.1:18766/ws" {
        panic!("Benchmark mode is restricted to the dedicated official loopback server");
    }
    if let Err(e) = apron_gpui::media::endpoint(&url) {
        eprintln!("Invalid server: {e}");
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.bind_keys([
                KeyBinding::new("ctrl-enter", view::SendMessage, Some("Apron")),
                // Override Textarea secondary-enter, which inserts a newline first.
                KeyBinding::new("ctrl-enter", view::SendMessage, Some("Input")),
                KeyBinding::new("ctrl-pageup", view::LoadOlder, Some("Apron")),
                KeyBinding::new("ctrl-pagedown", view::Latest, Some("Apron")),
                KeyBinding::new("escape", view::CancelReply, Some("Apron")),
                KeyBinding::new("escape", view::CancelReply, Some("Input")),
                KeyBinding::new("ctrl-b", view::ToggleRooms, Some("Apron")),
                KeyBinding::new("ctrl-b", view::ToggleRooms, Some("Input")),
            ]);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.), px(780.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Apron · GPUI comparison".into()),
                    ..Default::default()
                }),
                app_id: Some("chat.apron.GpuiExperiment".into()),
                ..Default::default()
            };
            let (handle, view) = gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| view::Conversation::new(url, window, cx))
            })
            .expect("Wayland/Vulkan window unavailable");
            if let Some((room, output)) = bench {
                handle
                    .update(cx, |_, window, cx| {
                        benchmark::start(&view, room, output, window, cx)
                    })
                    .unwrap();
            }
            if let Some(output) = smoke {
                handle
                    .update(cx, |_, window, cx| smoke::start(&view, output, window, cx))
                    .unwrap();
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        });
}
