//! Capture and playout through Core Audio: the process tap (with its private aggregate
//! device), capture from a plain input device, the sample-rate watcher and the spike's own
//! 1 kHz tone. The callbacks follow guardrail 8: no allocation, no lock, no log; they only
//! write into the atomic ring. The `unsafe` here is FFI; the evidence runs cover it.

use crate::ca;
use crate::levels::{WindowAccumulator, TONE_HZ};
use crate::ring::StatsRing;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceIsStackedKey,
    kAudioAggregateDeviceMainSubDeviceKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey,
    kAudioDevicePropertyNominalSampleRate, kAudioStreamPropertyVirtualFormat,
    kAudioSubDeviceUIDKey, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    kAudioTapPropertyFormat, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectAddPropertyListener, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectRemovePropertyListener, CATapDescription,
    CATapMuteBehavior,
};
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use std::ffi::{c_void, CStr};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Mono-mixes the first input buffer (interleaved Float32) and feeds the window accumulator.
struct CaptureState {
    acc: WindowAccumulator,
    ring: Arc<StatsRing>,
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C-unwind" fn capture_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the leaked `Box<CaptureState>` that `start` created; it stays alive
    // until `AudioDeviceStop` and `AudioDeviceDestroyIOProcID` have returned, and only this
    // callback touches it while IO runs.
    let state = unsafe { &mut *client.cast::<CaptureState>() };
    state.ring.note_callback();
    // SAFETY: Core Audio passes a valid buffer list for the duration of the callback.
    let list = unsafe { input.as_ref() };
    if list.mNumberBuffers == 0 {
        return 0;
    }
    let buf = &list.mBuffers[0];
    let channels = buf.mNumberChannels as usize;
    if channels == 0 || buf.mData.is_null() {
        return 0;
    }
    let frames = buf.mDataByteSize as usize / (4 * channels);
    // SAFETY: the buffer holds `mDataByteSize` bytes of Float32 samples (the tap and the virtual
    // input device both deliver interleaved Float32).
    let data = unsafe { std::slice::from_raw_parts(buf.mData.cast::<f32>(), frames * channels) };
    for frame in data.chunks_exact(channels) {
        let mono = frame.iter().sum::<f32>() / channels as f32;
        if let Some(window) = state.acc.push(mono) {
            state.ring.push(window);
        }
    }
    0
}

/// A running capture. Dropping it stops the capture and removes the tap and aggregate device.
pub struct Capture {
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    state: *mut CaptureState,
    tap: Option<AudioObjectID>,
    aggregate: Option<AudioObjectID>,
    pub rate_hz: u32,
}

fn ns_string(s: &CStr) -> Retained<NSString> {
    NSString::from_str(&s.to_string_lossy())
}

fn any<T: objc2::Message>(object: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an `AnyObject`; the retain count moves with the value.
    unsafe { Retained::cast_unchecked(object) }
}

