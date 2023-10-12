#![warn(missing_docs)]
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use std::path::Path;
use std::{thread, time::Duration, time::Instant};
#[allow(unused_imports)]
use log::{trace, debug, info, warn, error};

use regex::Regex;

use opencv::{
    core::{self},
    prelude::*,
    imgcodecs,
    imgproc,
};

use enigo::*;

use core_foundation::base::{CFGetTypeID, CFTypeID, ToVoid};
use core_foundation::string::{
    kCFStringEncodingUTF8, CFString, CFStringGetCStringPtr, CFStringGetTypeID,
};
use core_foundation::number::{
    CFBooleanGetTypeID, CFNumberGetTypeID, CFNumberGetValue, CFNumberRef,
    kCFNumberSInt32Type, kCFNumberSInt64Type, kCFNumberFloat32Type, kCFNumberFloat64Type,
    CFBooleanGetValue, CFNumberGetType,
};
use core_foundation::dictionary::{CFDictionaryGetTypeID};
use core_graphics::display::*;
use std::ffi::CStr;
use std::ops::Deref;
use std::os::raw::c_void;

pub mod error;

const DEFAULT_HIGH_DPI_RATIO: u32 = 2; // For standard DPI screen: 1, for Retina-like: 2
const DEFAULT_WAIT_TIME: Duration = Duration::from_millis(90); // delay between mouse move and mouse down and up
const DEFAULT_CAPTURE_FREQUENCY: f32 = 3.0; // xx captures per second

type Result<T> = std::result::Result<T, error::Error>;

#[derive(Debug)]
enum DictEntryValue {
    _Number(i64),
    _Float(f64),
    _Bool(bool),
    _String(String),
    _DictRef(CFDictionaryRef),
    _Unknown,
}

#[derive(Debug)]
/// The `WindowList` struct represents a list of windows.
pub struct WindowList(Vec<Window>);

impl WindowList {
    /// Creates a new `WindowList`.
    pub fn new() -> WindowList {
        WindowList(WindowList::_window_list().unwrap())
    }

    // From https://github.com/sassman/t-rec-rs/blob/39e7560f06055f15dc4078ea1e65db48b135669a/src/macos/window_id.rs
    // hard nut to crack, some starting point was:
    // https://stackoverflow.com/questions/60117318/getting-window-owner-names-via-cgwindowlistcopywindowinfo-in-rust
    // then some more PRs where needed:
    // https://github.com/servo/core-foundation-rs/pulls?q=is%3Apr+author%3Asassman+
    fn _window_list() -> Result<Vec<Window>> {
        let mut win_list: Vec<Window> = vec![];
        let window_list_info = unsafe {
            CGWindowListCopyWindowInfo(
                kCGWindowListOptionIncludingWindow
                    | kCGWindowListOptionOnScreenOnly
                    | kCGWindowListExcludeDesktopElements,
                kCGNullWindowID,
            )
        };
        if window_list_info.is_null() {
            return Err(error::Error { kind: error::ErrorKind::CoreFoundation, message: "Cannot get window list results from low level C-API call `CGWindowListCopyWindowInfo` -> null".into() });
        }

        let count = unsafe { CFArrayGetCount(window_list_info) };
        for i in 0..count {
            let dic_ref =
                unsafe { CFArrayGetValueAtIndex(window_list_info, i as isize) as CFDictionaryRef };
            if dic_ref.is_null() {
                unsafe {
                    CFRelease(window_list_info.cast());
                }
                return Err(error::Error { kind: error::ErrorKind::CoreFoundation, message: "Cannot get a result from the window list from low level C-API call `CFArrayGetValueAtIndex` -> null".into() });
            }
            let window_name = get_from_dict(dic_ref, "kCGWindowName");
            let window_owner = get_from_dict(dic_ref, "kCGWindowOwnerName");
            let window_id = get_from_dict(dic_ref, "kCGWindowNumber");
            let window_bounds = get_from_dict(dic_ref, "kCGWindowBounds");
            if let (DictEntryValue::_String(win_name), DictEntryValue::_String(win_owner), DictEntryValue::_Number(win_id)) =
                (window_name, window_owner, window_id)
            {
                let mut w = Window{ name: win_name, owner_name: win_owner, id: win_id, bounds: None, capture_frequency: DEFAULT_CAPTURE_FREQUENCY };
                if let DictEntryValue::_DictRef(b_dic_ref) = window_bounds {
                    let b_height = get_from_dict(b_dic_ref, "Height");
                    let b_width = get_from_dict(b_dic_ref, "Width");
                    let b_x = get_from_dict(b_dic_ref, "X");
                    let b_y = get_from_dict(b_dic_ref, "Y");
                    if let (DictEntryValue::_Float(win_height), DictEntryValue::_Float(win_width), DictEntryValue::_Float(win_x), DictEntryValue::_Float(win_y)) =
                        (b_height, b_width, b_x, b_y)
                    {
                        w.bounds = Some(Bounds { x: win_x, y: win_y, width: win_width, height: win_height });
                        trace!("Window bounds {}, {}, size {} x {}, ", win_x, win_y, win_height, win_width);
                    }
                }
                win_list.push(w);
            }
        }

        unsafe {
            CFRelease(window_list_info.cast());
        }

        Ok(win_list)
    }

