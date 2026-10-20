//! AppContainer (no administrator rights): the program runs as a low-box process of the profile
//! `vibefork.sandbox`. It can read system folders (Windows grants them to all app containers) and
//! only the folders of the policy, which the helper grants to the container's SID in their ACLs
//! (the user owns them, no elevation needed). Capability: internet client. The program and
//! everything it starts are in a job object that ends with the helper.
//!
//! Known limit: an app container cannot connect to `localhost` servers outside itself.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use vf_core::sandbox::SandboxPolicy;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, HANDLE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL, CONTAINER_INHERIT_ACE, CreateWellKnownSid, DACL_SECURITY_INFORMATION,
    EqualSid, FreeSid, GetAce, NO_INHERITANCE, OBJECT_INHERIT_ACE, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES, WinCapabilityInternetClientSid,
};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Environment::GetCommandLineW;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
    WaitForSingleObject,
};

/// Name of the AppContainer profile.
const PROFILE: &str = "vibefork.sandbox";
/// `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)`.
const ALREADY_EXISTS: i32 = 0x8007_00B7_u32 as i32;
/// `DELETE` standard right; `SE_GROUP_ENABLED` attribute.
const DELETE: u32 = 0x0001_0000;
const SE_GROUP_ENABLED: u32 = 0x0000_0004;
/// Rights on read folders and on writable folders.
const READ: u32 = GENERIC_READ | GENERIC_EXECUTE;
const WRITE: u32 = GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | DELETE;
/// The same rights as stored in ACEs (generic bits mapped to file rights).
const STORED_READ: u32 = 0x0012_00A9;
const STORED_WRITE: u32 = 0x0013_01BF;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// The SID of the profile (created on first use). Freed on drop.
struct ContainerSid(PSID);

impl Drop for ContainerSid {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: the SID was allocated by `CreateAppContainerProfile` / `Derive…`, freed once here.
        unsafe {
            FreeSid(self.0);
        }
    }
}

#[allow(unsafe_code)]
fn container_sid() -> Result<ContainerSid, String> {
    let name = wide(OsStr::new(PROFILE));
    let display = wide(OsStr::new("Vibefork sandbox"));
    let mut sid: PSID = std::ptr::null_mut();
    // SAFETY: the strings are NUL-terminated and outlive the call; no capabilities are passed; `sid`
    // receives an allocated SID on success.
    let hr = unsafe {
        CreateAppContainerProfile(
            name.as_ptr(),
            display.as_ptr(),
            display.as_ptr(),
            std::ptr::null(),
            0,
            &mut sid,
        )
    };
    if hr == 0 {
        return Ok(ContainerSid(sid));
    }
    if hr != ALREADY_EXISTS {
        return Err(format!(
            "cannot create the AppContainer profile (0x{hr:08x})"
        ));
    }
    // SAFETY: as above; the profile exists, its SID is derived from the name.
    let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
    if hr == 0 {
        Ok(ContainerSid(sid))
    } else {
        Err(format!("cannot find the AppContainer profile (0x{hr:08x})"))
    }
}

/// Deletes the profile (uninstall). Permissions granted to it stay as unused entries.
#[allow(unsafe_code)]
pub(crate) fn remove() {
    let name = wide(OsStr::new(PROFILE));
    // SAFETY: `name` is NUL-terminated; a missing profile is not an error worth reporting.
    unsafe {
        DeleteAppContainerProfile(name.as_ptr());
    }
}

/// Whether app containers can be used.
pub(crate) fn available() -> bool {
    container_sid().is_ok()
}

/// Whether `acl` already allows `sid` everything in `mask`.
#[allow(unsafe_code)]
fn allows(acl: *const ACL, sid: PSID, mask: u32) -> bool {
    if acl.is_null() {
        return false;
    }
    // SAFETY: `acl` points to a valid ACL returned by `GetNamedSecurityInfoW`; `GetAce` checks the
    // index; only ACCESS_ALLOWED_ACE (type 0) entries are read as such, `SidStart` is their SID.
    unsafe {
        for i in 0..u32::from((*acl).AceCount) {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            if GetAce(acl, i, &mut ace) == 0 || ace.is_null() {
                continue;
            }
            let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
            if (*allowed).Header.AceType == 0
                && EqualSid(
                    std::ptr::addr_of!((*allowed).SidStart).cast_mut().cast(),
                    sid,
                ) != 0
                && (*allowed).Mask & mask == mask
            {
                return true;
            }
        }
    }
    false
}

