use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, HANDLE_FLAG_INHERIT, LocalFree,
    SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT, WIN32_ERROR,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{
    ACE_FLAGS, ACL, DACL_SECURITY_INFORMATION, FreeSid, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_CAPABILITIES, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::SystemInformation::{
    GetSystemDirectoryW, GetSystemWindowsDirectoryW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows_sys::core::HRESULT;

use super::{Invocation, capture, joined, spawn_error, timeout_message};

/// Every tool shares one AppContainer identity, so install-root grants made for one run stay
/// valid for the next instead of being rewritten per invocation.
const PROFILE_NAME: &str = "Dream.GeneratorTool";
const PROFILE_EXISTS: HRESULT = (0x8007_0000 | (ERROR_ALREADY_EXISTS & 0xFFFF)) as HRESULT;
const COMMAND_LINE_LIMIT: usize = 32_767;

pub(super) fn run(invocation: &Invocation<'_>) -> Result<Output, String> {
    let binary = invocation.binary;
    if binary
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
    {
        return Err(format!(
            "generator tool `{}` is a batch script; compiler-run tools must be executables",
            binary.display()
        ));
    }
    let system = SystemDirs::query()?;
    let container = AppContainer::open()?;
    container.grant(
        invocation.scratch,
        FILE_ALL_ACCESS,
        SUB_CONTAINERS_AND_OBJECTS_INHERIT,
    )?;
    // The install-root ACE is left in place: it names only this compiler's AppContainer and
    // grants read/execute, while revoking it would rewrite the whole tree on every run and race
    // concurrent invocations sharing the same tool.
    if let Some(resources) = invocation.resources
        && !within(&system.root, resources)
    {
        container.grant(
            resources,
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        )?;
    }
    let binary_readable = within(&system.root, binary)
        || invocation
            .resources
            .is_some_and(|root| within(root, binary));
    if !binary_readable {
        container.grant(
            binary,
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            NO_INHERITANCE,
        )?;
    }

    let application = wide(binary.as_os_str())?;
    let mut command_line = command_line(binary, invocation.args)?;
    let environment = environment(&system, invocation.scratch)?;
    let directory = wide(invocation.scratch.as_os_str())?;

    let job = job()?;
    let (stdout_read, stdout_write) = pipe()?;
    let (stderr_read, stderr_write) = pipe()?;
    let stdin = OwnedHandle::from(
        File::open("NUL").map_err(|e| format!("opening NUL for tool stdin: {e}"))?,
    );
    let child = spawn(
        &container,
        &application,
        &mut command_line,
        &environment,
        &directory,
        [&stdin, &stdout_write, &stderr_write],
    )?;
    drop((stdin, stdout_write, stderr_write));
    child.start(&job)?;

    let stdout = capture(Some(File::from(stdout_read)));
    let stderr = capture(Some(File::from(stderr_read)));
    let exited = wait(&child.process, invocation.timeout);
    // Descendants must not outlive their scratch folder even when the root process exited.
    unsafe {
        TerminateJobObject(job.as_raw_handle(), 1);
    }
    let stdout = joined(stdout, "stdout")?;
    let stderr = joined(stderr, "stderr")?;
    if !exited? {
        return Err(timeout_message(invocation.timeout));
    }
    Ok(Output {
        status: child.exit_status()?,
        stdout,
        stderr,
    })
}

struct SystemDirs {
    root: PathBuf,
    system32: PathBuf,
}

impl SystemDirs {
    fn query() -> Result<Self, String> {
        Ok(Self {
            root: system_path(|buf, len| unsafe { GetSystemWindowsDirectoryW(buf, len) })?,
            system32: system_path(|buf, len| unsafe { GetSystemDirectoryW(buf, len) })?,
        })
    }
}

fn system_path(query: impl Fn(*mut u16, u32) -> u32) -> Result<PathBuf, String> {
    let mut buffer = vec![0u16; 260];
    loop {
        let capacity = u32::try_from(buffer.len()).map_err(|e| e.to_string())?;
        let len = query(buffer.as_mut_ptr(), capacity) as usize;
        if len == 0 {
            return Err(os_error("locating the Windows system directory"));
        }
        if len < buffer.len() {
            buffer.truncate(len);
            return Ok(PathBuf::from(OsString::from_wide(&buffer)));
        }
        buffer.resize(len, 0);
    }
}

/// Case-insensitive containment on resolved paths; unresolvable paths are never contained.
fn within(root: &Path, path: &Path) -> bool {
    let folded = |p: &Path| {
        std::fs::canonicalize(p)
            .ok()
            .map(|p| PathBuf::from(p.to_string_lossy().to_lowercase()))
    };
    matches!((folded(root), folded(path)), (Some(root), Some(path)) if path.starts_with(&root))
}

struct AppContainer {
    sid: PSID,
}

impl AppContainer {
    fn open() -> Result<Self, String> {
        let name = wide(OsStr::new(PROFILE_NAME))?;
        let description = wide(OsStr::new("Dream compiler generator tools"))?;
        let mut sid: PSID = null_mut();
        let created = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                description.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        let result = if created == PROFILE_EXISTS {
            unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) }
        } else {
            created
        };
        if result < 0 || sid.is_null() {
            return Err(format!(
                "creating the generator tool AppContainer failed (HRESULT {result:#010x})"
            ));
        }
        Ok(Self { sid })
    }

    /// Adds an allow ACE for the container, skipping the write when the DACL already has it.
    fn grant(&self, path: &Path, access: u32, inheritance: ACE_FLAGS) -> Result<(), String> {
        let name = wide(path.as_os_str())?;
        let context = |what: &str| format!("{what} for `{}`", path.display());
        let mut dacl: *mut ACL = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        check(status, || context("reading the access control list"))?;
        let _descriptor = LocalBox(descriptor);
        // A missing DACL already admits every principal, AppContainers included.
        if dacl.is_null() {
            return Ok(());
        }
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: access,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: inheritance,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: self.sid.cast(),
            },
        };
        let mut updated: *mut ACL = null_mut();
        let status = unsafe { SetEntriesInAclW(1, &entry, dacl, &mut updated) };
        check(status, || context("extending the access control list"))?;
        let _updated = LocalBox(updated.cast());
        // SAFETY: both ACLs come from the security APIs above and stay alive until return.
        if unsafe { acl_bytes(dacl) == acl_bytes(updated) } {
            return Ok(());
        }
        let status = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                updated,
                null(),
            )
        };
        check(status, || {
            context("granting the generator tool sandbox access")
        })
    }
}

