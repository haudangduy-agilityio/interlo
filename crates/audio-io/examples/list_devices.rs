//! Manual run for M1-01 and M1-41: reads the audio devices of this Mac and prints
//! them, then the probed setup rows (microphone permission, capture route,
//! virtual microphone, headphones). It only reads; it opens the microphone for a
//! moment, starts only the two fixed system tools and changes no setting.

use audio_io::{
    microphone, probe_setup, read_devices, read_system_macos_version, ModelState, ProbeGate,
    SetupItemId, SystemEnv,
};

const NOT_READ: &str = "not read in this task";

fn main() {
    let snapshot = match read_devices() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("device list failed: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "{} devices, {} skipped",
        snapshot.devices.len(),
        snapshot.skipped
    );
    // Names stay out of the output: the evidence goes into a public pull request.
    for (n, d) in snapshot.devices.iter().enumerate() {
        println!(
            "device {} kind={:?} in={} out={} transport={:?} default_in={} default_out={}",
            n + 1,
            d.kind,
            d.input_channels,
            d.output_channels,
            d.transport,
            d.is_default_input,
            d.is_default_output
        );
    }
    println!(
        "microphone device: {}",
        if microphone(&snapshot.devices).is_some() {
            "found"
        } else {
            "none"
        }
    );
    // The microphone is opened for a moment (the permission is the one of the
    // terminal app, M0b-05 finding 1). Device names, levels and samples are
    // never printed. The models are read by the model manager (M1-04).
    match read_system_macos_version() {
        Some((major, minor)) => println!("macos version: {major}.{minor}"),
        None => println!("macos version: not read"),
    }
    let mut env = SystemEnv::new(ProbeGate::new());
    let outcome = probe_setup(&mut env, [ModelState::Missing; 4], false);
    println!("mic permission fact: {:?}", outcome.mic_permission);
    for r in outcome.report.items {
        match r.id {
            SetupItemId::Mic => println!("item {:?} state={:?} fix={:?}", r.id, r.state, r.fix),
            SetupItemId::Capture | SetupItemId::Vmic | SetupItemId::Phones => println!(
                "item {:?} state={:?} detail={} fix={:?} warning={:?}",
                r.id, r.state, r.detail, r.fix, r.warning
            ),
            SetupItemId::Sr | SetupItemId::Tr | SetupItemId::EnV | SetupItemId::ViV => {
                println!("item {:?} {NOT_READ}", r.id)
            }
        }
    }
}
