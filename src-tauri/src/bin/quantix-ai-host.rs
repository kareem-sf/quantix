//! Quantix-owned process boundary, independent of the frozen Python DLL loader.
//!
//! `quantix-ai-host -- /absolute/program args...` proxies parent stdin and inherits
//! stdout/stderr. Closing parent stdin, a stop signal, or worker exit terminates
//! the complete owned process group/job. Arguments contain paths, never secrets.
//! `--decompress-brotli SOURCE TARGET MAX_BYTES` unpacks one hash-checked native
//! client inside an inactive installation candidate; it never executes that file.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
#[cfg(not(windows))]
use std::time::Duration;

static STOP: AtomicBool = AtomicBool::new(false);

fn decompress_brotli(arguments: &[OsString]) -> io::Result<()> {
    use std::fs::{self, File, OpenOptions};
    use std::path::PathBuf;

    const MAXIMUM_BYTES: u64 = 600 * 1024 * 1024;
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Invalid private client archive.");
    if arguments.len() != 3 {
        return Err(invalid());
    }
    let source = Path::new(&arguments[0]);
    let target = Path::new(&arguments[1]);
    let maximum = arguments[2].to_str().and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0 && *value <= MAXIMUM_BYTES).ok_or_else(invalid)?;
    if !source.is_absolute() || !target.is_absolute() || source.extension() != Some(std::ffi::OsStr::new("br")) {
        return Err(invalid());
    }
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_BYTES {
        return Err(invalid());
    }
    let source = source.canonicalize()?;
    let parent = target.parent().ok_or_else(invalid)?.canonicalize()?;
    // Only materialize the same package's sibling binary. Absolute paths alone
    // are insufficient: resolve parents before checking their common directory.
    if source.parent() != Some(parent.as_path()) || source.with_extension("").file_name() != target.file_name() {
        return Err(invalid());
    }
    let target = parent.join(target.file_name().ok_or_else(invalid)?);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(invalid());
    }
    struct PendingFile { path: PathBuf, complete: bool }
    impl Drop for PendingFile {
        fn drop(&mut self) {
            if !self.complete { let _ = fs::remove_file(&self.path); }
        }
    }
    let input = File::open(source)?;
    let mut output = OpenOptions::new().write(true).create_new(true).open(&target)?;
    let mut pending = PendingFile { path: target, complete: false };
    let result = (|| -> io::Result<()> {
        let mut decoder = brotli_decompressor::Decompressor::new(input, 64 * 1024);
        let mut buffer = [0_u8; 64 * 1024];
        let mut written = 0_u64;
        loop {
            let size = decoder.read(&mut buffer)?;
            if size == 0 { break; }
            written = written.checked_add(size as u64).ok_or_else(invalid)?;
            if written > maximum { return Err(invalid()); }
            output.write_all(&buffer[..size])?;
        }
        if written == 0 { return Err(invalid()); }
        output.sync_all()?;
        Ok(())
    })();
    // Windows cannot unlink an open output file on a decoder/write error.
    drop(output);
    result?;
    pending.complete = true;
    Ok(())
}

fn forward_input(mut child_input: impl Write + Send + 'static) {
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    let queued = Arc::new(AtomicUsize::new(0));
    let writer_queued = Arc::clone(&queued);
    // The reader must observe parent EOF even if a child stops reading its pipe.
    // Separate writing plus a hard queue bound avoids either a stuck shutdown or
    // unbounded buffering of private request data.
    std::thread::spawn(move || {
        for bytes in receiver {
            let size = bytes.len();
            if child_input.write_all(&bytes).and_then(|_| child_input.flush()).is_err() {
                STOP.store(true, Ordering::SeqCst);
                break;
            }
            writer_queued.fetch_sub(size, Ordering::SeqCst);
        }
    });
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match input.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    if queued.fetch_add(size, Ordering::SeqCst) + size > 64 * 1024 * 1024 {
                        break;
                    }
                    if sender.send(buffer[..size].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
        STOP.store(true, Ordering::SeqCst);
    });
}

fn clean_environment() {
    // Restore the platform loader defaults in this small, separate executable.
    // The frozen core's process-wide DLL directory is never changed.
    for name in ["LD_LIBRARY_PATH", "LIBPATH", "DYLD_LIBRARY_PATH", "DYLD_FRAMEWORK_PATH"] {
        std::env::remove_var(name);
        std::env::remove_var(format!("{name}_ORIG"));
    }
    let inherited: Vec<_> = std::env::vars_os().map(|(key, _)| key).collect();
    for key in inherited {
        let upper = key.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("_PYI_") || upper.starts_with("PYTHON")
            || upper == "VIRTUAL_ENV" || upper == "__PYVENV_LAUNCHER__"
        {
            std::env::remove_var(key);
        }
    }
    std::env::set_var("PYTHONNOUSERSITE", "1");
    std::env::set_var("PYTHONUTF8", "1");
    std::env::set_var("PYTHONDONTWRITEBYTECODE", "1");
}

