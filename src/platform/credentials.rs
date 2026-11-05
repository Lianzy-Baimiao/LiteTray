use crate::{config::Provider, weather::Secrets};
use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, GetLastError},
    Security::Credentials::*,
};

pub struct WindowsSecrets;
impl Secrets for WindowsSecrets {
    fn get(&self, provider: Provider) -> Result<Option<String>, String> {
        provider.credential_target().map(read).unwrap_or(Ok(None))
    }
}
pub fn read(target: &str) -> Result<Option<String>, String> {
    unsafe {
        let target = super::wide(target);
        let mut credential = std::ptr::null_mut();
        if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 {
            if GetLastError() == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err("无法读取 Windows 凭据管理器".into());
        }
        let result = if (*credential).CredentialBlobSize == 0 {
            Ok(None)
        } else if (*credential).CredentialBlob.is_null() || (*credential).CredentialBlobSize > 16384
        {
            Err("凭据内容无效".into())
        } else {
            let bytes = std::slice::from_raw_parts(
                (*credential).CredentialBlob,
                (*credential).CredentialBlobSize as usize,
            );
            String::from_utf8(bytes.to_vec())
                .map(Some)
                .map_err(|_| "凭据编码无效".into())
        };
        CredFree(credential.cast());
        result
    }
}
pub fn save(target: &str, secret: &str) -> Result<(), String> {
    unsafe {
        if secret.is_empty() || secret.len() > 2400 || secret.contains(['\r', '\n', '\0']) {
            return Err("凭据不能为空、含换行或超过 2400 字节".into());
        }
        let mut target = super::wide(target);
        let mut user = super::wide("LiteTray");
        let mut blob = secret.as_bytes().to_vec();
        let c = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            ..std::mem::zeroed()
        };
        let result = CredWriteW(&c, 0);
        blob.fill(0);
        if result == 0 {
            Err("无法保存到 Windows 凭据管理器".into())
        } else {
            Ok(())
        }
    }
}
pub fn delete(target: &str) -> Result<(), String> {
    unsafe {
        let target = super::wide(target);
        if CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) == 0
            && GetLastError() != ERROR_NOT_FOUND
        {
            Err("无法删除凭据".into())
        } else {
            Ok(())
        }
    }
}
