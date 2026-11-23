//! Core Audio queries and settings: devices, process objects, sample rates and the
//! audio-capture permission. All `unsafe` in this file calls a Core Audio function with
//! pointers to local values that outlive the call; each block says so. The code that
//! needs hardware (and so cannot be a unit test) is exercised by the evidence runs in
//! `docs/m0/M0b-05/evidence/`.

use objc2::rc::Retained;
use objc2_core_audio::{
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertyStreamConfiguration, kAudioDevicePropertyTransportType,
    kAudioDeviceTransportTypeVirtual, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioHardwarePropertyProcessObjectList,
    kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
    kAudioProcessPropertyIsRunningOutput, kAudioProcessPropertyPID, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectSetPropertyData,
};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList};
use objc2_foundation::NSString;
use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr::NonNull;

/// Plain old data that Core Audio may fill byte by byte: numbers and C structs of numbers.
pub trait Pod: Copy {
    fn zeroed() -> Self;
}

macro_rules! pod_numbers {
    ($($t:ty),*) => {$(impl Pod for $t { fn zeroed() -> Self { 0 as $t } })*};
}
pod_numbers!(u32, i32, f64);

impl Pod for objc2_core_audio_types::AudioStreamBasicDescription {
    fn zeroed() -> Self {
        Self {
            mSampleRate: 0.0,
            mFormatID: 0,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: 0,
            mBytesPerFrame: 0,
            mChannelsPerFrame: 0,
            mBitsPerChannel: 0,
            mReserved: 0,
        }
    }
}

pub const SYSTEM_OBJECT: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;
pub const SCOPE_GLOBAL: u32 = kAudioObjectPropertyScopeGlobal;
pub const SCOPE_INPUT: u32 = kAudioObjectPropertyScopeInput;
pub const SCOPE_OUTPUT: u32 = kAudioObjectPropertyScopeOutput;

pub fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Read one fixed-size value. `None` when Core Audio reports an error or another size.
pub fn get_value<T: Pod>(obj: AudioObjectID, selector: u32, scope: u32) -> Option<T> {
    let mut addr = address(selector, scope);
    let mut size = std::mem::size_of::<T>() as u32;
    let mut out = T::zeroed();
    // SAFETY: `addr`, `size` and `out` are live locals; `out` has room for `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            obj,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut out).cast(),
        )
    };
    (status == 0 && size as usize == std::mem::size_of::<T>()).then_some(out)
}

/// Read one fixed-size value from a property that takes a qualifier.
fn get_value_qualified<T: Pod, Q>(
    obj: AudioObjectID,
    selector: u32,
    scope: u32,
    qualifier: &Q,
) -> Option<T> {
    let mut addr = address(selector, scope);
    let mut size = std::mem::size_of::<T>() as u32;
    let mut out = T::zeroed();
    // SAFETY: all pointers are live locals or a borrow; sizes match their types.
    let status = unsafe {
        AudioObjectGetPropertyData(
            obj,
            NonNull::from(&mut addr),
            std::mem::size_of::<Q>() as u32,
            (qualifier as *const Q).cast(),
            NonNull::from(&mut size),
            NonNull::from(&mut out).cast(),
        )
    };
    (status == 0).then_some(out)
}

