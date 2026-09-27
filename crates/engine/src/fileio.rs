//! Positional file writes so every connection can write its own region of the
//! output file without seeking a shared cursor.

use std::fs::File;
use std::io;

#[cfg(unix)]
pub fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(buf, offset)
}

#[cfg(windows)]
pub fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_write(buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Mark the file as sparse on NTFS. Without this, writing near the end of a
/// freshly extended file makes Windows zero-fill everything before it first,
/// which stalls multi-connection downloads for seconds on large files.
#[cfg(windows)]
pub fn make_sparse(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let mut returned: u32 = 0;
    // SAFETY: valid open handle; FSCTL_SET_SPARSE takes no input/output buffers.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Unix filesystems allocate lazily already.
#[cfg(not(windows))]
pub fn make_sparse(_file: &File) -> io::Result<()> {
    Ok(())
}
