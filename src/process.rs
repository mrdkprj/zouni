use serde::{Deserialize, Serialize};
use shared_child::SharedChild;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, LazyLock, Mutex,
    },
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnOption {
    pub program: String,
    pub args: Option<Vec<String>>,
    pub cancellation_token: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommandStatus {
    pub success: bool,
    pub code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Output {
    pub status: CommandStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    fn error(code: Option<i32>, error: Option<String>) -> Self {
        Self {
            status: CommandStatus {
                success: false,
                code,
                error,
            },
            ..Default::default()
        }
    }
}

static CHILDREN: LazyLock<Mutex<HashMap<String, Arc<SharedChild>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static UUID: AtomicU16 = AtomicU16::new(0);

pub async fn spawn(option: SpawnOption) -> Result<Output, Output> {
    let mut command = Command::new(option.program);
    if let Some(args) = option.args {
        command.args(args);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW.0);

    let token = if let Some(token) = option.cancellation_token {
        token
    } else {
        UUID.fetch_add(1, Ordering::Relaxed).to_string()
    };

    let child = SharedChild::spawn(&mut command).map_err(|e| Output::error(e.raw_os_error(), Some(e.to_string())))?;
    {
        CHILDREN.lock().unwrap().insert(token.clone(), Arc::new(child));
    }

    smol::spawn(async move {
        let mut children = CHILDREN.lock().unwrap();
        let child = children.get(&token).unwrap();
        match child.wait() {
            Ok(exit_status) => {
                let stdout = if let Some(mut out) = child.take_stdout() {
                    let mut buf = String::new();
                    out.read_to_string(&mut buf).map_err(|e| Output::error(e.raw_os_error(), Some(e.to_string())))?;
                    buf
                } else {
                    String::new()
                };

                let stderr = if let Some(mut out) = child.take_stderr() {
                    let mut buf = String::new();
                    out.read_to_string(&mut buf).map_err(|e| Output::error(e.raw_os_error(), Some(e.to_string())))?;
                    buf
                } else {
                    String::new()
                };

                children.remove(&token);

                let result = Output {
                    status: CommandStatus {
                        success: exit_status.success(),
                        code: exit_status.code(),
                        error: None,
                    },
                    stderr,
                    stdout,
                };

                if exit_status.success() {
                    Ok(result)
                } else {
                    Err(result)
                }
            }
            Err(e) => Err(Output {
                status: CommandStatus {
                    success: false,
                    code: e.raw_os_error(),
                    error: Some(e.to_string()),
                },
                stderr: String::new(),
                stdout: String::new(),
            }),
        }
    })
    .await
}

pub fn kill(cancellation_token: String) -> Result<(), String> {
    if let Ok(mut children) = CHILDREN.try_lock() {
        if children.contains_key(&cancellation_token) {
            children.get_mut(&cancellation_token).unwrap().kill().map_err(|e| e.to_string())?;
            children.remove(&cancellation_token);
        }
    }

    Ok(())
}

pub fn clear() {
    let children = {
        let mut lock = CHILDREN.lock().unwrap();
        std::mem::take(&mut *lock)
    };
    for child in children.into_values() {
        let _ = child.kill();
    }
}