impl Capture {
    /// Tap the given process objects (stereo mixdown) through a private aggregate device
    /// that clocks from the default output device.
    pub fn start_tap(
        process_objects: &[AudioObjectID],
        ring: Arc<StatsRing>,
    ) -> Result<Self, String> {
        let output = ca::default_output_device().ok_or("no default output device")?;
        let output_uid = ca::device_uid(output).ok_or("default output device has no uid")?;

        let numbers: Vec<Retained<NSNumber>> = process_objects
            .iter()
            .map(|o| NSNumber::new_u32(*o))
            .collect();
        let list = NSArray::from_retained_slice(&numbers);
        // SAFETY: plain Objective-C construction of the tap description with valid objects.
        let description = unsafe {
            let d =
                CATapDescription::initStereoMixdownOfProcesses(CATapDescription::alloc(), &list);
            d.setName(&NSString::from_str("m0b05 tap"));
            d.setUUID(&NSUUID::new());
            d.setPrivate(true);
            d.setMuteBehavior(CATapMuteBehavior::Unmuted);
            d
        };
        let mut tap: AudioObjectID = 0;
        // SAFETY: `description` is live and `tap` is a live local.
        let status = unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut tap) };
        if status != 0 {
            return Err(format!("AudioHardwareCreateProcessTap failed: {status}"));
        }
        // SAFETY: reading the UUID of a live description.
        let tap_uid = unsafe { description.UUID().UUIDString() };

        let sub_device = NSDictionary::from_retained_objects(
            &[&*ns_string(kAudioSubDeviceUIDKey)],
            &[any(NSString::from_str(&output_uid))],
        );
        let sub_tap = NSDictionary::from_retained_objects(
            &[
                &*ns_string(kAudioSubTapDriftCompensationKey),
                &*ns_string(kAudioSubTapUIDKey),
            ],
            &[any(NSNumber::new_bool(true)), any(tap_uid)],
        );
        let keys: [Retained<NSString>; 8] = [
            ns_string(kAudioAggregateDeviceNameKey),
            ns_string(kAudioAggregateDeviceUIDKey),
            ns_string(kAudioAggregateDeviceMainSubDeviceKey),
            ns_string(kAudioAggregateDeviceIsPrivateKey),
            ns_string(kAudioAggregateDeviceIsStackedKey),
            ns_string(kAudioAggregateDeviceTapAutoStartKey),
            ns_string(kAudioAggregateDeviceSubDeviceListKey),
            ns_string(kAudioAggregateDeviceTapListKey),
        ];
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let aggregate_uid = NSUUID::new().UUIDString();
        let dict = NSDictionary::from_retained_objects(
            &key_refs,
            &[
                any(NSString::from_str("m0b05 aggregate")),
                any(aggregate_uid),
                any(NSString::from_str(&output_uid)),
                any(NSNumber::new_bool(true)),
                any(NSNumber::new_bool(false)),
                any(NSNumber::new_bool(true)),
                any(NSArray::from_retained_slice(&[sub_device])),
                any(NSArray::from_retained_slice(&[sub_tap])),
            ],
        );
        let mut aggregate: AudioObjectID = 0;
        // SAFETY: an NSDictionary is a CFDictionary (toll-free bridge); `aggregate` is a live local.
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(
                &*Retained::as_ptr(&dict).cast(),
                NonNull::from(&mut aggregate),
            )
        };
        if status != 0 {
            // SAFETY: `tap` was created above and is destroyed once.
            unsafe { AudioHardwareDestroyProcessTap(tap) };
            return Err(format!(
                "AudioHardwareCreateAggregateDevice failed: {status}"
            ));
        }

        let rate_hz = ca::get_value::<AudioStreamBasicDescription>(
            tap,
            kAudioTapPropertyFormat,
            ca::SCOPE_GLOBAL,
        )
        .map(|f| f.mSampleRate)
        .or_else(|| ca::device_rate(aggregate))
        .unwrap_or(48_000.0) as u32;
        Self::start_io(aggregate, Some(tap), Some(aggregate), rate_hz, ring)
    }

    /// Capture from an input device (the 2-channel virtual device route).
    pub fn start_device(device: AudioObjectID, ring: Arc<StatsRing>) -> Result<Self, String> {
        let rate_hz = ca::device_rate(device).unwrap_or(48_000.0) as u32;
        Self::start_io(device, None, None, rate_hz, ring)
    }

    fn start_io(
        device: AudioObjectID,
        tap: Option<AudioObjectID>,
        aggregate: Option<AudioObjectID>,
        rate_hz: u32,
        ring: Arc<StatsRing>,
    ) -> Result<Self, String> {
        let state = Box::into_raw(Box::new(CaptureState {
            acc: WindowAccumulator::new(rate_hz),
            ring,
        }));
        let mut proc_id: AudioDeviceIOProcID = None;
        // SAFETY: `state` is a valid leaked box; `proc_id` is a live local; the callback matches
        // the `AudioDeviceIOProc` signature.
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                device,
                Some(capture_proc),
                state.cast(),
                NonNull::from(&mut proc_id),
            )
        };
        let mut capture = Capture {
            device,
            proc_id,
            state,
            tap,
            aggregate,
            rate_hz,
        };
        if status != 0 {
            capture.proc_id = None;
            return Err(format!("AudioDeviceCreateIOProcID failed: {status}"));
        }
        // SAFETY: the device and the proc id were created above.
        let status = unsafe { AudioDeviceStart(device, capture.proc_id) };
        if status != 0 {
            return Err(format!("AudioDeviceStart failed: {status}"));
        }
        Ok(capture)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: each id was created by this value and is released once; `AudioDeviceStop`
        // returns after the callback has stopped, so freeing `state` afterwards is sound.
        unsafe {
            if self.proc_id.is_some() {
                AudioDeviceStop(self.device, self.proc_id);
                AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            }
            if let Some(a) = self.aggregate {
                AudioHardwareDestroyAggregateDevice(a);
            }
            if let Some(t) = self.tap {
                AudioHardwareDestroyProcessTap(t);
            }
            drop(Box::from_raw(self.state));
        }
    }
}

/// Counts sample-rate change notifications of one device. The listener runs on a Core Audio
/// thread and only increments an atomic.
pub struct RateWatch {
    device: AudioObjectID,
    counter: *const AtomicU64,
    address: AudioObjectPropertyAddress,
}

unsafe extern "C-unwind" fn rate_listener(
    _object: AudioObjectID,
    _count: u32,
    _addresses: NonNull<AudioObjectPropertyAddress>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the pointer of the `Arc<AtomicU64>` that `RateWatch` keeps alive
    // until the listener is removed.
    let counter = unsafe { &*client.cast::<AtomicU64>() };
    counter.fetch_add(1, Ordering::Relaxed);
    0
}

impl RateWatch {
    pub fn new(device: AudioObjectID, changes: &Arc<AtomicU64>) -> Result<Self, String> {
        let mut address = ca::address(kAudioDevicePropertyNominalSampleRate, ca::SCOPE_GLOBAL);
        let counter = Arc::into_raw(Arc::clone(changes));
        // SAFETY: `address` is a live local; the listener and the counter pointer stay valid
        // until `Drop` removes the listener and releases the `Arc`.
        let status = unsafe {
            AudioObjectAddPropertyListener(
                device,
                NonNull::from(&mut address),
                Some(rate_listener),
                counter.cast_mut().cast(),
            )
        };
        if status != 0 {
            // SAFETY: reclaims the `Arc` leaked just above.
            drop(unsafe { Arc::from_raw(counter) });
            return Err(format!("AudioObjectAddPropertyListener failed: {status}"));
        }
        Ok(Self {
            device,
            counter,
            address,
        })
    }
}

