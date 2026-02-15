#![windows_subsystem = "windows"]

use std::env;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

fn main() {
    // 1. 路径处理 - 预先计算路径避免重复连接
    let mut base_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    base_dir.pop();

    // 预先计算常用路径以减少重复连接
    let th123_dir = base_dir.join("th123");
    let game_path = th123_dir.join("th123.exe");

    // 2. 创建 Job Object (内核级自动清理)
    let job_handle = unsafe {
        let h = CreateJobObjectW(ptr::null(), ptr::null());
        if h != 0 {
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

    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NO_WINDOW: u32 = 0x00000200;

    // 3. 直接处理辅助工具，避免创建中间数组，预先计算路径
    // 避免重复的路径连接操作
    if game_path.exists() {
        // 如果游戏存在，则启动辅助工具
        // 预先计算工具路径，避免重复连接
        let swarm_path = th123_dir.join("swarm.exe");
        let tsk_path = th123_dir.join("tsk/tsk_110A/tsk_yamei.exe");

        if swarm_path.exists() {
            if let Ok(child) = Command::new(&swarm_path)
                .current_dir(&base_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                .spawn()
            {
                if job_handle != 0 {
                    unsafe {
                        AssignProcessToJobObject(job_handle, child.as_raw_handle() as HANDLE);
                    }
                }
            }
        }

        if tsk_path.exists() {
            if let Ok(child) = Command::new(&tsk_path)
                .current_dir(&base_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                .spawn()
            {
                if job_handle != 0 {
                    unsafe {
                        AssignProcessToJobObject(job_handle, child.as_raw_handle() as HANDLE);
                    }
                }
            }
        }

        // 4. 运行游戏并阻塞
        if let Ok(mut game_proc) = Command::new(&game_path)
            .current_dir(&th123_dir)
            .spawn()
        {
            let _ = game_proc.wait();
        }
    }

    // 5. 退出清理
    if job_handle != 0 {
        unsafe { CloseHandle(job_handle); }
    }
}