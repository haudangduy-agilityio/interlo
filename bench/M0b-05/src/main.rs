mod analysis;
#[allow(unsafe_code)]
mod ca;
#[allow(unsafe_code)]
mod capture;
mod cli;
mod levels;
mod log;
mod monitor;
mod opts;
mod ring;
mod route;
#[allow(unsafe_code)]
mod sig;
mod spike;
mod targets;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = || args.get(1..).unwrap_or_default();
    let code = match args.first().map(String::as_str) {
        Some("permission") => {
            println!(
                "{:?} code={:?}",
                ca::audio_capture_permission(),
                ca::preflight_code()
            );
            0
        }
        Some("permission-code") => {
            println!("{}", ca::preflight_code().unwrap_or(-1));
            0
        }
        Some("analyze") => {
            let read = |path: &str| std::fs::read_to_string(path).ok();
            let (code, line) = cli::analyze(rest(), &read);
            println!("{line}");
            code
        }
        Some("rate") => cli::rate(rest()),
        Some("probe-output") => cli::probe_output(rest()),
        _ => match opts::parse(&args) {
            Ok(o) => spike::run(&o),
            Err(e) => {
                eprintln!("m0b05: {e}");
                cli::EXIT_USAGE
            }
        },
    };
    std::process::exit(code);
}