impl Drop for AppContainer {
    fn drop(&mut self) {
        unsafe {
            FreeSid(self.sid);
        }
    }
}

unsafe fn acl_bytes<'a>(acl: *const ACL) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), usize::from((*acl).AclSize)) }
}

struct LocalBox(*mut c_void);

impl Drop for LocalBox {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn job() -> Result<OwnedHandle, String> {
    let raw = unsafe { CreateJobObjectW(null(), null()) };
    if raw.is_null() {
        return Err(os_error("creating the tool job object"));
    }
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // Closing the only job handle, including when the compiler dies, kills the whole tree.
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    let configured = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        return Err(os_error("configuring the tool job object"));
    }
    Ok(job)
}

fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
    let (mut read, mut write): (HANDLE, HANDLE) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, null(), 0) } == 0 {
        return Err(os_error("creating a tool output pipe"));
    }
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

struct Child {
    process: OwnedHandle,
    thread: OwnedHandle,
}

impl Child {
    /// The process was created suspended so no descendant can start before it joins the job.
    fn start(&self, job: &OwnedHandle) -> Result<(), String> {
        let process = self.process.as_raw_handle();
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0 {
            let error = os_error("placing the generator tool in its job object");
            unsafe {
                TerminateProcess(process, 1);
            }
            return Err(error);
        }
        if unsafe { ResumeThread(self.thread.as_raw_handle()) } == u32::MAX {
            return Err(os_error("starting the generator tool"));
        }
        Ok(())
    }