/// Read an array property.
pub fn get_array<T: Pod>(obj: AudioObjectID, selector: u32, scope: u32) -> Vec<T> {
    let mut addr = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: `addr` and `size` are live locals.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            obj,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut out = vec![T::zeroed(); size as usize / std::mem::size_of::<T>()];
    // SAFETY: `out` holds `size` bytes; Core Audio writes at most `size` bytes into it.
    let status = unsafe {
        AudioObjectGetPropertyData(
            obj,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(out.as_mut_ptr()).map_or(NonNull::dangling(), NonNull::cast),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    out.truncate(size as usize / std::mem::size_of::<T>());
    out
}

/// Write one fixed-size value. The error is the raw status code.
pub fn set_value<T: Copy>(
    obj: AudioObjectID,
    selector: u32,
    scope: u32,
    value: T,
) -> Result<(), i32> {
    let mut addr = address(selector, scope);
    let mut v = value;
    // SAFETY: `addr` and `v` are live locals; the size matches `T`.
    let status = unsafe {
        AudioObjectSetPropertyData(
            obj,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            NonNull::from(&mut v).cast(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Read a string property (Core Audio returns a retained string).
fn get_string(obj: AudioObjectID, selector: u32, scope: u32) -> Option<String> {
    let mut addr = address(selector, scope);
    let mut size = std::mem::size_of::<*mut NSString>() as u32;
    let mut out: *mut NSString = std::ptr::null_mut();
    // SAFETY: `out` is a live pointer-sized local; on success Core Audio stores a +1 retained string.
    let status = unsafe {
        AudioObjectGetPropertyData(
            obj,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut out).cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the pointer is null or an owned (+1) NSString, as the property contract states.
    let retained: Option<Retained<NSString>> = unsafe { Retained::from_raw(out) };
    retained.map(|s| s.to_string())
}

pub fn default_output_device() -> Option<AudioObjectID> {
    get_value::<u32>(
        SYSTEM_OBJECT,
        kAudioHardwarePropertyDefaultOutputDevice,
        SCOPE_GLOBAL,
    )
    .filter(|id| *id != 0)
}

pub fn device_uid(device: AudioObjectID) -> Option<String> {
    get_string(device, kAudioDevicePropertyDeviceUID, SCOPE_GLOBAL)
}

pub fn device_name(device: AudioObjectID) -> Option<String> {
    get_string(device, kAudioObjectPropertyName, SCOPE_GLOBAL)
}

pub fn device_rate(device: AudioObjectID) -> Option<f64> {
    get_value::<f64>(device, kAudioDevicePropertyNominalSampleRate, SCOPE_GLOBAL)
}

pub fn set_device_rate(device: AudioObjectID, hz: f64) -> Result<(), i32> {
    set_value(
        device,
        kAudioDevicePropertyNominalSampleRate,
        SCOPE_GLOBAL,
        hz,
    )
}

pub fn all_devices() -> Vec<AudioObjectID> {
    get_array::<u32>(SYSTEM_OBJECT, kAudioHardwarePropertyDevices, SCOPE_GLOBAL)
}

/// Channels of a device in one scope, summed over its stream buffers.
pub fn channel_count(device: AudioObjectID, scope: u32) -> u32 {
    let mut addr = address(kAudioDevicePropertyStreamConfiguration, scope);
    let mut size = 0u32;
    // SAFETY: `addr` and `size` are live locals.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            device,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 || (size as usize) < std::mem::size_of::<u32>() {
        return 0;
    }
    // u64 elements keep the buffer 8-byte aligned, as `AudioBufferList` needs.
    let mut raw = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: `raw` is at least `size` bytes long.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(raw.as_mut_ptr()).map_or(NonNull::dangling(), NonNull::cast),
        )
    };
    if status != 0 {
        return 0;
    }
    let list = raw.as_ptr().cast::<AudioBufferList>();
    // SAFETY: Core Audio filled a valid `AudioBufferList` of `size` bytes: the count first,
    // then that many buffers. We read only `n` buffers and stop at the buffer's byte length.
    unsafe {
        let n = (*list).mNumberBuffers as usize;
        let first = std::ptr::addr_of!((*list).mBuffers).cast::<AudioBuffer>();
        let max_buffers = (size as usize).saturating_sub(8) / std::mem::size_of::<AudioBuffer>();
        (0..n.min(max_buffers))
            .map(|i| (*first.add(i)).mNumberChannels)
            .sum()
    }
}

/// The 2-channel virtual audio device: a virtual-transport device with a "2ch" name and two
/// input channels. The name is matched by the generic "2ch" marker only.
pub fn find_two_ch_device() -> Option<AudioObjectID> {
    all_devices().into_iter().find(|d| {
        get_value::<u32>(*d, kAudioDevicePropertyTransportType, SCOPE_GLOBAL)
            == Some(kAudioDeviceTransportTypeVirtual)
            && device_name(*d).is_some_and(|n| n.contains("2ch"))
            && channel_count(*d, SCOPE_INPUT) == 2
    })
}

/// A process that has a Core Audio process object: its pid and whether it has an output
/// stream running.
#[derive(Debug, Clone)]
pub struct ProcessObject {
    pub pid: i32,
    pub running_output: bool,
}

pub fn process_objects() -> Vec<ProcessObject> {
    get_array::<u32>(
        SYSTEM_OBJECT,
        kAudioHardwarePropertyProcessObjectList,
        SCOPE_GLOBAL,
    )
    .into_iter()
    .map(|object| ProcessObject {
        pid: get_value::<i32>(object, kAudioProcessPropertyPID, SCOPE_GLOBAL).unwrap_or(-1),
        running_output: get_value::<u32>(
            object,
            kAudioProcessPropertyIsRunningOutput,
            SCOPE_GLOBAL,
        )
        .unwrap_or(0)
            != 0,
    })
    .collect()
}

/// The Core Audio process object of a pid, when that process has one.
pub fn process_object_for_pid(pid: u32) -> Option<AudioObjectID> {
    get_value_qualified::<u32, i32>(
        SYSTEM_OBJECT,
        kAudioHardwarePropertyTranslatePIDToProcessObject,
        SCOPE_GLOBAL,
        &(pid as i32),
    )
    .filter(|o| *o != 0)
}

/// State of the audio-capture permission of this app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tcc {
    Granted,
    Refused,
    /// Not decided yet (the dialog has not been answered) or the check is unavailable.
    Unknown,
}

extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// Ask the system for the audio-capture permission state without opening a dialog.
/// Uses the private TCC preflight call; there is no public call for this permission.
pub fn audio_capture_permission() -> Tcc {
    tcc_from_code(preflight_code())
}

/// Map the preflight code: 0 granted, 1 refused, anything else not decided.
pub fn tcc_from_code(code: Option<i32>) -> Tcc {
    match code {
        Some(0) => Tcc::Granted,
        Some(1) => Tcc::Refused,
        _ => Tcc::Unknown,
    }
}

/// The permission as seen by a fresh process. A long-running process keeps answering with the
/// state it first saw, so the spike asks a short-lived child of its own executable.
pub fn fresh_permission() -> Tcc {
    let code = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            std::process::Command::new(exe)
                .arg("permission-code")
                .output()
                .ok()
        })
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<i32>()
                .ok()
        });
    tcc_from_code(code)
}

