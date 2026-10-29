//! Folders of the agent readable and writable by the current user only: Unix `0700`; on Windows a
//! protected DACL with two entries, the current user and SYSTEM, inherited by everything inside.

use std::path::Path;

/// Creates `dir` when missing and makes it the current user's only (Unix `0700`; Windows: the
/// current user and SYSTEM, full access, inherited, the parent's entries cut off). A folder that
/// already is stays untouched.
pub fn secure_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if is_owner_only(dir)? {
        return Ok(());
    }
    restrict(dir)
}

/// Whether only the current user (and on Windows SYSTEM) can open `dir`.
#[cfg(unix)]
pub fn is_owner_only(dir: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt as _;
    Ok(std::fs::metadata(dir)?.permissions().mode() & 0o077 == 0)
}

#[cfg(unix)]
fn restrict(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Whether only the current user (and on Windows SYSTEM) can open `dir`.
#[cfg(windows)]
pub fn is_owner_only(dir: &Path) -> std::io::Result<bool> {
    windows::is_owner_only(dir)
}

#[cfg(windows)]
fn restrict(dir: &Path) -> std::io::Result<()> {
    windows::restrict(dir)
}

#[cfg(not(any(unix, windows)))]
pub fn is_owner_only(_dir: &Path) -> std::io::Result<bool> {
    Ok(true)
}

#[cfg(not(any(unix, windows)))]
fn restrict(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::io::{Error, Result};
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
        GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
        IsWellKnownSid, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER, TokenUser, WinLocalSystemSid,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// `ACCESS_ALLOWED_ACE_TYPE` and `ACCESS_DENIED_ACE_TYPE` (winnt.h).
    const ALLOWED: u8 = 0;
    const DENIED: u8 = 1;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    /// The SID of the user the agent runs as, in a buffer aligned for `TOKEN_USER`.
    struct User {
        buf: Vec<u64>,
    }

    impl User {
        fn current() -> Result<Self> {
            let mut token: HANDLE = std::ptr::null_mut();
            // SAFETY: the pseudo handle of the current process needs no closing; `token` receives
            // a new handle on success.
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
                return Err(Error::last_os_error());
            }
            let mut len = 0u32;
            // SAFETY: a size query with a null buffer of length 0; `len` receives the size.
            unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
            let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
            let size = u32::try_from(buf.len() * 8).unwrap_or(u32::MAX);
            // SAFETY: `buf` is writable for `size` bytes and aligned for `TOKEN_USER`.
            let ok = unsafe {
                GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), size, &mut len)
            };
            let err = Error::last_os_error();
            // SAFETY: `token` was opened above and is closed once.
            unsafe { CloseHandle(token) };
            if ok == 0 {
                return Err(err);
            }
            Ok(Self { buf })
        }

        /// The SID; valid while `self` lives.
        fn sid(&self) -> PSID {
            // SAFETY: `GetTokenInformation(TokenUser)` filled the buffer with a `TOKEN_USER` whose
            // SID points inside the same buffer.
            unsafe { (*self.buf.as_ptr().cast::<TOKEN_USER>()).User.Sid }
        }

        /// The SID as text (`S-1-5-21-…`).
        fn sid_string(&self) -> Result<String> {
            let mut text: *mut u16 = std::ptr::null_mut();
            // SAFETY: `sid()` is a valid SID; `text` receives a buffer freed with `LocalFree`.
            if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
                return Err(Error::last_os_error());
            }
            // SAFETY: on success `text` is a NUL-terminated UTF-16 string.
            let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
            // SAFETY: `len` characters before the NUL were read above.
            let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
            // SAFETY: allocated by `ConvertSidToStringSidW`, freed once.
            unsafe { LocalFree(text.cast()) };
            Ok(s)
        }
    }

    pub(super) fn restrict(dir: &Path) -> Result<()> {
        let user = User::current()?;
        let sddl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;SY)", user.sid_string()?);
        let sddl: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; `sd` receives a buffer freed with `LocalFree`.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(Error::last_os_error());
        }
        let (mut present, mut defaulted) = (0, 0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        // SAFETY: `sd` is the descriptor made above; `dacl` points inside it.
        let ok = unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) };
        let result = if ok == 0 || present == 0 || dacl.is_null() {
            Err(Error::other("no DACL in the descriptor"))
        } else {
            let name = wide(dir);
            // SAFETY: `name` is NUL-terminated, `dacl` is valid while `sd` lives; owner, group and
            // SACL stay as they are (null). Inherited entries of the files inside are replaced.
            let code = unsafe {
                SetNamedSecurityInfoW(
                    name.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null(),
                )
            };
            if code == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(Error::from_raw_os_error(
                    i32::try_from(code).unwrap_or(i32::MAX),
                ))
            }
        };
        // SAFETY: allocated by `ConvertStringSecurityDescriptorToSecurityDescriptorW`, freed once.
        unsafe { LocalFree(sd.cast()) };
        result
    }

    pub(super) fn is_owner_only(dir: &Path) -> Result<bool> {
        dacl_state(dir).map(|(protected, entries)| protected && entries)
    }

    /// Whether the DACL of `path` is protected and whether its entries allow only the user and
    /// SYSTEM.
    pub(super) fn dacl_state(path: &Path) -> Result<(bool, bool)> {
        let user = User::current()?;
        let name = wide(path);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; `sd` receives a buffer freed with `LocalFree`, `dacl`
        // points inside it.
        let code = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(Error::from_raw_os_error(
                i32::try_from(code).unwrap_or(i32::MAX),
            ));
        }
        let state = (
            protected(sd),
            !dacl.is_null() && entries_owner_only(dacl, user.sid()),
        );
        // SAFETY: allocated by `GetNamedSecurityInfoW`, freed once.
        unsafe { LocalFree(sd.cast()) };
        Ok(state)
    }

    /// Whether the DACL of `sd` does not inherit the parent's entries.
    fn protected(sd: PSECURITY_DESCRIPTOR) -> bool {
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: `sd` is a valid descriptor.
        let ok = unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) };
        ok != 0 && control & SE_DACL_PROTECTED != 0
    }

    /// Whether every entry of `dacl` denies, or allows the user or SYSTEM.
    fn entries_owner_only(dacl: *mut ACL, user: PSID) -> bool {
        // SAFETY: `dacl` is a valid ACL.
        let count = unsafe { (*dacl).AceCount };
        (0..u32::from(count)).all(|i| {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            // SAFETY: `i` is below the entry count; `ace` points inside the ACL.
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                return false;
            }
            // SAFETY: every entry starts with an `ACE_HEADER`.
            match unsafe { (*ace.cast::<ACE_HEADER>()).AceType } {
                DENIED => true,
                ALLOWED => {
                    // SAFETY: an allowed entry is an `ACCESS_ALLOWED_ACE`; its SID starts at
                    // `SidStart` and lies inside the entry.
                    let sid: PSID = unsafe {
                        std::ptr::addr_of_mut!((*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart)
                    }
                    .cast();
                    // SAFETY: both are valid SIDs.
                    unsafe {
                        EqualSid(sid, user) != 0 || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                    }
                }
                _ => false,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_folder_becomes_owner_only_and_files_inside_inherit_it() {
        let base = std::env::temp_dir().join(format!("vf-access-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("tools");
        assert!(std::fs::create_dir_all(dir.join("bin")).is_ok());
        assert!(std::fs::write(dir.join("bin").join("tool.exe"), b"x").is_ok());
        assert!(secure_dir(&dir).is_ok());
        assert_eq!(is_owner_only(&dir).ok(), Some(true));
        // The current user still reads and writes inside.
        assert!(std::fs::write(dir.join("bin").join("tool.exe"), b"y").is_ok());
        assert!(std::fs::write(dir.join("new.txt"), b"z").is_ok());
        assert!(
            secure_dir(&dir).is_ok(),
            "a second call keeps the folder as is"
        );
        #[cfg(windows)]
        for inside in [
            dir.join("bin"),
            dir.join("bin").join("tool.exe"),
            dir.join("new.txt"),
        ] {
            assert_eq!(
                windows::dacl_state(&inside).ok(),
                Some((false, true)),
                "{inside:?} inherits only the user and SYSTEM"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
