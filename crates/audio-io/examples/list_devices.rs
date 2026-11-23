//! Manual run for M1-01: reads the audio devices of this Mac and prints them,
//! then the report rows that depend on devices only. It only reads; it starts
//! no process other than the one the reader starts and changes no setting.

use audio_io::{
    build_report, microphone, read_system_devices, MicPermission, ModelState, ProcessRunner, RowId,
    SetupFacts,
};

const NOT_READ: &str = "not read in this task";

fn main() {
    let snapshot = match read_system_devices(&ProcessRunner) {
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
    // The permission, the tap check and the models are read by later tasks:
    // the facts below are placeholders, and their rows are printed as not read.
    let facts = SetupFacts {
        devices: snapshot,
        tap_available: false,
        mic_permission: MicPermission::Unknown,
        models: [ModelState::Missing; 4],
    };
    for r in build_report(&facts).rows {
        match r.id {
            RowId::Mic | RowId::Vmic | RowId::Phones => println!(
                "row {:?} state={:?} detail={} fix={:?} warning={:?}",
                r.id, r.state, r.detail, r.fix, r.warning
            ),
            RowId::Capture | RowId::Sr | RowId::Tr | RowId::EnV | RowId::ViV => {
                println!("row {:?} {NOT_READ}", r.id)
            }
        }
    }
}
