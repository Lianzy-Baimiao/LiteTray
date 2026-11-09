//! Exact-build SystemTray entry points, resolved from the matching Microsoft
//! PDB offline. Reject changed images and pre-existing function patches.
use litetray_adapter_protocol::compatibility::{ResolvedTray, TrayAbi};
pub(crate) fn resolve(tray: &ResolvedTray) -> Result<[usize; 3], String> {
    unsafe {
        match tray.abi {
            TrayAbi::SystemTrayCoordinatesX64V1
            | TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1 => {}
        }
        let module = crate::compatibility::loaded(&tray.module)?;
        let mut addresses = [0; 3];
        for (index, entry) in tray.entries.iter().enumerate() {
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