    /// Returns a formatted string representing the list of windows.
    pub fn prettify(&self) -> String {
        let max_width = 30;
        let mut table: String = format!("{:<6} {:<width$} {:<width$}\n", "Id", "Window Name", "Window Owner Name", width = max_width);
        table.push_str(&format!("{}\n","-".repeat(6+max_width*2)));
        for w in &self.0 {
            let name: String = if w.name.len() > max_width { format!("{}...",&w.name[..max_width-3]) } else { w.name.clone() };
            let owner = w.owner_name.clone();
            table.push_str(&format!("{:<6} {:<width$} {:<width$}\n", w.id, name, owner, width = max_width));
        }
        return table;
    }
}

/// Structure representing a rectangle zone in the window.
#[derive(Debug)]
pub struct Rect {
    /// Left coordinate of the rectangle, relative to x-axis of the window.
    pub x: u32,
    /// Top coordinate of the rectangle, relative to y-axis of the window.
    pub y: u32,
    /// Width of the rectangle.
    pub width: u32,
    /// Height of the rectangle.
    pub height: u32,
}

impl Rect {
    /// Creates a new `Rect`.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Rect {
            x: x,
            y: y,
            width: width,
            height: height,
        }
    }

    /// Returns the coordinates of the center of the rectangle.
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

/// Structure representing the absolute coordinates of a window.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
/// The `Window` struct represents a single window.
#[derive(Clone, Debug)]
pub struct Window {
    id: i64,
    name: String,
    owner_name: String,
    bounds: Option<Bounds>,
    capture_frequency: f32
}

impl Window {
    /// Captures a screenshot of the window and saves it to the specified file.
    pub fn screenshot(&self, file: &Path) -> Result<()> {
        let (width, height, mut raw_v) = self._capture()?;
        debug!("w={}, h={}", width, height);

        // Convert to gray CV image
        let mut result = Mat::default();
        Window::_vec_to_cvmat(&mut raw_v, width as i32, height as i32, &mut result, imgproc::COLOR_BGRA2GRAY)?;

        // Save file
        imgcodecs::imwrite(file.to_str().unwrap(), &mut result, &core::Vector::new())?;
        Ok(())
    }

    /// Sets the number of captures per seconds.
    pub fn set_capture_frequency(&mut self, value: f32) {
        self.capture_frequency = value;
    }

    fn _vec_to_cvmat(vec: &mut Vec<u8>, width: i32, height: i32, dest: &mut Mat, color_conv: i32) -> Result<()> {
        // generate a Mat from raw data image
        let mat = unsafe {
            core::Mat::new_rows_cols_with_data(
                height,
                width,
                core::CV_8UC4, //u8::typ(),
                vec.as_mut_ptr() as *mut c_void,
                core::Mat_AUTO_STEP,
                )?
        };
        // Convert to gray
        imgproc::cvt_color(&mat, dest, color_conv, 0)?;
        Ok(())
    }

