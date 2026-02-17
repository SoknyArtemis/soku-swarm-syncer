#![windows_subsystem = "windows"]

use std::env;
use std::fs::File;
use std::io::Read;
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
    // 1. 初始化路径上下文并用 Arc 包装
    let app_context = Arc::new(ApplicationContext::new());

    // 2. 异步预热线程：利用 OS Page Cache 加速冷启动过程
    let warm_up_ctx = Arc::clone(&app_context);
    thread::spawn(move || {
        let targets = [
            &warm_up_ctx.game_path,
            &warm_up_ctx.swarm_path,
            &warm_up_ctx.tsk_path,
        ];
        for path in targets {
            // 只打开并读取第一个字节，强制 OS 将文件头部加载至内存
            if let Ok(mut f) = File::open(path) {
                let mut buf = [0u8; 1];
                let _ = f.read(&mut buf);
            }
        }
    });

    // 3. 初始化 Job Object 确保主程序退出时关闭所有子进程
    let job_object = Arc::new(JobObjectManager::new());

    // 4. 启动逻辑
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
        
        // 【关键】严格保持 TSK 路径处理逻辑不动
        let tsk_path = th123_dir.join("tsk/tsk_110A/tsk_yamei.exe");

        Self {
            base_dir,
            th123_dir,
            game_path,
            swarm_path,
            tsk_path,
        }
    }

    fn launch_helper_tools(self: &Arc<Self>, job_object: &Arc<JobObjectManager>) {
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x00000200;

        // 启动 Swarm
        let ctx_s = Arc::clone(self);
        let job_s = Arc::clone(job_object);
        thread::spawn(move || {
            if let Ok(child) = Command::new(&ctx_s.swarm_path)
                .current_dir(&ctx_s.base_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                .spawn()
            {
                job_s.assign_process(child.as_raw_handle() as HANDLE);
            }
        });

        // 启动 TSK - 严格保持原有的 path 和 current_dir 逻辑
        let ctx_t = Arc::clone(self);
        let job_t = Arc::clone(job_object);
        thread::spawn(move || {
            if let Ok(child) = Command::new(&ctx_t.tsk_path)
                .current_dir(&ctx_t.base_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                .spawn()
            {
                job_t.assign_process(child.as_raw_handle() as HANDLE);
            }
        });
    }

    fn run_game_with_job_object(self: Arc<Self>, job_object: Arc<JobObjectManager>) {
        match Command::new(&self.game_path)
            .current_dir(&self.th123_dir)
            .spawn()
        {
            Ok(mut game_proc) => {
                job_object.assign_process(game_proc.as_raw_handle() as HANDLE);
                let _ = game_proc.wait();
            }
            Err(_) => {
                // 修改后的报错信息，匹配你要求的格式
                show_error_message("Error", "Could not find \"th123/th123.exe\"");
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
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

fn show_error_message(title: &str, message: &str) {
    let title_wide: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let message_wide: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect();

    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}