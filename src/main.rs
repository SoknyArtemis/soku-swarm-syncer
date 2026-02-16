#![windows_subsystem = "windows"]

use std::env;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::Arc;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

fn main() {
    let app_context = ApplicationContext::new();
    let job_object = Arc::new(JobObjectManager::new());

    app_context.launch_helper_tools(&job_object);
    app_context.run_game_with_job_object(job_object);
}

struct ApplicationContext {
    base_dir: PathBuf,
    th123_dir: PathBuf,
    game_path: PathBuf,
    swarm_path: PathBuf,
    tsk_path: PathBuf,
}

impl ApplicationContext {
    fn new() -> Self {
        let mut base_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        base_dir.pop();

        let th123_dir = base_dir.join("th123");
        let game_path = th123_dir.join("th123.exe");
        let swarm_path = th123_dir.join("swarm.exe");
        let tsk_path = th123_dir.join("tsk/tsk_110A/tsk_yamei.exe");

        Self {
            base_dir,
            th123_dir,
            game_path,
            swarm_path,
            tsk_path,
        }
    }

    fn launch_helper_tools(&self, job_object: &Arc<JobObjectManager>) {
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x00000200;

        if self.swarm_path.exists() {
            let job_obj = Arc::clone(job_object);
            let path = self.swarm_path.clone();
            let dir = self.base_dir.clone();
            thread::spawn(move || {
                if let Ok(child) = Command::new(path)
                    .current_dir(dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                    .spawn()
                {
                    job_obj.assign_process(child.as_raw_handle() as HANDLE);
                }
            });
        }

        if self.tsk_path.exists() {
            let job_obj = Arc::clone(job_object);
            let path = self.tsk_path.clone();
            let dir = self.base_dir.clone();
            thread::spawn(move || {
                if let Ok(child) = Command::new(path)
                    .current_dir(dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                    .spawn()
                {
                    job_obj.assign_process(child.as_raw_handle() as HANDLE);
                }
            });
        }
    }

    fn run_game_with_job_object(self, job_object: Arc<JobObjectManager>) {
        if !self.game_path.exists() {
            show_error_message("Error", &format!("th123.exe not found: {:?}", self.game_path));
            return;
        }

        match Command::new(&self.game_path)
            .current_dir(&self.th123_dir)
            .spawn()
        {
            Ok(mut game_proc) => {
                job_object.assign_process(game_proc.as_raw_handle() as HANDLE);
                let _ = game_proc.wait();
            },
            Err(e) => {
                show_error_message("Error", &format!("Could not start game: {}", e));
            }
        }
    }
}

struct JobObjectManager {
    handle: HANDLE,
}

unsafe impl Send for JobObjectManager {}
unsafe impl Sync for JobObjectManager {}

impl JobObjectManager {
    fn new() -> Self {
        let handle = unsafe {
            let h = CreateJobObjectW(ptr::null(), ptr::null());
            // 适配 0.61.2：判断指针是否为空
            if !h.is_null() {
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
            }
            h
        };
        Self { handle }
    }

    pub fn assign_process(&self, process_handle: HANDLE) {
        // 适配 0.61.2：判断指针是否为空
        if !self.handle.is_null() && !process_handle.is_null() {
            unsafe {
                AssignProcessToJobObject(self.handle, process_handle);
            }
        }
    }
}

impl Drop for JobObjectManager {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { CloseHandle(self.handle); }
        }
    }
}

fn show_error_message(title: &str, message: &str) {
    let title_wide: Vec<u16> = std::ffi::OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let message_wide: Vec<u16> = std::ffi::OsStr::new(message).encode_wide().chain(Some(0)).collect();

    unsafe {
        // 适配 0.61.2：第一个参数 HWND 必须是空指针
        MessageBoxW(ptr::null_mut(), message_wide.as_ptr(), title_wide.as_ptr(), MB_OK | MB_ICONERROR);
    }
}