    fn _capture(&self) -> Result<(u32, u32, Vec<u8>)> {
//TODO: check i32 to u32 cast before
        let img = unsafe {
            CGDisplay::screenshot(
                CGRectNull,
                kCGWindowListOptionIncludingWindow | kCGWindowListExcludeDesktopElements,
                self.id as u32,
                kCGWindowImageBestResolution
                    | kCGWindowImageBoundsIgnoreFraming
                    | kCGWindowImageShouldBeOpaque,
            ).unwrap()
        };

        let cfdata = img.data();
        let v = cfdata.bytes().to_vec();

        trace!("img {} x {}", img.width(), img.height());
        trace!("img bits_per_component {}", img.bits_per_component());
        trace!("img bits_per_pixel {}", img.bits_per_pixel());
        trace!("img bytes_per_row {}", img.bytes_per_row());

        if img.bytes_per_row() * img.height() != v.len() {
            panic!("Cannot grab screenshot from CGDisplay of window id {}", self.id);
        }

//TODO: check i32 to u32 cast before
        let bytes_per_pixel = (img.bits_per_pixel() / img.bits_per_component()) as u32;
        let w = img.bytes_per_row() as u32 / bytes_per_pixel;
        let h = img.height() as u32;
        // The bytes per row (also called the “stride”) can be larger than the width of the image.
        // The extra bytes at the end of each row are simply ignored.
        // https://stackoverflow.com/a/25706554

        Ok((w, h, v))
    }

    /// Attempts to find the specified template image within the window.
    fn find(&self, tpl_file: &Path, time_out: Duration) -> Result<Rect> {
        let sleep_d = Duration::from_millis((1f32 / self.capture_frequency * 1000f32) as u64);
        trace!("Sleep time set to {}\"{}", sleep_d.as_secs(), sleep_d.subsec_millis());

        if ! time_out.is_zero() && time_out.checked_sub(sleep_d).is_none() {
            warn!("Time-out is too low ({} ms) for the capture period ({} ms)", time_out.as_millis(), sleep_d.as_millis());
        }

        let cv_template = imgcodecs::imread(&tpl_file.to_str().unwrap(), imgcodecs::IMREAD_GRAYSCALE)?;
        trace!("template = {:#?}", cv_template);
//TODO: template caching

        let start = Instant::now();
        let mut count = 0u32;
        loop {
            count += 1;
            trace!("Loop n°{}", count);

            // Take screenshot
            let (width, height, mut raw_v) = self._capture()?;
            // Convert to gray OpenCV image
            let mut cv_screenshot = Mat::default();
            Window::_vec_to_cvmat(&mut raw_v, width as i32, height as i32, &mut cv_screenshot, imgproc::COLOR_BGRA2GRAY)?;

            // Construct the result matrix, a single-channel 32-bit floating-point.
            // If image is W x H and template is w x h, then result is (W - w + 1) x (H - h + 1)
            let zero = core::Mat::zeros(
                width as i32 - cv_template.rows() + 1,
                height as i32 - cv_template.cols() + 1,
                core::CV_32FC1,
            )
            .unwrap();
            let mut result = zero.to_mat().unwrap();

            // Optional
            // Only two matching methods currently accept a mask: TM_SQDIFF and TM_CCORR_NORMED
            let mask = Mat::default();

            imgproc::match_template(&cv_screenshot, &cv_template, &mut result, imgproc::TM_CCOEFF_NORMED, &mask)?;

            // Find the location of the best match
            let mut min_val: f64 = 0.0;
            let mut max_val: f64 = 0.0;
            let mut min_loc: core::Point = core::Point::new(0,0);
            let mut max_loc: core::Point = core::Point::new(0,0);
            core::min_max_loc(&result, Some(&mut min_val), Some(&mut max_val), Some(&mut min_loc), Some(&mut max_loc), &mask)?;
            let threshold = 0.8; // with TM_SQDIFF_NORMED you could use 0.1
            if max_val > threshold {
    //TODO: check i32 to u32 cast before
                let rect = Rect::new(max_loc.x as u32, max_loc.y as u32, cv_template.cols() as u32, cv_template.rows() as u32); // with TM_SQDIFF_NORMED use min_loc

                return Ok(rect);
            }

            // loop until time-out
            thread::sleep(sleep_d);
            let elapsed = start.elapsed();
            if elapsed > time_out {
                debug!("Timed-out after {}\"{}", elapsed.as_secs(), elapsed.subsec_millis());
                //return Err(opencv::Error { code: core::StsError, message: "No match found!".to_string() }.into() );
                return Err(error::Error { kind: error::ErrorKind::ImageNotFound, message: format!("Template {} not found", tpl_file.display()) }.into() );
            }
        }
    }
}

