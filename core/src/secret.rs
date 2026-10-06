//! At-rest protection for the identity key and the connection password.
//!
//! On Windows secrets are sealed with DPAPI (bound to the current user
//! account). On Android the app's private data directory is already
//! inaccessible to other apps; the host app may additionally wrap the whole
//! data directory with the Android Keystore. Other platforms store the
//! bytes with owner-only permissions.

use std::io;
use std::path::Path;

pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
    imp::protect(data)
}

pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
    imp::unprotect(data)
}

/// Writes `data` so that a crash never leaves a truncated file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    const ENTROPY: &[u8] = b"vegamshare-secret-v1";

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        // SAFETY: on success DPAPI returns a LocalAlloc'd buffer of cbData bytes
        // that we own and must release with LocalFree.
        unsafe {
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData.cast());
            v
        }
    }

    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        let input = blob(data);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: all pointers reference live stack values or borrowed slices
        // that outlive the call; DPAPI does not retain them.
        let ok = unsafe {
            CryptProtectData(&input, ptr::null(), &entropy, ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(take(out))
    }

    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        let input = blob(data);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: as in `protect`.
        let ok = unsafe {
            CryptUnprotectData(&input, ptr::null_mut(), &entropy, ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(take(out))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;

    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let sealed = super::protect(b"hunter2").unwrap();
        assert_eq!(super::unprotect(&sealed).unwrap(), b"hunter2");
        #[cfg(windows)]
        assert_ne!(sealed, b"hunter2");
    }
}
