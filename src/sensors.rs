//! This code is from https://github.com/ishitatsuyuki/sensors/tree/fix-chip-filter
//!
//! TODO: use `sensors` crate when/if https://github.com/nyantec/sensors/pull/6 gets merged

#![allow(dead_code)]

use libsensors_sys as libsensors;

pub use libsensors::sensors_feature_type as FeatureType;
pub use libsensors::sensors_subfeature_type as SubfeatureType;

use std::ffi::CStr;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Once;

static INIT: Once = Once::new();

#[derive(Copy, Clone, Debug)]
pub enum LibsensorsError {
    Wildcards,
    NoEntry,
    AccessRead,
    Kernel,
    DivZero,
    ChipName,
    BusName,
    Parse,
    AccessWrite,
    IO,
    Recursion,
    Unknown,
}

impl LibsensorsError {
    fn from_i32(e: i32) -> LibsensorsError {
        use self::LibsensorsError::*;

        match e {
            libsensors::SENSORS_ERR_WILDCARDS => Wildcards,
            libsensors::SENSORS_ERR_NO_ENTRY => NoEntry,
            libsensors::SENSORS_ERR_ACCESS_R => AccessRead,
            libsensors::SENSORS_ERR_KERNEL => Kernel,
            libsensors::SENSORS_ERR_DIV_ZERO => DivZero,
            libsensors::SENSORS_ERR_CHIP_NAME => ChipName,
            libsensors::SENSORS_ERR_BUS_NAME => BusName,
            libsensors::SENSORS_ERR_PARSE => Parse,
            libsensors::SENSORS_ERR_ACCESS_W => AccessWrite,
            libsensors::SENSORS_ERR_IO => IO,
            libsensors::SENSORS_ERR_RECURSION => Recursion,
            _ => Unknown,
        }
    }
}

impl std::fmt::Display for LibsensorsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "libsensors error: {}",
            match *self {
                Self::Unknown => "Unknown error",
                Self::Wildcards => "Wildcard found in chip name",
                Self::NoEntry => "No such subfeature known",
                Self::AccessRead => "Can't read",
                Self::Kernel => "Kernel interface error",
                Self::DivZero => "Divide by zero",
                Self::ChipName => "Can't parse chip name",
                Self::BusName => "Can't parse bus name",
                Self::Parse => "General parse error",
                Self::AccessWrite => "Can't write",
                Self::IO => "I/O error",
                Self::Recursion => "Evaluation recurses too deep",
            }
        )
    }
}

impl std::error::Error for LibsensorsError {}

#[derive(Copy, Clone, Debug)]
pub struct Sensors {
    marker: PhantomData<()>,
}

#[derive(Copy, Clone, Debug)]
pub struct BusId {
    bus_type: i16,
    nr: i16,
}

#[derive(Debug)]
pub struct Chip {
    inner: *const libsensors::sensors_chip_name,
    prefix: String,
    bus: BusId,
    addr: i32,
    path: PathBuf,
}

pub struct ChipIterator {
    chip_name: Option<libsensors::sensors_chip_name>,
    index: i32,
}

/// Data about a single chip feature (or category leader)
#[derive(Debug)]
pub struct Feature {
    inner: *const libsensors::sensors_feature,
    chip_ptr: *const libsensors::sensors_chip_name,
    name: String,
    number: i32,
    feature_type: FeatureType,
}

pub struct FeatureIterator {
    chip_ptr: *const libsensors::sensors_chip_name,
    index: i32,
}

#[derive(Debug)]
pub struct Subfeature {
    inner: *const libsensors::sensors_subfeature,
    chip_ptr: *const libsensors::sensors_chip_name,
    name: String,
    number: i32,
    subfeature_type: SubfeatureType,
    mapping: i32,
    flags: u32,
}

pub struct SubfeatureIterator {
    chip_ptr: *const libsensors::sensors_chip_name,
    feature_ptr: *const libsensors::sensors_feature,
    index: i32,
}

impl Sensors {
    pub fn new() -> Self {
        INIT.call_once(|| unsafe {
            assert_eq!(libsensors::sensors_init(std::ptr::null_mut()), 0);
            assert_eq!(libc::atexit(Self::cleanup), 0);
        });

        Sensors {
            marker: PhantomData,
        }
    }