fn get_from_dict(dict: CFDictionaryRef, key: &str) -> DictEntryValue {
    let key: CFString = key.into();
    let mut value: *const c_void = std::ptr::null();
    if unsafe { CFDictionaryGetValueIfPresent(dict, key.to_void(), &mut value) != 0 } {
        let type_id: CFTypeID = unsafe { CFGetTypeID(value) };
        trace!("key: {:#?} type: {:#?}", key, type_id);
        if type_id == unsafe { CFNumberGetTypeID() } {
            let value = value as CFNumberRef;
            #[allow(non_upper_case_globals)]
            match unsafe { CFNumberGetType(value) } {
                kCFNumberSInt64Type => {
                    trace!("key: {:#?} num type (i64): {:#?}", key, kCFNumberSInt64Type);
                    let mut value_i64 = 0_i64;
                    let out_value: *mut i64 = &mut value_i64;
                    let converted = unsafe { CFNumberGetValue(value, kCFNumberSInt64Type, out_value.cast()) };
                    if converted {
                        return DictEntryValue::_Number(value_i64);
                    }
                }
                kCFNumberSInt32Type => {
                    trace!("key: {:#?} num type (i32): {:#?}", key, kCFNumberSInt32Type);
                    let mut value_i32 = 0_i32;
                    let out_value: *mut i32 = &mut value_i32;
                    let converted = unsafe { CFNumberGetValue(value, kCFNumberSInt32Type, out_value.cast()) };
                    if converted {
                        return DictEntryValue::_Number(value_i32 as i64);
                    }
                }
                kCFNumberFloat64Type => {
                    trace!("key: {:#?} num type (f64): {:#?}", key, kCFNumberFloat64Type);
                    let mut value_f64 = 0_f64;
                    let out_value: *mut f64 = &mut value_f64;
                    let converted = unsafe { CFNumberGetValue(value, kCFNumberFloat64Type, out_value.cast()) };
                    if converted {
                        return DictEntryValue::_Float(value_f64);
                    }
                }
                kCFNumberFloat32Type => {
                    trace!("key: {:#?} num type (f32): {:#?}", key, kCFNumberFloat32Type);
                    let mut value_f32 = 0_f32;
                    let out_value: *mut f32 = &mut value_f32;
                    let converted = unsafe { CFNumberGetValue(value, kCFNumberFloat32Type, out_value.cast()) };
                    if converted {
                        return DictEntryValue::_Float(value_f32 as f64);
                    }
                }
                n => {
                    warn!("Unsupported Number of typeId: {}", n);
                }
            }
        } else if type_id == unsafe { CFBooleanGetTypeID() } {
            return DictEntryValue::_Bool(unsafe { CFBooleanGetValue(value.cast()) });
        } else if type_id == unsafe { CFDictionaryGetTypeID() } {
            return DictEntryValue::_DictRef(value as CFDictionaryRef);
            //let window_height = get_from_dict(value as CFDictionaryRef, "Height");
            //trace!("Height={:#?}", window_height);
        } else if type_id == unsafe { CFStringGetTypeID() } {
            let c_ptr = unsafe { CFStringGetCStringPtr(value.cast(), kCFStringEncodingUTF8) };
            return if !c_ptr.is_null() {
                let c_result = unsafe { CStr::from_ptr(c_ptr) };
                let result = String::from(c_result.to_str().unwrap());
                DictEntryValue::_String(result)
            } else {
                // in this case there is a high chance we got a `NSString` instead of `CFString`
                // we have to use the objc runtime to fetch it
                use objc_foundation::{INSString, NSString};
                use objc_id::Id;
                let nss: Id<NSString> = unsafe { Id::from_ptr(value as *mut NSString) };
                let str = std::str::from_utf8(nss.deref().as_str().as_bytes());

                match str {
                    Ok(s) => DictEntryValue::_String(s.to_owned()),
                    Err(_) => DictEntryValue::_Unknown,
                }
            };
        } else {
            warn!("Unexpected type: {}", type_id);
        }
    }

    DictEntryValue::_Unknown
}