#[cfg(unix)]
fn launch(arguments: &[OsString]) -> io::Result<i32> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    extern "C" fn stop_signal(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGTERM, stop_signal as libc::sighandler_t);
        libc::signal(libc::SIGINT, stop_signal as libc::sighandler_t);
        libc::signal(libc::SIGHUP, stop_signal as libc::sighandler_t);
    }
    let mut child = Command::new(&arguments[0]).args(&arguments[1..])
        .stdin(Stdio::piped()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .process_group(0).spawn()?;
    let group = child.id() as libc::pid_t;
    forward_input(child.stdin.take().ok_or_else(|| io::Error::other("Worker input is unavailable."))?);
    let result = loop {
        if let Some(status) = child.try_wait()? {
            break status.code().unwrap_or(1);
        }
        if STOP.load(Ordering::SeqCst) {
            break 130;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    // A successful worker can still leave SDK descendants behind.
    unsafe { libc::kill(-group, libc::SIGTERM); }
    // Finish before MCP's outer two-second process-group kill deadline.
    let deadline = std::time::Instant::now() + Duration::from_millis(750);
    while std::time::Instant::now() < deadline {
        if unsafe { libc::kill(-group, 0) } == -1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    unsafe { libc::kill(-group, libc::SIGKILL); }
    let _ = child.wait();
    Ok(result)
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::fs::File;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Console::*;
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::LibraryLoader::*;
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::*;

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
    }
    fn checked(raw: HANDLE) -> io::Result<Handle> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE { Err(io::Error::last_os_error()) }
        else { Ok(Handle(raw)) }
    }
    unsafe extern "system" fn control(_: u32) -> i32 {
        STOP.store(true, Ordering::SeqCst);
        1
    }
    fn inherit(original: HANDLE) -> io::Result<Handle> {
        let mut target = null_mut();
        let process = unsafe { GetCurrentProcess() };
        if unsafe { DuplicateHandle(process, original, process, &mut target, 0, 1, DUPLICATE_SAME_ACCESS) } == 0 {
            return Err(io::Error::last_os_error());
        }
        checked(target)
    }
    // Windows argv quoting, preserving non-Unicode path code units. No shell.
    fn command_line(arguments: &[OsString]) -> Vec<u16> {
        let mut result = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            if index != 0 { result.push(b' ' as u16); }
            result.push(b'"' as u16);
            let mut slashes = 0;
            for unit in argument.encode_wide() {
                if unit == b'\\' as u16 { slashes += 1; continue; }
                let count = if unit == b'"' as u16 { slashes * 2 + 1 } else { slashes };
                result.extend(std::iter::repeat_n(b'\\' as u16, count));
                result.push(unit);
                slashes = 0;
            }
            result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
            result.push(b'"' as u16);
        }
        result.push(0);
        result
    }
    pub fn launch(arguments: &[OsString]) -> io::Result<i32> {
        unsafe {
            if SetDllDirectoryW(null()) == 0 || SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) == 0 {
                return Err(io::Error::last_os_error());
            }
            SetConsoleCtrlHandler(Some(control), 1);
        }
        let job = checked(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe { SetInformationJobObject(job.0, JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _, size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let security = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(), bInheritHandle: 1 };
        let (mut input_read, mut input_write) = (null_mut(), null_mut());
        if unsafe { CreatePipe(&mut input_read, &mut input_write, &security, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let input_read = checked(input_read)?;
        let input_write = checked(input_write)?;
        if unsafe { SetHandleInformation(input_write.0, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let output = inherit(unsafe { GetStdHandle(STD_OUTPUT_HANDLE) })?;
        let error = inherit(unsafe { GetStdHandle(STD_ERROR_HANDLE) })?;
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = input_read.0;
        startup.hStdOutput = output.0;
        startup.hStdError = error.0;
        let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
        let application: Vec<u16> = arguments[0].encode_wide().chain(Some(0)).collect();
        let mut line = command_line(arguments);
        if unsafe { CreateProcessW(application.as_ptr(), line.as_mut_ptr(), null(), null(), 1,
            CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            null(), null(), &startup, &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let process = checked(info.hProcess)?;
        let thread = checked(info.hThread)?;
        // Assignment precedes the first child instruction: no spawn/assign race.
        if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
            unsafe { TerminateProcess(process.0, 1); }
            return Err(io::Error::last_os_error());
        }
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        drop(input_read);
        drop(output);
        drop(error);
        let raw_input = input_write.0;
        std::mem::forget(input_write);
        forward_input(unsafe { File::from_raw_handle(raw_input) });
        let result = loop {
            let state = unsafe { WaitForSingleObject(process.0, 25) };
            if state == WAIT_OBJECT_0 {
                let mut code = 1;
                if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                break code as i32;
            }
            if state == WAIT_FAILED { return Err(io::Error::last_os_error()); }
            if STOP.load(Ordering::SeqCst) { break 130; }
        };
        unsafe { TerminateJobObject(job.0, result as u32); }
        // Dropping the only non-inherited job handle also covers host crashes.
        Ok(result)
    }
}

fn main() {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|argument| argument == "--decompress-brotli") {
        match decompress_brotli(&arguments[1..]) {
            Ok(()) => std::process::exit(0),
            Err(_) => {
                eprintln!("Quantix could not unpack the private AI client within its file and size limits.");
                std::process::exit(1);
            }
        }
    }
    if arguments.len() < 2 || arguments[0] != "--"
        || !Path::new(&arguments[1]).is_absolute() || !Path::new(&arguments[1]).is_file()
    {
        eprintln!("Quantix AI software requires an existing absolute program path.");
        std::process::exit(2);
    }
    clean_environment();
    #[cfg(windows)]
    let result = windows::launch(&arguments[1..]);
    #[cfg(unix)]
    let result = launch(&arguments[1..]);
    match result {
        Ok(code) => std::process::exit(code),
        Err(_) => {
            // OS error text can contain private paths; keep it out of MCP stdout.
            eprintln!("Quantix could not start or stop its owned AI process.");
            std::process::exit(1);
        }
    }
}