/// Grants `mask` on `path` (inherited by everything inside a folder) unless already granted.
#[allow(unsafe_code)]
fn grant(path: &Path, sid: PSID, mask: u32) -> Result<(), String> {
    let name = wide(path.as_os_str());
    let mut old: *mut ACL = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated; `old` and `sd` receive pointers into one allocation (`sd`),
    // freed with `LocalFree` below.
    let err = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut old,
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if err != ERROR_SUCCESS {
        return Err(format!(
            "cannot read the permissions of {} ({err})",
            path.display()
        ));
    }
    let result = (|| {
        let stored = if mask == WRITE {
            STORED_WRITE
        } else {
            STORED_READ
        };
        if allows(old, sid, stored) {
            return Ok(());
        }
        let inherit = if path.is_dir() {
            OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
        } else {
            NO_INHERITANCE
        };
        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: mask,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: inherit,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: sid.cast(),
            },
        };
        let mut new: *mut ACL = std::ptr::null_mut();
        // SAFETY: `ea` is fully initialized and names a valid SID; `old` is the current DACL; `new`
        // receives a LocalAlloc'ed ACL, freed below.
        let err = unsafe { SetEntriesInAclW(1, &ea, old, &mut new) };
        if err != ERROR_SUCCESS {
            return Err(format!(
                "cannot build permissions for {} ({err})",
                path.display()
            ));
        }
        // SAFETY: `name` is NUL-terminated, `new` is the valid ACL built above.
        let err = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr().cast_mut(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                new,
                std::ptr::null(),
            )
        };
        // SAFETY: `new` was allocated by `SetEntriesInAclW`.
        unsafe {
            LocalFree(new.cast());
        }
        if err == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("cannot grant access to {} ({err})", path.display()))
        }
    })();
    // SAFETY: `sd` was allocated by `GetNamedSecurityInfoW`.
    unsafe {
        LocalFree(sd.cast());
    }
    result
}

/// The part of this process's command line after `sandbox-exec -- `, passed to the program as is
/// (keeps `cmd /s /c "…"` quoting intact).
#[allow(unsafe_code)]
fn program_line() -> Option<Vec<u16>> {
    // SAFETY: `GetCommandLineW` returns a NUL-terminated string owned by the process.
    let line = unsafe {
        let p = GetCommandLineW();
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    };
    let marker = format!("{} -- ", crate::EXEC_COMMAND);
    let tail = &line[line.find(&marker)? + marker.len()..];
    Some(tail.encode_utf16().chain(std::iter::once(0)).collect())
}

/// Grants the policy, runs the rest of the command line in the container, waits and exits with its
/// code. Returns only on failure.
#[allow(unsafe_code)]
pub(crate) fn exec(policy: &SandboxPolicy) -> String {
    let sid = match container_sid() {
        Ok(s) => s,
        Err(e) => return e,
    };
    for (paths, mask) in [(&policy.read, READ), (&policy.write, WRITE)] {
        for p in paths {
            if let Err(e) = grant(p, sid.0, mask) {
                return e;
            }
        }
    }
    let Some(mut line) = program_line() else {
        return "no program".into();
    };

    // Capability: internet client.
    let mut cap_sid = [0u8; 68];
    let mut cap_len = cap_sid.len() as u32;
    // SAFETY: `cap_sid` is a buffer of SECURITY_MAX_SID_SIZE bytes, `cap_len` its size.
    if unsafe {
        CreateWellKnownSid(
            WinCapabilityInternetClientSid,
            std::ptr::null_mut(),
            cap_sid.as_mut_ptr().cast(),
            &mut cap_len,
        )
    } == 0
    {
        return "cannot create the internet capability".into();
    }
    let mut caps = [SID_AND_ATTRIBUTES {
        Sid: cap_sid.as_mut_ptr().cast(),
        Attributes: SE_GROUP_ENABLED,
    }];
    let security = SECURITY_CAPABILITIES {
        AppContainerSid: sid.0,
        Capabilities: caps.as_mut_ptr(),
        CapabilityCount: 1,
        Reserved: 0,
    };

    // Attribute list with the security capabilities.
    let mut size = 0usize;
    // SAFETY: the first call only reports the needed size (it fails by design).
    unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
    let mut buf = vec![0u8; size];
    let list: LPPROC_THREAD_ATTRIBUTE_LIST = buf.as_mut_ptr().cast();
    // SAFETY: `buf` has the size reported above and outlives the list (deleted before return).
    if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
        return "cannot prepare the process attributes".into();
    }
    // SAFETY: `security` (and the SIDs it points to) live until the process is created.
    let ok = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            std::ptr::addr_of!(security).cast(),
            std::mem::size_of::<SECURITY_CAPABILITIES>(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        // SAFETY: initialized above.
        unsafe { DeleteProcThreadAttributeList(list) };
        return "cannot set the security capabilities".into();
    }

    // SAFETY: plain C structs; all-zero is valid; the needed fields are set below.
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    // SAFETY: `GetStdHandle` has no preconditions; the handles are this process's own (inheritable
    // pipes from the caller).
    unsafe {
        si.StartupInfo.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
        si.StartupInfo.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
        si.StartupInfo.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    }
    si.lpAttributeList = list;
    // SAFETY: as above.
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `line` is a writable NUL-terminated buffer; the standard handles are inherited (they
    // are the only inheritable ones this process passes on); the attribute list is valid.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW,
            std::ptr::null(),
            std::ptr::null(),
            &si.StartupInfo,
            &mut pi,
        )
    };
    // SAFETY: initialized above, not used after this.
    unsafe { DeleteProcThreadAttributeList(list) };
    if created == 0 {
        return format!(
            "cannot start the program in the sandbox: {}",
            std::io::Error::last_os_error()
        );
    }

    // SAFETY: valid handles from `CreateProcessW`; the job is configured before the process runs and
    // closes (killing the tree) when this helper ends.
    let code = unsafe {
        let job: HANDLE = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if !job.is_null() {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            AssignProcessToJobObject(job, pi.hProcess);
        }
        ResumeThread(pi.hThread);
        WaitForSingleObject(pi.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(pi.hProcess, &mut code);
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        code
    };
    std::process::exit(code as i32)
}