#[derive(Debug)]
/// The `Bot` struct provides automation capabilities for interacting with a window.
pub struct Bot {
    /// The `Window` that the `Bot` interacts with.
    pub window: Option<Window>,
    controller: Option<Enigo>,
    high_dpi_ratio: u32,
    wait_time: Duration,
    capture_frequency: f32
}

impl Bot {
    /// Creates a new instance of `Bot`.
    pub fn new() -> Bot {
        Bot {
            window: None,
            controller: None,
            high_dpi_ratio: DEFAULT_HIGH_DPI_RATIO,
            wait_time: DEFAULT_WAIT_TIME,
            capture_frequency: DEFAULT_CAPTURE_FREQUENCY
        }
    }

    /// Sets the window based on the specified name.
    pub fn set_window_from_name(&mut self, name: &str) {
        for w in WindowList::new().0.iter() {
            if w.name.eq(name) {
                let mut nw = w.clone();
                nw.set_capture_frequency(self.capture_frequency);
                self.window = Some(nw);
            }
        }
    }

    /// Sets the window based on the specified regex.
    pub fn set_window_from_regex(&mut self, regex: &str) {
        let re = Regex::new(regex).unwrap();
        for w in WindowList::new().0.iter() {
            if re.is_match(&w.name) {
                let mut nw = w.clone();
                nw.set_capture_frequency(self.capture_frequency);
                self.window = Some(nw);
            }
        }
    }

    /// Sets the window based on the specified id.
    pub fn set_window_from_id(&mut self, id: i64) {
        for w in WindowList::new().0.iter() {
            if w.id == id {
                let mut nw = w.clone();
                nw.set_capture_frequency(self.capture_frequency);
                self.window = Some(nw);
            }
        }
    }

    /// Sets the Enigo controller.
    pub fn set_controller(&mut self, controller: Enigo) {
        self.controller = Some(controller);
    }

    /// Sets High DPI mode (for standard screen: 1, for Retina-like: 2).
    pub fn set_high_dpi_ratio(&mut self, ratio: u32) {
        self.high_dpi_ratio = ratio;
    }

    /// Sets the delay between mouse move and mouse down and up.
    pub fn set_wait_time(&mut self, duration: Duration) {
        self.wait_time = duration;
    }

    /// Sets the number of captures per seconds.
    pub fn set_capture_frequency(&mut self, value: f32) {
        self.capture_frequency = value;
    }

    /// Waits for the specified duration in milliseconds.
    pub fn sleep(&mut self, millis: u64) {
        thread::sleep(Duration::from_millis(millis));
    }

    /// Clicks the mouse button at the specified coordinates relative to the window.
    pub fn click(&mut self, relative_x: u32, relative_y: u32) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
//TODO: check cast or change bounds fields type
        let window_x = self.window.as_ref().unwrap().bounds.as_ref().unwrap().x as u32;
        let window_y = self.window.as_ref().unwrap().bounds.as_ref().unwrap().y as u32;
        let screen_x = relative_x / self.high_dpi_ratio + window_x;
        trace!("screen x = {} / {} + {}", relative_x, self.high_dpi_ratio, window_x);
        let screen_y = relative_y / self.high_dpi_ratio + window_y;
        trace!("screen y = {} / {} + {}", relative_y, self.high_dpi_ratio, window_y);
        debug!("Click on: {}, {}", screen_x, screen_y);

        // move pointer
//TODO: check cast
        controller.mouse_move_to(screen_x as i32, screen_y as i32);
        thread::sleep(self.wait_time);
        // click
        controller.mouse_down(MouseButton::Left);
        thread::sleep(self.wait_time);
        controller.mouse_up(MouseButton::Left);
        Ok(())
    }

    /// Pushes down the mouse button at the specified coordinates relative to the window.
    pub fn mouse_down_on(&mut self, relative_x: u32, relative_y: u32) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
