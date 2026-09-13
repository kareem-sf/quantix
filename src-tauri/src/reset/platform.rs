//! Windows handles bind every check to the object actually used. Directory
//! handles deny rename and write access while descendants are visited; reparse points are
//! deleted as entries and never traversed. Leaf handles require all existing
//! users (including launchers' log handles) to close before deletion succeeds.
use super::{JOURNAL, LOCK};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use windows_sys::Win32::Storage::FileSystem::*;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Unsafe reset path.")
}

fn information(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

fn open(path: &Path, access: u32, share: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(share)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

pub fn pin_root(root: &Path) -> io::Result<Vec<File>> {
    if !root.is_absolute() {
        return Err(invalid());
    }
    let mut handles = Vec::new();
    let mut current = PathBuf::new();
    for component in root.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                current.push(component);
                continue;
            }
            Component::Normal(_) => current.push(component),
            _ => return Err(invalid()),
        }
        // Deny write-capable directory handles too: an empty directory must
        // not become a reparse point in place while its name stays pinned.
        let handle = open(
            &current,
            FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
            FILE_SHARE_READ,
        )?;
        let info = information(&handle)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return Err(invalid());
        }
        handles.push(handle);
    }
    if handles.is_empty() {
        return Err(invalid());
    }
    Ok(handles)
}

pub fn read_control(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let info = information(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(invalid());
    }
    Ok(file)
}

pub fn workspace_lock(path: &Path) -> io::Result<File> {
    if path.file_name() != Some(LOCK.as_ref()) {
        return Err(invalid());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let info = information(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(invalid());
    }
    // std uses LockFileEx: its exclusive range includes the byte at offset 0
    // held by the backend's Windows FileLock. Closing releases this lock.
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

pub fn write_control(root: &Path, saved: &serde_json::Value) -> io::Result<()> {
    use windows_sys::Wdk::Storage::FileSystem::{
        FileRenameInformation, NtSetInformationFile, FILE_RENAME_INFORMATION,
    };
    use windows_sys::Win32::{Foundation::RtlNtStatusToDosError, System::IO::IO_STATUS_BLOCK};
    let temporary = root.join(format!(".pending-reset-{}.tmp", std::process::id()));
    // Never follow a leftover link, and never truncate an existing file.
    match remove_entry(&temporary) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .access_mode(FILE_GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_WRITE_THROUGH)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(saved)?)?;
    file.sync_all()?;
    // MoveFileEx reopens the parent with write access, conflicting with the
    // directory pin. Native same-parent rename is anchored to the open source
    // handle, preserving directory protections throughout atomic replacement.
    let directory = open(
        root,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ,
    )?;
    let info = information(&directory)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
    {
        return Err(invalid());
    }
    let destination: Vec<u16> = JOURNAL.encode_utf16().collect();
    let size = std::mem::size_of::<FILE_RENAME_INFORMATION>() + destination.len() * 2;
    let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
    let rename = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // The usize buffer aligns FILE_RENAME_INFORMATION and includes its variable
    // UTF-16 tail. All pointers remain valid until the synchronous API returns.
    let mut status_block = IO_STATUS_BLOCK::default();
    let status = unsafe {
        (*rename).Anonymous.ReplaceIfExists = true;
        // A simple name and null RootDirectory rename within the source
        // handle's existing parent, without reopening that parent for writes.
        (*rename).RootDirectory = std::ptr::null_mut();
        (*rename).FileNameLength = (destination.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            destination.as_ptr(),
            std::ptr::addr_of_mut!((*rename).FileName).cast::<u16>(),
            destination.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status_block,
            rename.cast(),
            size as u32,
            FileRenameInformation,
        )
    };
    if status < 0 {
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ));
    }
    file.sync_all()?;
    Ok(())
}

pub fn remove_entry(path: &Path) -> io::Result<()> {
    // Probe with a rename/write-denying handle, then require an exclusive leaf
    // handle for deletion. An entry cannot be swapped between traversal and
    // delete: the handle identifies it, including a junction itself.
    let handle = deletion_handle(path)?;
    let info = information(&handle)?;
    let is_directory = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let is_reparse = info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    if is_directory && !is_reparse {
        for entry in fs::read_dir(path)? {
            remove_entry(&entry?.path())?;
        }
        delete_handle(&handle)
    } else {
        drop(handle);
        let exclusive = open(path, DELETE | FILE_READ_ATTRIBUTES, 0)?;
        let current = information(&exclusive)?;
        // A concurrently substituted ordinary directory is handled only by a
        // future traversal, never by following a previously inspected path.
        if current.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
            && current.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
        {
            return Err(invalid());
        }
        delete_handle(&exclusive)
    }
}

pub(super) fn deletion_handle(path: &Path) -> io::Result<File> {
    open(path, DELETE | FILE_READ_ATTRIBUTES, FILE_SHARE_READ)
}

fn delete_handle(file: &File) -> io::Result<()> {
    // Delete read-only imports without changing attributes on an outside hard
    // link. FORCE_IMAGE_SECTION_CHECK retains failure for mapped executables.
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE
            | FILE_DISPOSITION_FLAG_FORCE_IMAGE_SECTION_CHECK,
    };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            (&disposition as *const FILE_DISPOSITION_INFO_EX).cast(),
            std::mem::size_of_val(&disposition) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn wait_parent(pid: u32, timeout: Duration) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Ok(())
        } else {
            Err(error)
        };
    }
    let result =
        unsafe { WaitForSingleObject(handle, timeout.as_millis().min(u32::MAX as u128) as u32) };
    unsafe {
        CloseHandle(handle);
    }
    if result == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(io::ErrorKind::TimedOut.into())
    }
}
