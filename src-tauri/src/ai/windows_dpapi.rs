use std::{error::Error, fmt, ptr, slice};

use windows::Win32::{
    Foundation::{LocalFree, HLOCAL},
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub struct VaultCryptoError;

impl fmt::Debug for VaultCryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultCryptoError")
    }
}

impl fmt::Display for VaultCryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AI vault cryptography operation failed")
    }
}

impl Error for VaultCryptoError {}

struct LocalAllocBuffer {
    blob: CRYPT_INTEGER_BLOB,
}

impl LocalAllocBuffer {
    fn new(blob: CRYPT_INTEGER_BLOB) -> Self {
        Self { blob }
    }

    fn bytes(&self) -> Result<&[u8], VaultCryptoError> {
        let len = usize::try_from(self.blob.cbData).map_err(|_| VaultCryptoError)?;
        if len == 0 {
            return Ok(&[]);
        }
        if self.blob.pbData.is_null() {
            return Err(VaultCryptoError);
        }

        // SAFETY: a successful DPAPI call owns a LocalAlloc buffer of cbData bytes
        // until this guard frees it, and the null case was rejected above.
        Ok(unsafe { slice::from_raw_parts(self.blob.pbData, len) })
    }
}

impl Zeroize for LocalAllocBuffer {
    fn zeroize(&mut self) {
        let Ok(len) = usize::try_from(self.blob.cbData) else {
            return;
        };
        if len == 0 || self.blob.pbData.is_null() {
            return;
        }

        // SAFETY: DPAPI returned this LocalAlloc buffer with cbData writable bytes,
        // and the guard retains sole cleanup responsibility for the allocation.
        unsafe { slice::from_raw_parts_mut(self.blob.pbData, len) }.zeroize();
    }
}

impl ZeroizeOnDrop for LocalAllocBuffer {}

impl Drop for LocalAllocBuffer {
    fn drop(&mut self) {
        self.zeroize();
        if !self.blob.pbData.is_null() {
            // SAFETY: DPAPI documents output buffers as LocalAlloc allocations.
            let _ = unsafe { LocalFree(Some(HLOCAL(self.blob.pbData.cast()))) };
        }
        self.blob = CRYPT_INTEGER_BLOB::default();
    }
}

pub fn protect_for_current_user(
    plaintext: Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>, VaultCryptoError> {
    let input = input_blob(&plaintext)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    // SAFETY: the input pointer remains valid for the call, the output is
    // null-initialized for DPAPI, and all optional pointers are intentionally absent.
    let operation = unsafe {
        CryptProtectData(
            &input,
            windows_core::w!("Quantix AI vault"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let output = LocalAllocBuffer::new(output);

    match operation {
        Ok(()) => Ok(output.bytes()?.to_vec()),
        Err(_windows_error) => Err(VaultCryptoError),
    }
}

pub fn unprotect_for_current_user(
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultCryptoError> {
    let input = input_blob(ciphertext)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    // SAFETY: the input pointer remains valid for the call, the output is
    // null-initialized for DPAPI, and no description allocation is requested.
    let operation = unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    let output = LocalAllocBuffer::new(output);

    match operation {
        Ok(()) => Ok(Zeroizing::new(output.bytes()?.to_vec())),
        Err(_windows_error) => Err(VaultCryptoError),
    }
}

fn input_blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, VaultCryptoError> {
    let len = u32::try_from(bytes.len()).map_err(|_| VaultCryptoError)?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: len,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::{protect_for_current_user, unprotect_for_current_user};

    #[test]
    fn current_user_dpapi_round_trips_and_rejects_corruption() {
        let clear = Zeroizing::new("مفتاح-Quantix-123".as_bytes().to_vec());
        let encrypted = protect_for_current_user(clear).unwrap();
        assert!(!encrypted.windows(7).any(|part| part == b"Quantix"));
        assert_eq!(
            &*unprotect_for_current_user(&encrypted).unwrap(),
            "مفتاح-Quantix-123".as_bytes()
        );
        let mut corrupt = encrypted;
        let middle = corrupt.len() / 2;
        corrupt[middle] ^= 0x80;
        assert!(unprotect_for_current_user(&corrupt).is_err());
    }

    #[test]
    fn current_user_dpapi_round_trips_empty_plaintext() {
        let encrypted = protect_for_current_user(Zeroizing::new(Vec::new())).unwrap();

        assert!(unprotect_for_current_user(&encrypted).unwrap().is_empty());
    }

    #[test]
    fn current_user_dpapi_round_trips_one_mebibyte() {
        const ONE_MEBIBYTE: usize = 1024 * 1024;
        let encrypted = protect_for_current_user(Zeroizing::new(vec![0x5a; ONE_MEBIBYTE])).unwrap();
        let clear = unprotect_for_current_user(&encrypted).unwrap();

        assert_eq!(clear.len(), ONE_MEBIBYTE);
        assert!(clear.iter().all(|byte| *byte == 0x5a));
    }

    #[test]
    fn current_user_dpapi_rejects_truncated_ciphertext() {
        let mut encrypted =
            protect_for_current_user(Zeroizing::new(b"truncate-me".to_vec())).unwrap();
        encrypted.pop();

        assert!(unprotect_for_current_user(&encrypted).is_err());
    }

    #[test]
    fn current_user_dpapi_rejects_random_ciphertext_with_redacted_error() {
        let error = unprotect_for_current_user(&[0xa5; 64]).unwrap_err();

        assert_eq!(error.to_string(), "AI vault cryptography operation failed");
        assert_eq!(format!("{error:?}"), "VaultCryptoError");
    }
}
