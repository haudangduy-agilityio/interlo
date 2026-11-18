//! Crate-level error that wraps the per-module errors.

use crate::capture::CaptureError;
use crate::output::OutputError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AudioIoError {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Output(#[from] OutputError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_module_errors() {
        let e: AudioIoError = CaptureError::NoCaptureRoute.into();
        assert_eq!(e, AudioIoError::Capture(CaptureError::NoCaptureRoute));
        let e: AudioIoError = OutputError::NotImplemented.into();
        assert_eq!(e, AudioIoError::Output(OutputError::NotImplemented));
    }
}
