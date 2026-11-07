//! Exact-build SystemTray entry points, resolved from the matching Microsoft
//! PDB offline. Reject changed images and pre-existing function patches.
use litetray_adapter_protocol::compatibility::{self, TrayAbi};
pub(crate) fn resolve() -> Result<[usize; 3], String> {
    unsafe {
        let profile = compatibility::builtin()?;
        let TrayAbi::SystemTrayCoordinatesX64V1 = profile.tray_abi;
        let module = crate::compatibility::loaded(&profile.tray)?;
        let mut addresses = [0; 3];
        for (index, entry) in profile.tray_entries.iter().enumerate() {
            let address = module
                .checked_add(entry.rva as usize)
                .ok_or("托盘入口地址溢出")?;
            crate::inspect::check_region(address, 16, module, true)?;
            if crate::read_memory(address, 16).as_deref() != Some(entry.prologue.as_slice()) {
                return Err("托盘坐标入口已变化或已被其他组件改写".into());
            }
            addresses[index] = address;
        }
        Ok(addresses)
    }
}
