//! Capture routes and the decided route-selection order.

use crate::devices::{DeviceInfo, DeviceKind};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// No tap and no 2ch virtual device: the session must not start.
    #[error("no capture route is available")]
    NoCaptureRoute,
    #[error("capture is not implemented yet")]
    NotImplemented,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureMethod {
    ProcessTap { app_id: String },
    VirtualDevice { device_id: String },
}

/// A source of captured mono f32 samples. The consumer side is a ring buffer;
/// a slow consumer must lose samples, never stall capture.
pub trait CaptureSource {
    fn method(&self) -> &CaptureMethod;
    fn start(&mut self) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<(), CaptureError>;
}

/// Placeholder source until a real backend exists.
#[derive(Debug, Clone)]
pub struct UnimplementedCapture {
    method: CaptureMethod,
}

impl UnimplementedCapture {
    pub fn new(method: CaptureMethod) -> Self {
        Self { method }
    }
}

impl CaptureSource for UnimplementedCapture {
    fn method(&self) -> &CaptureMethod {
        &self.method
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::NotImplemented)
    }
}

/// Decided order: process tap for the chosen meeting app, else the 2ch
/// virtual device, else no route. An empty `app_id` means no app was chosen,
/// so the tap is skipped. A device that is not a 2ch virtual input is ignored.
pub fn capture_plan(
    tap_available: bool,
    app_id: &str,
    virtual_2ch: Option<&DeviceInfo>,
) -> Result<CaptureMethod, CaptureError> {
    if tap_available && !app_id.is_empty() {
        return Ok(CaptureMethod::ProcessTap {
            app_id: app_id.to_string(),
        });
    }
    match virtual_2ch {
        Some(d) if d.kind == DeviceKind::VirtualInput2ch => Ok(CaptureMethod::VirtualDevice {
            device_id: d.id.clone(),
        }),
        _ => Err(CaptureError::NoCaptureRoute),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virtual_dev() -> DeviceInfo {
        DeviceInfo::new("v2", "Virtual 2ch", DeviceKind::VirtualInput2ch)
    }

    #[test]
    fn tap_wins_when_available() {
        let d = virtual_dev();
        let m = capture_plan(true, "meeting.app", Some(&d)).unwrap();
        assert_eq!(
            m,
            CaptureMethod::ProcessTap {
                app_id: "meeting.app".into()
            }
        );
    }

    #[test]
    fn falls_back_to_virtual_device() {
        let d = virtual_dev();
        let m = capture_plan(false, "meeting.app", Some(&d)).unwrap();
        assert_eq!(
            m,
            CaptureMethod::VirtualDevice {
                device_id: "v2".into()
            }
        );
    }

    #[test]
    fn no_route_is_an_error() {
        assert_eq!(
            capture_plan(false, "meeting.app", None),
            Err(CaptureError::NoCaptureRoute)
        );
    }

    #[test]
    fn empty_app_id_skips_the_tap() {
        let d = virtual_dev();
        let m = capture_plan(true, "", Some(&d)).unwrap();
        assert!(matches!(m, CaptureMethod::VirtualDevice { .. }));
        assert_eq!(
            capture_plan(true, "", None),
            Err(CaptureError::NoCaptureRoute)
        );
    }

    #[test]
    fn wrong_device_kind_is_not_a_route() {
        let mic = DeviceInfo::new("m", "Mic", DeviceKind::Microphone);
        assert_eq!(
            capture_plan(false, "meeting.app", Some(&mic)),
            Err(CaptureError::NoCaptureRoute)
        );
    }

    #[test]
    fn placeholder_source_is_not_implemented() {
        let mut src = UnimplementedCapture::new(CaptureMethod::VirtualDevice {
            device_id: "v2".into(),
        });
        assert_eq!(src.start(), Err(CaptureError::NotImplemented));
        assert_eq!(src.stop(), Err(CaptureError::NotImplemented));
        assert!(matches!(src.method(), CaptureMethod::VirtualDevice { .. }));
    }
}