//TODO: check cast or change bounds fields type
        let window_x = self.window.as_ref().unwrap().bounds.as_ref().unwrap().x as u32;
        let window_y = self.window.as_ref().unwrap().bounds.as_ref().unwrap().y as u32;
        let screen_x = relative_x / self.high_dpi_ratio + window_x;
        trace!("screen x = {} / {} + {}", relative_x, self.high_dpi_ratio, window_x);
        let screen_y = relative_y / self.high_dpi_ratio + window_y;
        trace!("screen y = {} / {} + {}", relative_y, self.high_dpi_ratio, window_y);
        debug!("Mouse down on: {}, {}", screen_x, screen_y);

//TODO: check cast
        controller.mouse_move_to(screen_x as i32, screen_y as i32);
        thread::sleep(self.wait_time);
        controller.mouse_down(MouseButton::Left);
        Ok(())
    }

    /// Releases the mouse button at the specified coordinates relative to the window.
    pub fn mouse_up_on(&mut self, relative_x: u32, relative_y: u32) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
//TODO: check cast or change bounds fields type
        let window_x = self.window.as_ref().unwrap().bounds.as_ref().unwrap().x as u32;
        let window_y = self.window.as_ref().unwrap().bounds.as_ref().unwrap().y as u32;
        let screen_x = relative_x / self.high_dpi_ratio + window_x;
        trace!("screen x = {} / {} + {}", relative_x, self.high_dpi_ratio, window_x);
        let screen_y = relative_y / self.high_dpi_ratio + window_y;
        trace!("screen y = {} / {} + {}", relative_y, self.high_dpi_ratio, window_y);
        debug!("Mouse up on: {}, {}", screen_x, screen_y);

//TODO: check cast
        controller.mouse_move_to(screen_x as i32, screen_y as i32);
        thread::sleep(self.wait_time);
        controller.mouse_up(MouseButton::Left);
        Ok(())
    }

    /// Clicks at the top of the bottom to activate the window.
    pub fn activate_window(&mut self) -> Result<()> {
        // click on the middle of the title bar to activate the window
        debug!("Activating window");
//TODO: check cast or change bounds fields type
        let window_width = self.window.as_ref().unwrap().bounds.as_ref().unwrap().width as u32;
        self.click(window_width, 20)?;
        Ok(())
    }

    /// Searches for a a specified image within the window and returns the `Rect` coordinates.
    pub fn find(&mut self, template: &Path) -> Result<Rect> {
        let rect = self.window.as_ref().unwrap().find(template, Duration::ZERO)?;
        debug!("found: {:?}", rect);
        Ok(rect)
    }

    /// Searches for a specified image within the window and clicks at its center.
    pub fn click_on_image(&mut self, template: &Path, time_out: u64) -> Result<(u32, u32)> {
        debug!("Searching {}", template.display());
        let rect = self.window.as_ref().unwrap().find(template, Duration::from_millis(time_out))?;
        debug!("Image found on: {:?}", rect);
        let (x, y) = rect.center();
        self.click(x, y)?;
        Ok((x, y))
    }

    /// Presses down the given key.
    pub fn key_down(&mut self, key: Key) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
        debug!("Key down: {:#?}", key);
        controller.key_down(key);
        Ok(())
    }

    /// Releases the given key.
    pub fn key_up(&mut self, key: Key) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
        debug!("Key up: {:#?}", key);
        controller.key_up(key);
        Ok(())
    }

    /// Presses and release the key.
    pub fn key_click(&mut self, key: Key) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
        debug!("Key click: {:#?}", key);
        controller.key_click(key);
        Ok(())
    }

    /// Types a string.
    pub fn key_sequence(&mut self, text: &str) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
        debug!("Typing: {}", text);
        controller.key_sequence(text);
        Ok(())
    }

    /// Types a string (alias to `key_sequence`).
    pub fn write(&mut self, text: &str) -> Result<()> {
        self.key_sequence(text)
    }

    /// Types a string followed by return.
    pub fn writeln(&mut self, text: &str) -> Result<()> {
        let controller = self.controller.as_mut().unwrap();
        debug!("Typing: {}", text);
        controller.key_sequence(text);
        debug!("Pressing enter");
        controller.key_click(Key::Return);
        Ok(())
    }
}
