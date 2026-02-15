#![windows_subsystem = "windows"]

use std::env;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_OK, MB_ICONWARNING, MB_SYSTEMMODAL
};

fn main() {
    // 初始化应用上下文
    let app_context = ApplicationContext::new();

    // 检查游戏目录和文件是否存在
    if !app_context.validate_game_paths() {
        return;
    }

    // 创建作业对象用于进程管理
    let job_object = Arc::new(JobObjectManager::new());

    // 启动辅助工具
    app_context.launch_helper_tools(&job_object);

    // 启动游戏并等待其结束
    app_context.run_game();
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

    fn validate_game_paths(&self) -> bool {
        // 使用更高效的方式一次性检查所有必需文件
        let required_paths = [
            (&self.th123_dir, "游戏目录"),
            (&self.game_path, "游戏文件"),
            (&self.swarm_path, "Swarm 工具"),
            (&self.tsk_path, "TSK 工具"),
        ];

        for (path, desc) in &required_paths {
            if !path.exists() {
                show_warning_message(&format!("未找到{}: {}", desc, path.display()));
                return false;
            }
        }

        true
    }

    fn launch_helper_tools(&self, job_object: &Arc<JobObjectManager>) {
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x00000200;

        // 使用预计算的基础目录
        let base_dir = &self.base_dir;

        // 定义要启动的工具列表
        let tools = [&self.swarm_path, &self.tsk_path];

        // 遍历并启动所有存在的工具
        for tool_path in &tools {
            if tool_path.exists() {
                if let Ok(child) = Command::new(tool_path)
                    .current_dir(base_dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                    .spawn()
                {
                    job_object.assign_process(child.as_raw_handle() as HANDLE);
                }
            }
        }
    }
    

    fn run_game(&self) {
        // 使用更高效的进程启动方式
        // 启动游戏后等待游戏结束，以便作业对象能够管理工作进程
        if let Ok(mut game_proc) = Command::new(&self.game_path)
            .current_dir(&self.th123_dir)
            .spawn()
        {
            // 等待游戏进程结束，这样作业对象才能在游戏结束后继续存在一段时间以管理工作进程
            let _ = game_proc.wait();
        }
    }
}

// 使 JobObjectManager 线程安全
unsafe impl Send for JobObjectManager {}
unsafe impl Sync for JobObjectManager {}

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

// 显示警告消息框的函数
fn show_warning_message(message: &str) {
    unsafe {
        // 将字符串转换为宽字符
        let wide_msg: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
        let wide_title: Vec<u16> = "警告".encode_utf16().chain(std::iter::once(0)).collect();
        
        MessageBoxW(
            0 as HWND,
            wide_msg.as_ptr(),
            wide_title.as_ptr(),
            MB_OK | MB_ICONWARNING | MB_SYSTEMMODAL
        );
    }
}