/// Raw result of the preflight call, `None` when the call is unavailable.
pub fn preflight_code() -> Option<i32> {
    type Preflight = unsafe extern "C" fn(*const c_void, *const c_void) -> c_int;
    let (Ok(path), Ok(name)) = (
        CString::new("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC"),
        CString::new("TCCAccessPreflight"),
    ) else {
        return None;
    };
    // SAFETY: both strings are NUL-terminated and live for the calls; mode 2 is RTLD_NOW.
    let handle = unsafe { dlopen(path.as_ptr(), 2) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: `handle` is a valid library handle and `name` is NUL-terminated.
    let sym = unsafe { dlsym(handle, name.as_ptr()) };
    if sym.is_null() {
        return None;
    }
    // SAFETY: the symbol has the signature `int TCCAccessPreflight(CFStringRef, CFDictionaryRef)`.
    let preflight: Preflight = unsafe { std::mem::transmute::<*mut c_void, Preflight>(sym) };
    let service = NSString::from_str("kTCCServiceAudioCapture");
    // SAFETY: an NSString is a CFString (toll-free bridge); a null options pointer is allowed.
    let code = unsafe { preflight((&*service as *const NSString).cast(), std::ptr::null()) };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_codes_map_to_states() {
        assert_eq!(tcc_from_code(Some(0)), Tcc::Granted);
        assert_eq!(tcc_from_code(Some(1)), Tcc::Refused);
        assert_eq!(tcc_from_code(Some(2)), Tcc::Unknown);
        assert_eq!(tcc_from_code(None), Tcc::Unknown);
    }
}