    extern "C" fn cleanup() {
        unsafe {
            libsensors::sensors_cleanup();
        }
    }

    /// Returns an iterator over all detected chips that match a given chip name
    pub fn detected_chips<S: AsRef<str>>(&self, name: S) -> Result<ChipIterator, LibsensorsError> {
        let c_name = std::ffi::CString::new(name.as_ref()).unwrap();
        let mut chip_name = libsensors::sensors_chip_name {
            prefix: std::ptr::null_mut(),
            bus: libsensors::sensors_bus_id {
                type_: Default::default(),
                nr: Default::default(),
            },
            addr: Default::default(),
            path: std::ptr::null_mut(),
        };

        let res = unsafe { libsensors::sensors_parse_chip_name(c_name.as_ptr(), &mut chip_name) };
        if res == 0 {
            let iterator = ChipIterator {
                chip_name: Some(chip_name),
                index: 0,
            };

            Ok(iterator)
        } else {
            Err(LibsensorsError::from_i32(res))
        }
    }
}

impl BusId {
    pub fn bus_type(&self) -> i16 {
        self.bus_type
    }

    pub fn nr(&self) -> i16 {
        self.nr
    }

    /// Return the adapter name of the bus.
    /// If it could not be found, it returns None
    pub fn get_adapter_name(&self) -> Option<String> {
        let bus_id = libsensors::sensors_bus_id {
            type_: self.bus_type,
            nr: self.nr,
        };
        let cstr_ptr = unsafe { libsensors::sensors_get_adapter_name(&bus_id) };
        if !cstr_ptr.is_null() {
            let cstr = unsafe { CStr::from_ptr(cstr_ptr) };
            Some(cstr.to_string_lossy().into_owned())
        } else {
            None
        }
    }
}

impl Chip {
    unsafe fn from_ptr(ptr: *const libsensors::sensors_chip_name) -> Chip {
        let chip = *ptr;
        let prefix_cstr = CStr::from_ptr(chip.prefix);
        let path_cstr = CStr::from_ptr(chip.path);

        Chip {
            inner: ptr,
            prefix: prefix_cstr.to_string_lossy().into_owned(),
            bus: BusId {
                bus_type: chip.bus.type_,
                nr: chip.bus.nr,
            },
            addr: chip.addr,
            path: PathBuf::from(path_cstr.to_string_lossy().into_owned()),
        }
    }

    fn c_ptr(&self) -> *const libsensors::sensors_chip_name {
        self.inner
    }

    pub fn prefix(&self) -> &str {
        self.prefix.as_str()
    }

    pub fn address(&self) -> i32 {
        self.addr
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn bus(&self) -> &BusId {
        &self.bus
    }

    /// Return the chip name from its internal representation.
    pub fn get_name(&self) -> Result<String, LibsensorsError> {
        let mut buffer: [std::os::raw::c_char; 128] = [0; 128];
        let res = unsafe {
            libsensors::sensors_snprintf_chip_name(&mut buffer[0], buffer.len(), self.c_ptr())
        };
        if res >= 0 {
            let name_cstr = unsafe { CStr::from_ptr(&buffer[0]) };
            Ok(name_cstr.to_string_lossy().into_owned())
        } else {
            Err(LibsensorsError::from_i32(res))
        }
    }
}

impl Feature {
    unsafe fn from_ptr(
        ptr: *const libsensors::sensors_feature,
        chip: *const libsensors::sensors_chip_name,
    ) -> Feature {
        let feature = *ptr;
        let name_cstr = CStr::from_ptr(feature.name);

        Feature {
            inner: ptr,
            chip_ptr: chip,
            name: name_cstr.to_string_lossy().into_owned(),
            number: feature.number,
            feature_type: feature.type_,
        }
    }

