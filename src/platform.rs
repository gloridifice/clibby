use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::history::Store;
#[cfg(target_os = "windows")]
use crate::history::{ClipEntry, new_group_id, record_path_reference, snapshot_text_to_history};

pub(crate) fn sync_system_clipboard(store: &Store) {
    #[cfg(not(target_os = "windows"))]
    let _ = store;

    #[cfg(target_os = "windows")]
    if let Err(error) = sync_windows_system_clipboard(store) {
        eprintln!("Warning: could not sync the system clipboard: {error:#}");
    }
}

#[cfg(target_os = "windows")]
fn sync_windows_system_clipboard(store: &Store) -> Result<()> {
    match system_clipboard::read_content()? {
        None => Ok(()),
        Some(system_clipboard::ClipboardContent::Files(paths)) => sync_system_files(store, paths),
        Some(system_clipboard::ClipboardContent::Text(text)) => sync_system_text(store, &text),
    }
}

#[cfg(target_os = "windows")]
fn sync_system_files(store: &Store, paths: Vec<PathBuf>) -> Result<()> {
    let paths = paths
        .into_iter()
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(());
    }

    let history = store.load_history()?;
    if system_paths_match_history(&history.entries, &paths) {
        return Ok(());
    }

    // Entries recorded from one clipboard selection share a group id so
    // `paste` can restore the whole selection instead of a single file.
    let group = new_group_id();
    for path in paths {
        if let Err(error) = record_path_reference(store, &path, Some(&group)) {
            eprintln!(
                "Warning: could not import system clipboard path {}: {error:#}",
                path.display()
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn sync_system_text(store: &Store, text: &str) -> Result<()> {
    let history = store.load_history()?;
    if history
        .entries
        .last()
        .and_then(|entry| entry.system_text.as_deref())
        == Some(text)
    {
        return Ok(());
    }

    snapshot_text_to_history(store, text)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn system_paths_match_history(history: &[ClipEntry], paths: &[PathBuf]) -> bool {
    if paths.len() > history.len() {
        return false;
    }

    let recent_entries = &history[history.len() - paths.len()..];
    recent_entries.iter().zip(paths).all(|(entry, path)| {
        entry
            .system_source
            .as_ref()
            .is_some_and(|source| source == path)
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn publish_system_clipboard(paths: &[PathBuf]) -> Result<()> {
    system_clipboard::write_file_list(paths)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn publish_system_clipboard(_paths: &[PathBuf]) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn publish_system_clipboard_text(text: &str) -> Result<()> {
    system_clipboard::write_unicode_text(text)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn publish_system_clipboard_text(_text: &str) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
mod system_clipboard {
    use std::{
        ffi::OsString,
        mem::size_of,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::PathBuf,
        ptr, thread,
        time::Duration,
    };

    use anyhow::{Context, Result, bail};
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
                OpenClipboard, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{CF_HDROP, CF_UNICODETEXT},
        },
        UI::Shell::{DROPFILES, DragQueryFileW, HDROP},
    };

    pub(super) enum ClipboardContent {
        Files(Vec<PathBuf>),
        Text(String),
    }

    pub(super) fn read_content() -> Result<Option<ClipboardContent>> {
        let _clipboard = ClipboardSession::open()?;

        if unsafe { IsClipboardFormatAvailable(CF_HDROP as u32) } != 0 {
            return read_file_list().map(|paths| Some(ClipboardContent::Files(paths)));
        }
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT as u32) } != 0 {
            return read_unicode_text().map(|text| Some(ClipboardContent::Text(text)));
        }
        Ok(None)
    }

    fn read_file_list() -> Result<Vec<PathBuf>> {
        let handle = unsafe { GetClipboardData(CF_HDROP as u32) };
        if handle.is_null() {
            bail!(
                "GetClipboardData(CF_HDROP) failed: {}",
                std::io::Error::last_os_error()
            );
        }

        let drop_handle = handle as HDROP;
        let count = unsafe { DragQueryFileW(drop_handle, u32::MAX, ptr::null_mut(), 0) };
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop_handle, index, ptr::null_mut(), 0) };
            let mut buffer = vec![0_u16; length as usize + 1];
            let written = unsafe {
                DragQueryFileW(drop_handle, index, buffer.as_mut_ptr(), buffer.len() as u32)
            };
            if written == 0 && length != 0 {
                bail!("DragQueryFileW failed: {}", std::io::Error::last_os_error());
            }
            paths.push(PathBuf::from(OsString::from_wide(
                &buffer[..written as usize],
            )));
        }
        Ok(paths)
    }

    fn read_unicode_text() -> Result<String> {
        let memory = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
        if memory.is_null() {
            bail!(
                "GetClipboardData(CF_UNICODETEXT) failed: {}",
                std::io::Error::last_os_error()
            );
        }

        let byte_count = unsafe { GlobalSize(memory) };
        let memory_pointer = unsafe { GlobalLock(memory) };
        if memory_pointer.is_null() {
            bail!("GlobalLock failed: {}", std::io::Error::last_os_error());
        }

        let text = unsafe {
            let code_units = std::slice::from_raw_parts(
                memory_pointer.cast::<u16>(),
                byte_count / size_of::<u16>(),
            );
            let nul_index = code_units
                .iter()
                .position(|code_unit| *code_unit == 0)
                .unwrap_or(code_units.len());
            String::from_utf16_lossy(&code_units[..nul_index])
        };
        unsafe {
            GlobalUnlock(memory);
        }
        Ok(text)
    }

    pub(super) fn write_unicode_text(text: &str) -> Result<()> {
        let mut encoded_text: Vec<u16> = text.encode_utf16().collect();
        encoded_text.push(0);
        let byte_count = encoded_text
            .len()
            .checked_mul(size_of::<u16>())
            .context("system clipboard text is too large")?;

        let clipboard = ClipboardSession::open()?;
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_count) };
        if memory.is_null() {
            bail!("GlobalAlloc failed: {}", std::io::Error::last_os_error());
        }

        let memory_pointer = unsafe { GlobalLock(memory) };
        if memory_pointer.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            bail!("GlobalLock failed: {}", std::io::Error::last_os_error());
        }
        unsafe {
            ptr::copy_nonoverlapping(
                encoded_text.as_ptr(),
                memory_pointer.cast::<u16>(),
                encoded_text.len(),
            );
            GlobalUnlock(memory);
        }

        if unsafe { EmptyClipboard() } == 0 {
            unsafe {
                GlobalFree(memory);
            }
            bail!("EmptyClipboard failed: {}", std::io::Error::last_os_error());
        }
        if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            bail!(
                "SetClipboardData(CF_UNICODETEXT) failed: {}",
                std::io::Error::last_os_error()
            );
        }

        drop(clipboard);
        Ok(())
    }

    pub(super) fn write_file_list(paths: &[PathBuf]) -> Result<()> {
        if paths.is_empty() {
            bail!("cannot publish an empty file list to the system clipboard");
        }

        let mut encoded_paths = Vec::new();
        for path in paths {
            encoded_paths.extend(path.as_os_str().encode_wide());
            encoded_paths.push(0);
        }
        encoded_paths.push(0);

        let byte_count = size_of::<DROPFILES>()
            .checked_add(encoded_paths.len() * size_of::<u16>())
            .context("system clipboard file list is too large")?;
        let clipboard = ClipboardSession::open()?;
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_count) };
        if memory.is_null() {
            bail!("GlobalAlloc failed: {}", std::io::Error::last_os_error());
        }

        let memory_pointer = unsafe { GlobalLock(memory) };
        if memory_pointer.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            bail!("GlobalLock failed: {}", std::io::Error::last_os_error());
        }

        unsafe {
            ptr::write(
                memory_pointer.cast::<DROPFILES>(),
                DROPFILES {
                    pFiles: size_of::<DROPFILES>() as u32,
                    pt: Default::default(),
                    fNC: 0,
                    fWide: 1,
                },
            );
            let path_data = memory_pointer
                .cast::<u8>()
                .add(size_of::<DROPFILES>())
                .cast::<u16>();
            ptr::copy_nonoverlapping(encoded_paths.as_ptr(), path_data, encoded_paths.len());
            GlobalUnlock(memory);
        }

        if unsafe { EmptyClipboard() } == 0 {
            unsafe {
                GlobalFree(memory);
            }
            bail!("EmptyClipboard failed: {}", std::io::Error::last_os_error());
        }
        if unsafe { SetClipboardData(CF_HDROP as u32, memory) }.is_null() {
            unsafe {
                GlobalFree(memory);
            }
            bail!(
                "SetClipboardData(CF_HDROP) failed: {}",
                std::io::Error::last_os_error()
            );
        }

        drop(clipboard);
        Ok(())
    }

    struct ClipboardSession;

    impl ClipboardSession {
        fn open() -> Result<Self> {
            for _ in 0..5 {
                if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                    return Ok(Self);
                }
                thread::sleep(Duration::from_millis(20));
            }
            bail!("OpenClipboard failed: {}", std::io::Error::last_os_error());
        }
    }

    impl Drop for ClipboardSession {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn open_with_default_application(path: &Path) -> Result<()> {
    let status = Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(path)
        .status()
        .context("could not start the Windows start command")?;
    if !status.success() {
        bail!("Windows start command exited with {status}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn open_with_default_application(path: &Path) -> Result<()> {
    let status = Command::new("open")
        .arg(path)
        .status()
        .context("could not start the open command")?;
    if !status.success() {
        bail!("open exited with {status}");
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn open_with_default_application(path: &Path) -> Result<()> {
    let status = Command::new("xdg-open")
        .arg(path)
        .status()
        .context("could not start the xdg-open command")?;
    if !status.success() {
        bail!("xdg-open exited with {status}");
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
pub(crate) fn open_with_default_application(_path: &Path) -> Result<()> {
    bail!("opening an item with the default application is not supported on this operating system")
}