    fn exit_status(&self) -> Result<ExitStatus, String> {
        let mut code = 0u32;
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(os_error("reading the generator tool exit code"));
        }
        Ok(ExitStatus::from_raw(code))
    }
}

fn spawn(
    container: &AppContainer,
    application: &[u16],
    command_line: &mut [u16],
    environment: &[u16],
    directory: &[u16],
    stdio: [&OwnedHandle; 3],
) -> Result<Child, String> {
    let handles: [HANDLE; 3] = stdio.map(AsRawHandle::as_raw_handle);
    // Inheritance is enabled only on handles the caller closes right after spawning; the
    // handle list keeps this child from inheriting anything else the compiler holds.
    for handle in handles {
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(os_error("preparing tool stdio handles"));
        }
    }
    let capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: container.sid,
        Capabilities: null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    let mut attributes = AttributeList::new(2)?;
    attributes.set(
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
        (&raw const capabilities).cast(),
        size_of::<SECURITY_CAPABILITIES>(),
    )?;
    attributes.set(
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        handles.as_ptr().cast(),
        size_of_val(&handles),
    )?;
    // SAFETY: STARTUPINFOEXW is plain data for which all-zero is the documented initial state.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.as_ptr();
    // SAFETY: PROCESS_INFORMATION is an out-parameter of plain handles and ids.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW,
            environment.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    };
    if created == 0 {
        return Err(spawn_error(io::Error::last_os_error()));
    }
    Ok(unsafe {
        Child {
            process: OwnedHandle::from_raw_handle(info.hProcess),
            thread: OwnedHandle::from_raw_handle(info.hThread),
        }
    })
}

struct AttributeList {
    buffer: Vec<usize>,
}

impl AttributeList {
    fn new(count: u32) -> Result<Self, String> {
        let mut size = 0usize;
        // The sizing call is expected to fail; it only reports the required buffer length.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut size);
        }
        if size == 0 {
            return Err(os_error("sizing the tool process attribute list"));
        }
        let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), count, 0, &mut size)
        } == 0
        {
            return Err(os_error("initializing the tool process attribute list"));
        }
        Ok(Self { buffer })
    }

    fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.buffer.as_mut_ptr().cast()
    }

    /// `value` must stay alive until the process is created; callers pass locals that outlive
    /// the list.
    fn set(&mut self, attribute: u32, value: *const c_void, size: usize) -> Result<(), String> {
        let list = self.as_ptr();
        let updated = unsafe {
            UpdateProcThreadAttribute(list, 0, attribute as usize, value, size, null_mut(), null())
        };
        if updated == 0 {
            return Err(os_error("configuring the tool process attributes"));
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        let list = self.as_ptr();
        unsafe {
            DeleteProcThreadAttributeList(list);
        }
    }
}

/// Returns `Ok(false)` when `timeout` elapsed before the process exited.
fn wait(process: &OwnedHandle, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now().checked_add(timeout);
    loop {
        let remaining = deadline.map_or(Duration::MAX, |d| {
            d.saturating_duration_since(Instant::now())
        });
        let millis = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX - 1);
        match unsafe { WaitForSingleObject(process.as_raw_handle(), millis) } {
            WAIT_OBJECT_0 => return Ok(true),
            WAIT_TIMEOUT if remaining.is_zero() => return Ok(false),
            WAIT_TIMEOUT => {}
            _ => return Err(os_error("waiting for generator tool")),
        }
    }
}

/// Uses MSVC argv rules; the program path is quoted verbatim because argv[0] has no escapes.
fn command_line(binary: &Path, args: &[String]) -> Result<Vec<u16>, String> {
    let program: Vec<u16> = binary.as_os_str().encode_wide().collect();
    if program.contains(&u16::from(b'"')) {
        return Err(format!(
            "generator tool path `{}` contains a quote",
            binary.display()
        ));
    }
    let mut line = vec![u16::from(b'"')];
    line.extend(program);
    line.push(u16::from(b'"'));
    for arg in args {
        let mut quoted = String::from(" ");
        quote_arg(arg, &mut quoted);
        line.extend(quoted.encode_utf16());
    }
    if line.contains(&0) {
        return Err("generator tool arguments contain a NUL character".into());
    }
    if line.len() >= COMMAND_LINE_LIMIT {
        return Err(format!(
            "generator tool command line exceeds the Windows limit of {COMMAND_LINE_LIMIT} characters"
        ));
    }
    line.push(0);
    Ok(line)
}

