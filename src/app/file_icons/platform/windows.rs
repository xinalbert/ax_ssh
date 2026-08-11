use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{
    SHFILEINFOW, SHGFI_ICON, SHGFI_LINKOVERLAY, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    SHGetFileInfoW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};

use super::*;

#[derive(Default)]
pub(in crate::app::file_icons) struct Resolver {
    _private: (),
}

impl IconResolver for Resolver {
    fn cache_identity(&self) -> CacheIdentity {
        CacheIdentity::new("windows-shell-small-icon-v1")
    }

    fn resolve(&self, key: &FileIconKey) -> Option<FileIcon> {
        let _com = ComGuard::initialize()?;
        let (name, attributes, extra_flags) = match key {
            FileIconKey::Folder => ("folder".to_owned(), FILE_ATTRIBUTE_DIRECTORY, 0),
            FileIconKey::Symlink => (
                "remote-file".to_owned(),
                FILE_ATTRIBUTE_NORMAL,
                SHGFI_LINKOVERLAY,
            ),
            FileIconKey::GenericFile => ("remote-file".to_owned(), FILE_ATTRIBUTE_NORMAL, 0),
            FileIconKey::Extension(extension) => {
                (format!("remote-file.{extension}"), FILE_ATTRIBUTE_NORMAL, 0)
            }
        };
        let wide_name = name.encode_utf16().chain([0]).collect::<Vec<_>>();
        let mut info = SHFILEINFOW::default();
        // SAFETY: `wide_name` is NUL-terminated, `info` is writable for its declared size,
        // and SHGFI_USEFILEATTRIBUTES ensures the synthetic path is not accessed.
        let result = unsafe {
            SHGetFileInfoW(
                wide_name.as_ptr(),
                attributes,
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES | extra_flags,
            )
        };
        if result == 0 || info.hIcon.is_null() {
            return None;
        }
        let icon = OwnedIcon(info.hIcon);
        icon_to_rgba(icon.0)
    }
}

struct ComGuard {
    uninitialize: bool,
}

impl ComGuard {
    fn initialize() -> Option<Self> {
        // SAFETY: this call only changes COM state for the current blocking worker thread; a
        // null reserved pointer and the MTA flag are the documented initialization contract.
        let result = unsafe { CoInitializeEx(null_mut(), COINIT_MULTITHREADED as u32) };
        if result >= 0 {
            Some(Self { uninitialize: true })
        } else if result == RPC_E_CHANGED_MODE {
            // The thread is already initialized under another COM apartment model.
            // Do not call CoUninitialize because this call did not increment its COM count.
            Some(Self {
                uninitialize: false,
            })
        } else {
            None
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: `uninitialize` is true only when this guard successfully incremented the
            // current thread's COM initialization count.
            unsafe { CoUninitialize() };
        }
    }
}

struct OwnedIcon(HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the HICON returned by SHGetFileInfoW.
        unsafe { DestroyIcon(self.0) };
    }
}

struct MemoryDc(HDC);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the memory DC returned by CreateCompatibleDC.
        unsafe { DeleteDC(self.0) };
    }
}

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: this wrapper uniquely owns the HBITMAP returned by CreateDIBSection.
        unsafe { DeleteObject(self.0) };
    }
}

fn select_failed(object: HGDIOBJ) -> bool {
    object.is_null() || object == (-1_isize as HGDIOBJ)
}

fn icon_to_rgba(icon: HICON) -> Option<FileIcon> {
    // SAFETY: a null compatible HDC requests a memory DC compatible with the screen.
    let dc = MemoryDc(unsafe { CreateCompatibleDC(null_mut()) });
    if dc.0.is_null() {
        return None;
    }

    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: FILE_ICON_EDGE as i32,
            biHeight: -(FILE_ICON_EDGE as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: FILE_ICON_BYTE_LEN as u32,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits: *mut c_void = null_mut();
    // SAFETY: `bitmap_info` describes a bounded 24x24 32-bit DIB and `bits` is a valid out
    // pointer. No file mapping is supplied, so GDI allocates the backing storage.
    let bitmap = OwnedBitmap(unsafe {
        CreateDIBSection(dc.0, &bitmap_info, DIB_RGB_COLORS, &mut bits, null_mut(), 0)
    });
    if bitmap.0.is_null() || bits.is_null() {
        return None;
    }
    // SAFETY: the DIB allocation is exactly FILE_ICON_BYTE_LEN bytes and is exclusively owned
    // here. Clearing it gives DrawIconEx a deterministic transparent destination.
    unsafe { std::ptr::write_bytes(bits.cast::<u8>(), 0, FILE_ICON_BYTE_LEN) };

    // SAFETY: both handles are live GDI objects owned by this scope.
    let previous = unsafe { SelectObject(dc.0, bitmap.0 as HGDIOBJ) };
    if select_failed(previous) {
        return None;
    }
    // SAFETY: the selected DIB is exactly FILE_ICON_EDGE square and `icon` is live for the
    // duration of the call.
    let drawn = unsafe {
        DrawIconEx(
            dc.0,
            0,
            0,
            icon,
            FILE_ICON_EDGE as i32,
            FILE_ICON_EDGE as i32,
            0,
            null_mut(),
            DI_NORMAL,
        )
    };
    // SAFETY: `previous` is the object returned by SelectObject for this same DC.
    let restored = unsafe { SelectObject(dc.0, previous) };
    if select_failed(restored) {
        // A failed restore leaves ownership of the bitmap ambiguous: it may still be selected
        // into the DC. Leaking this tiny bounded GDI object is safer than allowing Drop to call
        // DeleteObject on a selected bitmap, which violates the GDI lifetime contract.
        std::mem::forget(bitmap);
        return None;
    }
    if drawn == 0 {
        return None;
    }

    // SAFETY: CreateDIBSection returned a buffer of FILE_ICON_BYTE_LEN bytes that remains
    // valid until `bitmap` is dropped below. It is copied before either GDI owner is released.
    let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), FILE_ICON_BYTE_LEN) };
    if !bgra.chunks_exact(4).any(|pixel| pixel[3] != 0) {
        return None;
    }
    let mut rgba = Vec::with_capacity(FILE_ICON_BYTE_LEN);
    for pixel in bgra.chunks_exact(4) {
        let alpha = pixel[3];
        rgba.push(unpremultiply(pixel[2], alpha));
        rgba.push(unpremultiply(pixel[1], alpha));
        rgba.push(unpremultiply(pixel[0], alpha));
        rgba.push(alpha);
    }
    FileIcon::from_rgba(FILE_ICON_EDGE, FILE_ICON_EDGE, rgba)
}

fn unpremultiply(channel: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    ((u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8
}
