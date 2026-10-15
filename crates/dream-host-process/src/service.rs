use crate::operations::{Failure, Output, runtime};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{ChildStderr, ChildStdin, ChildStdout, Command},
    sync::{Mutex as AsyncMutex, watch},
};

#[derive(Default, Deserialize)]
#[serde(default)]
pub(crate) struct Options {
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub clear_env: bool,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub input: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Request {
    Run {
        command: String,
        options: Options,
    },
    Spawn {
        command: String,
        options: Options,
    },
    Read {
        handle: i32,
        stream: i32,
        max_bytes: usize,
    },
    Write {
        handle: i32,
    },
    CloseStdin {
        handle: i32,
    },
    Wait {
        handle: i32,
    },
    Close {
        handle: i32,
    },
}
struct Child {
    stdin: AsyncMutex<Option<ChildStdin>>,
    stdout: AsyncMutex<Option<ChildStdout>>,
    stderr: AsyncMutex<Option<ChildStderr>>,
    cancel: watch::Sender<bool>,
    exit: watch::Receiver<Option<Result<i32, Failure>>>,
}
#[derive(Default)]
struct Children {
    next: i32,
    children: BTreeMap<i32, Arc<Child>>,
}
fn children() -> &'static Mutex<Children> {
    static CHILDREN: OnceLock<Mutex<Children>> = OnceLock::new();
    CHILDREN.get_or_init(Mutex::default)
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
fn io(error: std::io::Error) -> Failure {
    ("EIO", error.to_string())
}
fn child(id: i32) -> Result<Arc<Child>, Failure> {
    lock(children())
        .children
        .get(&id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "child process is closed"))
}
pub(crate) fn close(id: i32) {
    if let Some(child) = lock(children()).children.remove(&id) {
        let _ = child.cancel.send(true);
    }
}
pub(crate) struct OwnedChild {
    id: i32,
    child: Arc<Child>,
    detached: bool,
}
impl OwnedChild {
    pub fn detach(&mut self) {
        self.detached = true;
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.detached {
            close(self.id);
        }
    }
}
async fn wait(child: &Child) -> Result<i32, Failure> {
    let mut receiver = child.exit.clone();
    loop {
        if let Some(result) = receiver.borrow().clone() {
            return result;
        }
        receiver
            .changed()
            .await
            .map_err(|_| error("EIO", "process supervisor stopped"))?;
    }
}
async fn spawn(command: &str, wire: &[u8], options: &Options) -> Result<OwnedChild, Failure> {
    let args = crate::process_host::split_args(wire).map_err(|e| error("EINVAL", e))?;
    let mut command = Command::new(command);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if !options.cwd.is_empty() {
        command.current_dir(&options.cwd);
    }
    if options.clear_env {
        command.env_clear();
    }
    for (key, value) in &options.env {
        if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
            return Err(error("EINVAL", "invalid environment override"));
        }
        command.env(key, value);
    }
    #[cfg(unix)]
    // SAFETY: this child-side callback uses only signal-set operations before exec.
    unsafe {
        command.pre_exec(|| {
            let mut signals = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            if libc::sigemptyset(signals.as_mut_ptr()) != 0
                || libc::sigaddset(signals.as_mut_ptr(), libc::SIGPIPE) != 0
                || libc::pthread_sigmask(libc::SIG_UNBLOCK, signals.as_ptr(), std::ptr::null_mut())
                    != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut process = command.spawn().map_err(|e| error("ESPAWN", e))?;
    let (cancel, mut cancellation) = watch::channel(false);
    let (exited, exit) = watch::channel(None);
    let state = Arc::new(Child {
        stdin: AsyncMutex::new(process.stdin.take()),
        stdout: AsyncMutex::new(process.stdout.take()),
        stderr: AsyncMutex::new(process.stderr.take()),
        cancel,
        exit,
    });
    let id = {
        let mut registry = lock(children());
        if registry.next == i32::MAX || registry.children.len() >= 1024 {
            return Err(error("ELIMIT", "too many child processes"));
        }
        registry.next += 1;
        let id = registry.next;
        registry.children.insert(id, state.clone());
        id
    };
    let timeout_ms = options.timeout_ms;
    runtime()?.spawn(async move {
        let deadline = async {
            if timeout_ms == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            }
        };
        let status = tokio::select! {
            result = process.wait() => result.map(|s| s.code().unwrap_or(-2)).map_err(io),
            _ = cancellation.changed() => {
                let _ = process.start_kill();
                process.wait().await.map(|s| s.code().unwrap_or(-2)).map_err(io)
            }
            _ = deadline => {
                let _ = process.start_kill();
                let _ = process.wait().await;
                Err(error("ETIMEOUT", "process deadline exceeded"))
            }
        };
        let _ = exited.send(Some(status));
    });
    Ok(OwnedChild {
        id,
        child: state,
        detached: false,
    })
}
async fn read(child: &Child, stream: i32, maximum: usize) -> Result<Vec<u8>, Failure> {
    if maximum > 16 * 1024 * 1024 {
        return Err(error("EINVAL", "process reads are limited to 16 MiB"));
    }
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let mut bytes = vec![0; maximum];
    let mut cancellation = child.cancel.subscribe();
    if *cancellation.borrow() {
        return Err(error("ECLOSED", "child process closed"));
    }
    let size = if stream == 0 {
        let mut pipe = child.stdout.lock().await;
        let Some(pipe) = pipe.as_mut() else {
            return Err(error("ECLOSED", "stdout closed"));
        };
        tokio::select! { result = pipe.read(&mut bytes) => result.map_err(io)?, _ = cancellation.changed() => return Err(error("ECLOSED", "child process closed")) }
    } else if stream == 1 {
        let mut pipe = child.stderr.lock().await;
        let Some(pipe) = pipe.as_mut() else {
            return Err(error("ECLOSED", "stderr closed"));
        };
        tokio::select! { result = pipe.read(&mut bytes) => result.map_err(io)?, _ = cancellation.changed() => return Err(error("ECLOSED", "child process closed")) }
    } else {
        return Err(error("EINVAL", "invalid process stream"));
    };
    bytes.truncate(size);
    Ok(bytes)
}
async fn write(child: &Child, bytes: &[u8]) -> Result<usize, Failure> {
    let mut cancellation = child.cancel.subscribe();
    if *cancellation.borrow() {
        return Err(error("ECLOSED", "child process closed"));
    }
    let mut stdin = child.stdin.lock().await;
    let pipe = stdin
        .as_mut()
        .ok_or_else(|| error("ECLOSED", "stdin closed"))?;
    tokio::select! { result = pipe.write(bytes) => result.map_err(io), _ = cancellation.changed() => Err(error("ECLOSED", "child process closed")) }
}
async fn collect(child: &Child, stream: i32, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut output = Vec::new();
    loop {
        let bytes = read(child, stream, 65536).await?;
        if bytes.is_empty() {
            return Ok(output);
        }
        if bytes.len() > limit.saturating_sub(output.len()) {
            return Err(error(
                "ELIMIT",
                "captured process output exceeds configured limit",
            ));
        }
        output.extend_from_slice(&bytes);
    }
}
async fn run(
    command: String,
    bytes: Vec<u8>,
    options: Options,
    mut cancellation: watch::Receiver<bool>,
) -> Result<Vec<u8>, Failure> {
    if options.max_output_bytes > (i32::MAX as usize - 64) / 2 {
        return Err(error(
            "EINVAL",
            "capture limit exceeds the guest array range",
        ));
    }
    let mut launch = Options {
        cwd: options.cwd.clone(),
        env: options.env.clone(),
        clear_env: options.clear_env,
        ..Options::default()
    };
    launch.timeout_ms = 0;
    let owned = spawn(&command, &bytes, &launch).await?;
    let limit = if options.max_output_bytes == 0 {
        8 * 1024 * 1024
    } else {
        options.max_output_bytes
    };
    let execute = async {
        let input = async {
            let mut position = 0;
            while position < options.input.len() {
                let written = write(&owned.child, &options.input[position..]).await?;
                if written == 0 {
                    return Err(error("EIO", "stdin made no progress"));
                }
                position += written;
            }
            owned.child.stdin.lock().await.take();
            Ok::<_, Failure>(())
        };
        let ((), stdout, stderr, status) = tokio::try_join!(
            input,
            collect(&owned.child, 0, limit),
            collect(&owned.child, 1, limit),
            wait(&owned.child)
        )?;
        let mut output = format!("{status}\n{}\n", stdout.len()).into_bytes();
        output.extend(stdout);
        output.extend(stderr);
        Ok(output)
    };
    let deadline = async {
        if options.timeout_ms == 0 {
            std::future::pending::<()>().await;
        } else {
            tokio::time::sleep(Duration::from_millis(options.timeout_ms)).await;
        }
    };
    let result = tokio::select! {
        result = execute => result,
        _ = deadline => Err(error("ETIMEOUT", "process deadline exceeded")),
        _ = cancellation.changed() => Err(error("ECANCELLED", "process cancelled")),
    };
    if result.is_err() {
        let _ = owned.child.cancel.send(true);
        let _ = wait(&owned.child).await;
    }
    result
}
pub(crate) fn start(request: String, bytes: Vec<u8>) -> i32 {
    crate::operations::start(move |mut cancellation| async move {
        if *cancellation.borrow() {
            return Err(error("ECANCELLED", "process operation cancelled"));
        }
        let request: Request = serde_json::from_str(&request).map_err(|e| error("EINVAL", e))?;
        match request {
            Request::Run { command, options } => run(command, bytes, options, cancellation)
                .await
                .map(Output::bytes),
            Request::Spawn { command, options } => {
                let mut launch = Options {
                    cwd: options.cwd.clone(),
                    env: options.env.clone(),
                    clear_env: options.clear_env,
                    ..Options::default()
                };
                launch.timeout_ms = 0;
                let owned = spawn(&command, &bytes, &launch).await?;
                let id = owned.id;
                Ok(Output {
                    bytes: id.to_string().into_bytes(),
                    child: Some(owned),
                })
            }
            Request::Read {
                handle,
                stream,
                max_bytes,
            } => {
                let child = child(handle)?;
                tokio::select! { result = read(&child, stream, max_bytes) => result.map(Output::bytes), _ = cancellation.changed() => Err(error("ECANCELLED", "read cancelled")) }
            }
            Request::Write { handle } => {
                let child = child(handle)?;
                tokio::select! { result = write(&child, &bytes) => result.map(|size| Output::bytes(size.to_string().into_bytes())), _ = cancellation.changed() => Err(error("ECANCELLED", "write cancelled")) }
            }
            Request::CloseStdin { handle } => {
                child(handle)?.stdin.lock().await.take();
                Ok(Output::bytes(Vec::new()))
            }
            Request::Wait { handle } => {
                let child = child(handle)?;
                tokio::select! { result = wait(&child) => result.map(|code| Output::bytes(code.to_string().into_bytes())), _ = cancellation.changed() => Err(error("ECANCELLED", "wait cancelled")) }
            }
            Request::Close { handle } => {
                let child = child(handle)?;
                close(handle);
                let _ = wait(&child).await?;
                Ok(Output::bytes(Vec::new()))
            }
        }
    })
}