fn quote_arg(arg: &str, out: &mut String) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
}

fn environment(system: &SystemDirs, scratch: &Path) -> Result<Vec<u16>, String> {
    let mut path = OsString::from(&system.system32);
    path.push(";");
    path.push(&system.root);
    path.push(";");
    path.push(system.system32.join("Wbem"));
    let root = system.root.as_os_str();
    let scratch = scratch.as_os_str();
    // CreateProcessW expects the block sorted case-insensitively by name.
    let vars: [(&str, &OsStr); 9] = [
        ("HOME", scratch),
        ("LANG", OsStr::new("C")),
        ("LC_ALL", OsStr::new("C")),
        ("PATH", &path),
        ("SystemRoot", root),
        ("TEMP", scratch),
        ("TMP", scratch),
        ("USERPROFILE", scratch),
        ("windir", root),
    ];
    let mut block = Vec::new();
    for (name, value) in vars {
        let value: Vec<u16> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err(format!("generator tool {name} contains a NUL character"));
        }
        block.extend(name.encode_utf16());
        block.push(u16::from(b'='));
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn wide(text: &OsStr) -> Result<Vec<u16>, String> {
    let mut wide: Vec<u16> = text.encode_wide().collect();
    if wide.contains(&0) {
        return Err(format!("`{}` contains a NUL character", text.display()));
    }
    wide.push(0);
    Ok(wide)
}

fn check(status: WIN32_ERROR, context: impl FnOnce() -> String) -> Result<(), String> {
    if status == ERROR_SUCCESS {
        return Ok(());
    }
    let error = io::Error::from_raw_os_error(status as i32);
    Err(format!("{}: {error}", context()))
}

fn os_error(what: &str) -> String {
    format!("{what}: {}", io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(scratch: &Path, args: &[&str], timeout: Duration) -> Result<Output, String> {
        let system = SystemDirs::query().unwrap();
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        run(&Invocation {
            binary: &system.system32.join("cmd.exe"),
            args: &args,
            scratch,
            resources: Some(&system.system32),
            timeout,
        })
    }

    #[test]
    fn quotes_arguments_with_msvc_rules() {
        let quoted = |arg: &str| {
            let mut out = String::new();
            quote_arg(arg, &mut out);
            out
        };
        assert_eq!(quoted("plain"), "plain");
        assert_eq!(quoted(""), "\"\"");
        assert_eq!(quoted("a b"), "\"a b\"");
        assert_eq!(quoted("a\"b"), "\"a\\\"b\"");
        assert_eq!(quoted("dir\\ x\\"), "\"dir\\ x\\\\\"");
        assert_eq!(quoted("q\\\"x"), "\"q\\\\\\\"x\"");
        assert_eq!(quoted("c:\\path\\no-space"), "c:\\path\\no-space");
    }

    #[test]
    fn captures_stdout() {
        let scratch = tempfile::tempdir().unwrap();
        let output = invoke(scratch.path(), &["/c", "echo hi"], Duration::from_secs(30)).unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hi");
    }

    #[test]
    fn kills_tool_on_timeout() {
        let scratch = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let error = invoke(
            scratch.path(),
            &["/c", "for /l %i in (0,0,1) do @rem"],
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("timeout"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn writes_only_inside_scratch() {
        // cmd.exe does not honour MSVC `\"` escapes, so the outside target needs no quoting.
        let outside = tempfile::tempdir().unwrap();
        let scratch = outside.path().join("scratch");
        std::fs::create_dir(&scratch).unwrap();
        let output = invoke(
            &scratch,
            &["/c", "echo x> ..\\escaped.txt"],
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(!outside.path().join("escaped.txt").exists());

        let output = invoke(
            &scratch,
            &["/c", "echo x> inside.txt"],
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(scratch.join("inside.txt").exists());
    }
}