impl Drop for RateWatch {
    fn drop(&mut self) {
        // SAFETY: removes the listener added in `new`, then releases the `Arc` it held.
        unsafe {
            AudioObjectRemovePropertyListener(
                self.device,
                NonNull::from(&mut self.address),
                Some(rate_listener),
                self.counter.cast_mut().cast(),
            );
            drop(Arc::from_raw(self.counter));
        }
    }
}

/// The spike's own 1 kHz tone on the default output device (AC-12).
struct PlayState {
    phase: f64,
    step: f64,
    /// Peak amplitude as `f32` bits; the main thread changes it while the callback runs.
    amplitude: AtomicU32,
}

unsafe extern "C-unwind" fn playout_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    _input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the leaked `Box<PlayState>` owned by `TonePlayer`, alive until stop.
    let state = unsafe { &mut *client.cast::<PlayState>() };
    // SAFETY: Core Audio passes a valid output buffer list for the duration of the callback.
    let list = unsafe { output.as_ref() };
    if list.mNumberBuffers == 0 {
        return 0;
    }
    let buf = &list.mBuffers[0];
    let channels = buf.mNumberChannels as usize;
    if channels == 0 || buf.mData.is_null() {
        return 0;
    }
    let frames = buf.mDataByteSize as usize / (4 * channels);
    // SAFETY: the output buffer holds `mDataByteSize` bytes of Float32 samples (checked at start).
    let data =
        unsafe { std::slice::from_raw_parts_mut(buf.mData.cast::<f32>(), frames * channels) };
    for frame in data.chunks_exact_mut(channels) {
        let sample =
            (state.phase.sin() as f32) * f32::from_bits(state.amplitude.load(Ordering::Relaxed));
        state.phase += state.step;
        if state.phase > std::f64::consts::TAU {
            state.phase -= std::f64::consts::TAU;
        }
        frame.fill(sample);
    }
    0
}

pub struct TonePlayer {
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    state: *mut PlayState,
}

impl TonePlayer {
    /// Play a 1 kHz sine of the given peak amplitude on the default output device.
    pub fn start(amplitude: f32) -> Result<Self, String> {
        let device = ca::default_output_device().ok_or("no default output device")?;
        Self::start_on(device, amplitude)
    }

    /// Play a 1 kHz sine on a given output device.
    pub fn start_on(device: AudioObjectID, amplitude: f32) -> Result<Self, String> {
        let format = ca::get_value::<AudioStreamBasicDescription>(
            device,
            kAudioStreamPropertyVirtualFormat,
            ca::SCOPE_OUTPUT,
        )
        .ok_or("cannot read the output format")?;
        if format.mFormatFlags & objc2_core_audio_types::kAudioFormatFlagIsFloat == 0
            || format.mFormatFlags & objc2_core_audio_types::kAudioFormatFlagIsNonInterleaved != 0
        {
            return Err("output format is not interleaved Float32".to_string());
        }
        let step = std::f64::consts::TAU * TONE_HZ / format.mSampleRate;
        let state = Box::into_raw(Box::new(PlayState {
            phase: 0.0,
            step,
            amplitude: AtomicU32::new(amplitude.to_bits()),
        }));
        let mut proc_id: AudioDeviceIOProcID = None;
        // SAFETY: `state` is a valid leaked box; `proc_id` is a live local.
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                device,
                Some(playout_proc),
                state.cast(),
                NonNull::from(&mut proc_id),
            )
        };
        let player = TonePlayer {
            device,
            proc_id,
            state,
        };
        if status != 0 {
            return Err(format!("AudioDeviceCreateIOProcID (tone) failed: {status}"));
        }
        // SAFETY: the device and proc id were created above.
        let status = unsafe { AudioDeviceStart(device, player.proc_id) };
        if status != 0 {
            return Err(format!("AudioDeviceStart (tone) failed: {status}"));
        }
        Ok(player)
    }
}

impl TonePlayer {
    /// Change the peak amplitude while playing (0.0 is silence; the output stream keeps running).
    pub fn set_amplitude(&self, amplitude: f32) {
        // SAFETY: `state` stays valid until `Drop`; only an atomic field is touched.
        let state = unsafe { &*self.state };
        state
            .amplitude
            .store(amplitude.to_bits(), Ordering::Relaxed);
    }
}

impl Drop for TonePlayer {
    fn drop(&mut self) {
        // SAFETY: stops and destroys the proc created in `start`, then frees its state once.
        unsafe {
            if self.proc_id.is_some() {
                AudioDeviceStop(self.device, self.proc_id);
                AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            }
            drop(Box::from_raw(self.state));
        }
    }
}
