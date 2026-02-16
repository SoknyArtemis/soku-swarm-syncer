#![windows_subsystem = "windows"]

use std::env;
use std::ffi::OsStr;
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
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONERROR};

fn main() {
    // 极速启动：最大化并行化，最小化内存分配
    let app_context = ApplicationContext::new();

    // 创建作业对象用于进程管理
    let job_object = Arc::new(JobObjectManager::new());

    // 快速启动辅助工具（非阻塞）
    app_context.launch_helper_tools(&job_object);

    // 立即启动游戏，不进行完整验证
    app_context.run_game_with_job_object(job_object);
}

#[derive(Clone)]
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

        // 使用引用避免不必要的拷贝，只在需要时克隆
        let base_dir = &self.base_dir;
        let swarm_path = &self.swarm_path;
        let tsk_path = &self.tsk_path;

        // 并行启动辅助工具以提高效率，使用更少的内存拷贝
        // 为每个线程只传递必要的数据
        if swarm_path.exists() {
            let job_obj_swarm = Arc::clone(job_object); // 使用Arc::clone显式表示意图
            let base_dir_swarm = base_dir.clone();
            let swarm_path = swarm_path.clone();

            thread::spawn(move || {
                if let Ok(child) = Command::new(&swarm_path)
                    .current_dir(&base_dir_swarm)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                    .spawn()
                {
                    job_obj_swarm.assign_process(child.as_raw_handle() as HANDLE);
                }
            });
        }

        if tsk_path.exists() {
            let job_obj_tsk = Arc::clone(job_object); // 使用Arc::clone显式表示意图
            let base_dir_tsk = base_dir.clone();
            let tsk_path = tsk_path.clone();

            thread::spawn(move || {
                if let Ok(child) = Command::new(&tsk_path)
                    .current_dir(&base_dir_tsk)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                    .spawn()
                {
                    job_obj_tsk.assign_process(child.as_raw_handle() as HANDLE);
                }
            });
        }
    }
    

    fn run_game_with_job_object(self, job_object: Arc<JobObjectManager>) {
        // 检查游戏文件是否存在
        if !self.game_path.exists() {
            // 预先定义错误消息以减少运行时字符串操作
            let error_msg = format!("th123.exe not found: {}", self.game_path.display());
            show_error_message("Error: Game file not found", &error_msg);
            return;
        }

        // 启动游戏并将其添加到作业对象中
        match Command::new(&self.game_path)
            .current_dir(&self.th123_dir)
            .spawn()
        {
            Ok(mut game_proc) => {
                // 将游戏进程也添加到作业对象中
                job_object.assign_process(game_proc.as_raw_handle() as HANDLE);

                // 等待游戏进程结束
                let _ = game_proc.wait();
            },
            Err(_) => {
                let error_msg = format!("Could not start game: {}", self.game_path.display());
                show_error_message("Error: Could not start game", &error_msg);
            }
        }
    }
}

// 使 JobObjectManager 线程安全
unsafe impl Send for JobObjectManager {}
unsafe impl Sync for JobObjectManager {}

// 为 JobObjectManager 实现 Clone，以便在线程间共享
impl Clone for JobObjectManager {
    fn clone(&self) -> Self {
        JobObjectManager {
            handle: self.handle,
        }
    }
}

struct JobObjectManager {
    handle: HANDLE,
}

impl JobObjectManager {
    fn new() -> Self {
        let handle = unsafe {
            // 直接创建作业对象，减少不必要的检查
            let h = CreateJobObjectW(ptr::null_mut(), ptr::null());
            if h != 0 {
                // 设置作业对象属性以确保子进程随父进程关闭
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
        // 只有在句柄有效时才尝试分配进程
        if self.handle != 0 && process_handle != 0 {
            unsafe {
                AssignProcessToJobObject(self.handle, process_handle);
            }
        }
    }
}

impl Drop for JobObjectManager {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

fn show_error_message(title: &str, message: &str) {
    // 将字符串转换为宽字符（UTF-16），这是Windows API所要求的
    // 使用with_capacity预分配合适的容量以减少内存重分配
    let mut title_wide: Vec<u16> = OsStr::new(title).encode_wide().collect();
    title_wide.push(0); // 添加终止符
    
    let mut message_wide: Vec<u16> = OsStr::new(message).encode_wide().collect();
    message_wide.push(0); // 添加终止符

    unsafe {
        MessageBoxW(
            0, // hwnd参数应为0或INVALID_HANDLE_VALUE
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

