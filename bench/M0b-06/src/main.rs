//! Spike M0b-06: a panel over a full-screen window. It owns no product code, stores nothing and
//! opens no network connection (guardrails 1 and 2); product crates never import it (D-190).

mod analysis;
mod app;
mod cli;
mod command;
mod geometry;
mod log;
#[allow(unsafe_code)]
mod native_panel;
#[allow(unsafe_code)]
mod platform;
mod snapshot;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("analyze") {
        let read = |path: &str| std::fs::read_to_string(path).ok();
        let (code, line) = cli::analyze(args.get(1..).unwrap_or_default(), &read);
        println!("{line}");
        std::process::exit(code);
    }
    app::run(&args);
}