    fn c_ptr(&self) -> *const libsensors::sensors_feature {
        self.inner
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn number(&self) -> i32 {
        self.number
    }

    pub fn feature_type(&self) -> &FeatureType {
        &self.feature_type
    }

    /// Look up the label of the feature.
    /// If no label exists for this feature, its name is returned itself.
    pub fn get_label(&self) -> Result<String, LibsensorsError> {
        let label_ptr = unsafe { libsensors::sensors_get_label(self.chip_ptr, self.c_ptr()) };
        if !label_ptr.is_null() {
            let label = unsafe { CStr::from_ptr(label_ptr).to_string_lossy().into_owned() };
            unsafe {
                libc::free(label_ptr as *mut libc::c_void);
            }
            Ok(label)
        } else {
            Err(LibsensorsError::Unknown)
        }
    }

    /// Returns the subfeature of the given type,
    /// if it exists, None otherwise.
    pub fn get_subfeature(&self, subfeature_type: SubfeatureType) -> Option<Subfeature> {
        let ptr = unsafe {
            libsensors::sensors_get_subfeature(self.chip_ptr, self.c_ptr(), subfeature_type)
        };

        if !ptr.is_null() {
            unsafe { Some(Subfeature::from_ptr(ptr, self.chip_ptr)) }
        } else {
            None
        }
    }
}

impl Subfeature {
    unsafe fn from_ptr(
        ptr: *const libsensors::sensors_subfeature,
        chip: *const libsensors::sensors_chip_name,
    ) -> Subfeature {
        let subfeature = *ptr;
        let name_cstr = CStr::from_ptr(subfeature.name);

        Subfeature {
            inner: ptr,
            chip_ptr: chip,
            name: name_cstr.to_string_lossy().into_owned(),
            number: subfeature.number,
            subfeature_type: subfeature.type_,
            mapping: subfeature.mapping,
            flags: subfeature.flags,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn subfeature_type(&self) -> &SubfeatureType {
        &self.subfeature_type
    }

    /// Read the value of the subfeature.
    pub fn get_value(&self) -> Result<f64, LibsensorsError> {
        let mut value: f64 = 0.0;
        let res = unsafe { libsensors::sensors_get_value(self.chip_ptr, self.number, &mut value) };
        if res >= 0 {
            Ok(value)
        } else {
            Err(LibsensorsError::from_i32(res))
        }
    }

    /// Set the value of the subfeature.
    pub fn set_value(&self, value: f64) -> Result<(), LibsensorsError> {
        let res = unsafe { libsensors::sensors_set_value(self.chip_ptr, self.number, value) };
        if res >= 0 {
            Ok(())
        } else {
            Err(LibsensorsError::from_i32(res))
        }
    }
}

impl IntoIterator for Sensors {
    type Item = Chip;
    type IntoIter = ChipIterator;

    fn into_iter(self) -> Self::IntoIter {
        ChipIterator {
            chip_name: None,
            index: 0,
        }
    }
}

impl Iterator for ChipIterator {
    type Item = Chip;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = unsafe {
            libsensors::sensors_get_detected_chips(
                self.chip_name.as_ref().map_or(std::ptr::null(), |x| x),
                &mut self.index,
            )
        };

        if !ptr.is_null() {
            unsafe { Some(Chip::from_ptr(ptr)) }
        } else {
            None
        }
    }
}

impl Drop for ChipIterator {
    fn drop(&mut self) {
        if let Some(mut chip_name) = self.chip_name {
            unsafe {
                libsensors::sensors_free_chip_name(&mut chip_name);
            }
        };
    }
}

impl IntoIterator for Chip {
    type Item = Feature;
    type IntoIter = FeatureIterator;

    fn into_iter(self) -> Self::IntoIter {
        FeatureIterator {
            index: 0,
            chip_ptr: self.c_ptr(),
        }
    }
}

impl Iterator for FeatureIterator {
    type Item = Feature;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = unsafe { libsensors::sensors_get_features(self.chip_ptr, &mut self.index) };

        if !ptr.is_null() && !self.chip_ptr.is_null() {
            unsafe { Some(Feature::from_ptr(ptr, self.chip_ptr)) }
        } else {
            None
        }
    }
}

impl IntoIterator for Feature {
    type Item = Subfeature;
    type IntoIter = SubfeatureIterator;

    fn into_iter(self) -> Self::IntoIter {
        SubfeatureIterator {
            index: 0,
            chip_ptr: self.chip_ptr,
            feature_ptr: self.c_ptr(),
        }
    }
}

impl Iterator for SubfeatureIterator {
    type Item = Subfeature;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = unsafe {
            libsensors::sensors_get_all_subfeatures(
                self.chip_ptr,
                self.feature_ptr,
                &mut self.index,
            )
        };

        if !ptr.is_null() {
            unsafe { Some(Subfeature::from_ptr(ptr, self.chip_ptr)) }
        } else {
            None
        }
    }
}
