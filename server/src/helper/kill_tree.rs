//! A Windows Job Object that owns a `claude` child and everything it starts.
//! The npm install is a `.cmd` shim, so killing the direct child (cmd.exe)
//! would leave node running; terminating the job kills the whole tree, and
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` does the same when PhotoSift exits or
//! crashes and the last handle closes. Elsewhere it is a no-op.

use std::process::Child;

pub struct KillTree {
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

// The handle is an owned kernel object; Win32 job calls are thread-safe.
#[cfg(windows)]
unsafe impl Send for KillTree {}
#[cfg(windows)]
unsafe impl Sync for KillTree {}

#[cfg(windows)]
impl KillTree {
    /// A new job holding `child`. A child started before the assignment can
    /// in principle escape; `cmd.exe` takes far longer than that to start node.
    pub fn adopt(child: &Child) -> std::io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: plain Win32 calls on a handle this function owns; the
        // struct is fully initialised and its size passed alongside it.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let tree = Self { job };
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                u32::try_from(std::mem::size_of_val(&info)).unwrap_or(u32::MAX),
            );
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(tree)
        }
    }

    /// Kills every process in the job now.
    pub fn terminate(&self) {
        // SAFETY: `self.job` is a live job handle owned by `self`.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for KillTree {
    fn drop(&mut self) {
        // SAFETY: closed exactly once; closing the last handle kills the tree.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(not(windows))]
impl KillTree {
    pub fn adopt(_child: &Child) -> std::io::Result<Self> {
        Ok(Self {})
    }

    pub fn terminate(&self) {}
}
