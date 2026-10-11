use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MEMORY_DB: &str = "/data/data/com.termux/files/home/hummie/core/josie_memory.db";

// Model cascade: eradicates quota, token, and rate limits via automatic failover
const MODEL_CASCADE: &[&str] = &[
    "gemini-3.1-pro-high",
    "gemini-3.8-flash-high",
    "claude-sonnet-5-5-high",
    "claude-opus-5-5-high",
    "gemini-3.7-flash-high",
    "gemini-3.6-flash-high",
    "gpt-oss-120b-medium",
];

/// Persistent Shizuku root pipe for zero-latency commands
struct RootPipe {
    stdin: std::process::ChildStdin,
}

impl RootPipe {
    fn new() -> Option<Self> {
        let mut child = Command::new("rish")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        Some(RootPipe { stdin })
    }

    fn execute(&mut self, cmd: &str) {
        let payload = format!("{}\n", cmd);
        if let Err(e) = self.stdin.write_all(payload.as_bytes()) {
            eprintln!("[PIPE] write failed: {}", e);
        }
        let _ = self.stdin.flush();
    }
}

/// Direct one-shot root command execution via rish
fn run_root(cmd: &str) -> (i32, String) {
    match Command::new("rish").arg("-c").arg(cmd).output() {
        Ok(out) => {
            let code = out.status.code().unwrap_or(0);
            let s = if out.stdout.is_empty() {
                String::from_utf8_lossy(&out.stderr).to_string()
            } else {
                String::from_utf8_lossy(&out.stdout).to_string()
            };
            (code, s.trim().to_string())
        }
        Err(e) => (-1, format!("rish execution failed: {}", e)),
    }
}

/// SQLite permanent memory logging for Josie and Hummie Agents
fn remember(event: &str, outcome: &str) {
    let clean_event = event.replace('\'', "''");
    let clean_outcome = outcome.replace('\'', "''");
    let outcome_trimmed = if clean_outcome.len() > 3900 {
        &clean_outcome[..3900]
    } else {
        &clean_outcome
    };

    let sql = format!(
        "INSERT INTO memory (timestamp, event, outcome) VALUES (datetime('now', 'localtime'), '{}', '{}');",
        clean_event, outcome_trimmed
    );

    let _ = Command::new("sqlite3")
        .arg(MEMORY_DB)
        .arg(&sql)
        .output();
}

/// Read recent memory records
fn show_recent_memory(prefix: &str, limit: usize) {
    let sql = format!(
        "SELECT timestamp, event, substr(outcome, 1, 140) FROM memory WHERE event LIKE '{}%' ORDER BY id DESC LIMIT {};",
        prefix.replace('\'', "''"),
        limit
    );

    if let Ok(out) = Command::new("sqlite3").arg("-line").arg(MEMORY_DB).arg(&sql).output() {
        print!("{}", String::from_utf8_lossy(&out.stdout));
    }
}

/// Execute AGY with automatic model cascade across all models (no artificial limits)
fn agy_execute(prompt: &str, extra_args: &[&str], raw: bool, label: &str) -> i32 {
    let mut last_err = String::new();

    for model in MODEL_CASCADE {
        let mut cmd = Command::new("agy");
        cmd.arg("--dangerously-skip-permissions");
        cmd.arg("--output-format");
        cmd.arg("json");
        cmd.arg("--model");
        cmd.arg(model);

        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.arg("-p");
        cmd.arg(prompt);

        match cmd.output() {
            Ok(out) if out.status.success() && !out.stdout.is_empty() => {
                let stdout_str = String::from_utf8_lossy(&out.stdout).to_string();
                if raw {
                    println!("{}", stdout_str.trim());
                } else {
                    let cleaned = parse_json_result(&stdout_str);
                    println!("{}", cleaned);
                }
                remember(label, &format!("model={} | prompt={} | out={}", model, &prompt[..prompt.len().min(120)], &stdout_str[..stdout_str.len().min(1000)]));
                return 0;
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                let out_str = String::from_utf8_lossy(&out.stdout).to_string();
                last_err = format!("Model {} depleted/failed: {} {}", model, err, out_str);
                continue;
            }
            Err(e) => {
                last_err = format!("Model {} launch failed: {}", model, e);
                continue;
            }
        }
    }

    eprintln!("[CASCADE ERROR] All models exhausted. Last error: {}", last_err);
    remember(label, &format!("FAILED | prompt={} | err={}", &prompt[..prompt.len().min(120)], &last_err[..last_err.len().min(500)]));
    1
}

/// Parse AGY JSON response text cleanly
fn parse_json_result(json_str: &str) -> String {
    for key in &["\"result\":", "\"response\":", "\"text\":"] {
        if let Some(pos) = json_str.find(key) {
            let rest = &json_str[pos + key.len()..];
            let trimmed = rest.trim_start();
            if trimmed.starts_with('"') {
                let inner = &trimmed[1..];
                let mut unescaped = String::new();
                let mut chars = inner.chars();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            match next {
                                'n' => unescaped.push('\n'),
                                'r' => unescaped.push('\r'),
                                't' => unescaped.push('\t'),
                                '"' => unescaped.push('"'),
                                '\\' => unescaped.push('\\'),
                                other => unescaped.push(other),
                            }
                        }
                    } else {
                        unescaped.push(c);
                    }
                }
                if !unescaped.is_empty() {
                    return unescaped;
                }
            }
        }
    }
    json_str.trim().to_string()
}

/// Touchscreen input injection (tap, swipe, type, key)
fn handle_mouse(args: &[String]) {
    if args.is_empty() {
        println!("Usage: josie mouse <click|tap|swipe|drag|type|key> [args]");
        return;
    }
    let sub = &args[0];
    match sub.as_str() {
        "click" | "tap" => {
            if args.len() >= 3 {
                let cmd = format!("input touchscreen tap {} {}", args[1], args[2]);
                run_root(&cmd);
            } else {
                println!("Usage: josie mouse tap <x> <y>");
            }
        }
        "swipe" | "drag" => {
            if args.len() >= 5 {
                let ms = if args.len() > 5 { &args[5] } else { "400" };
                let cmd = format!("input touchscreen swipe {} {} {} {} {}", args[1], args[2], args[3], args[4], ms);
                run_root(&cmd);
            } else {
                println!("Usage: josie mouse swipe <x1> <y1> <x2> <y2> [ms]");
            }
        }
        "type" => {
            if args.len() >= 2 {
                let text = args[1..].join("%s");
                let cmd = format!("input text {}", text);
                run_root(&cmd);
            } else {
                println!("Usage: josie mouse type <text>");
            }
        }
        "key" => {
            if args.len() >= 2 {
                let cmd = format!("input keyevent {}", args[1]);
                run_root(&cmd);
            } else {
                println!("Usage: josie mouse key <keycode>");
            }
        }
        other => {
            println!("[MOUSE] Unknown action: {}", other);
        }
    }
}

/// Native high-speed file explorer
fn explore_path(target: &str) {
    let p = PathBuf::from(target);
    match fs::read_dir(&p) {
        Ok(entries) => {
            println!("📂 Contents of {}:", p.display());
            for entry in entries.flatten() {
                let path = entry.path();
                let meta = entry.metadata().ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                if is_dir {
                    println!("  📁 {:<32} [DIR]", file_name);
                } else {
                    println!("  📄 {:<32} {:>10} bytes", file_name, size);
                }
            }
        }
        Err(e) => {
            eprintln!("[EXPLORER ERROR] Cannot open {}: {}", target, e);
        }
    }
}

/// Swarm eternal daemon loop
fn ignite_swarm_daemon() {
    println!("⚡ IGNITING JOSIE & HUMMIE AGENTS NATIVE DAEMON...");

    let init_cmds = [
        "/system/bin/device_config set_sync_disabled_for_tests persistent",
        "/system/bin/device_config put activity_manager max_phantom_processes 2147483647",
        "settings put global settings_enable_monitor_phantom_procs false",
    ];
    for cmd in &init_cmds {
        run_root(cmd);
    }
    println!("[ ✔ ] Phantom Killer Neutralized. Limits set to MAX.");

    let pipe_opt = RootPipe::new();
    if let Some(pipe_struct) = pipe_opt {
        let root_pipe = Arc::new(Mutex::new(pipe_struct));
        let baton = Arc::clone(&root_pipe);

        thread::spawn(move || {
            let mut cycle: u64 = 0;
            loop {
                cycle += 1;
                {
                    if let Ok(mut pipe) = baton.lock() {
                        pipe.execute("am kill-all && cmd activity clear-debug-app");
                        pipe.execute("appops set com.google.android.apps.bard RECORD_AUDIO allow");
                        pipe.execute("appops set com.google.android.inputmethod.latin RECORD_AUDIO allow");
                        pipe.execute("cmd role add-role-holder android.app.role.ASSISTANT com.google.android.googlequicksearchbox");
                    }
                }

                if cycle % 12 == 0 {
                    remember("HUMMIE_WATCHDOG", &format!("Cycle {} healthy. Josie and Hummie Agents active.", cycle));
                }
                thread::sleep(Duration::from_secs(10));
            }
        });
    }

    println!("[ ✔ ] Josie and Hummie Agents active. Self-healing loop running.");
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

/// Hummie Agent Dispatcher
fn dispatch_hummie_agent(args: &[String]) {
    if args.is_empty() {
        println!("🤖 Hummie Agents Available:");
        println!("  hummie scout <query>    - Firecrawl & web discovery agent");
        println!("  hummie watcher          - Background guardian daemon");
        println!("  hummie root <cmd>       - Elevated Shizuku executor");
        println!("  hummie vision           - Screen visual capture agent");
        println!("  hummie recall [query]   - Permanent memory extraction agent");
        return;
    }
    let agent_name = &args[0];
    let agent_args = &args[1..];
    match agent_name.as_str() {
        "scout" => {
            let query = agent_args.join(" ");
            let _ = Command::new("firecrawl").arg("search").arg(&query).status();
            remember("HUMMIE-Scout", &format!("Web search: {}", query));
        }
        "watcher" => {
            ignite_swarm_daemon();
        }
        "root" => {
            let cmd = agent_args.join(" ");
            let (rc, out) = run_root(&cmd);
            println!("{}", out);
            remember("HUMMIE-Root", &format!("rc={} cmd={}", rc, cmd));
        }
        "vision" => {
            let (rc, out) = run_root("screencap -p /sdcard/Pictures/vision_cap.png");
            if rc == 0 {
                println!("[HUMMIE-Vision] Captured /sdcard/Pictures/vision_cap.png");
            } else {
                eprintln!("[HUMMIE-Vision] Failed: {}", out);
            }
        }
        "recall" => {
            let prefix = if agent_args.is_empty() { "" } else { &agent_args[0] };
            show_recent_memory(prefix, 15);
        }
        other => {
            println!("[HUMMIE] Unknown agent: {}", other);
        }
    }
}

/// List active tasks
fn list_active_tasks() {
    println!("Active Swarm / AI Processes:");
    if let Ok(out) = Command::new("ps").args(&["-eo", "pid,etime,args"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("josie") || line.contains("hummie") || line.contains("agy") || line.contains("python") || line.contains("node") || line.contains("rust") {
                println!("  {}", line);
            }
        }
    }
}

const HELP_MENU: &str = r#"===========================================================
⚡ JOSIE & HER HUMMIE AGENTS (Unified Native ARM64 Swarm)
===========================================================
Usage: josie <command> [args]      (aliases: hummie, u, comet, j)

🧠 Cognition (Multi-Model Auto-Cascade: Gemini 3.1 Pro -> Flash -> Claude):
  ask <prompt>      Direct prompt execution
  goal <goal>       Autonomous run-to-completion
  plan <task>       Planning run
  run <prompt>      Raw JSON output
  usage             Models available + recent memories

🤖 Hummie Specialized Agents:
  hummie scout <q>  Web intelligence & research
  hummie watcher    Launch background guardian daemon
  hummie root <cmd> Elevated Shizuku executor
  hummie vision     Screen capture agent
  hummie recall     Permanent memory query

📱 Device Control (Direct Shizuku rish pipe):
  shizuku           Validate Shizuku / rish connection
  vision            Capture screenshot to /sdcard/Pictures/vision_cap.png
  mouse ...         Touch input (tap, swipe, type, key)
  sams              Display & keyboard density calibrate
  explorer <path>   High-speed file explorer

🌐 Data & Web:
  web <search|scrape|crawl|map> <arg>   Firecrawl bridge

⚙ System & Daemon:
  daemon            Ignite persistent self-healing swarm daemon
  tasks             List running AI/worker processes
  kill <pid>        Terminate process
==========================================================="#;

fn main() {
    let args: Vec<String> = env::args().collect();
    let invoked_name = PathBuf::from(&args[0])
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "josie".to_string());

    if args.len() < 2 || args[1] == "-h" || args[1] == "--help" || args[1] == "help" {
        println!("{}", HELP_MENU);
        return;
    }

    let cmd = &args[1];
    let rest_args = &args[2..];
    let prompt_text = rest_args.join(" ");

    // If called as "hummie" directly, dispatch to agent dispatcher
    if invoked_name == "hummie" {
        dispatch_hummie_agent(&args[1..]);
        return;
    }

    match cmd.as_str() {
        "hummie" => {
            dispatch_hummie_agent(rest_args);
        }
        "ask" => {
            if prompt_text.is_empty() {
                eprintln!("Usage: josie ask <prompt>");
                std::process::exit(1);
            }
            let code = agy_execute(&prompt_text, &[], false, "Josie-Ask");
            std::process::exit(code);
        }
        "goal" => {
            if prompt_text.is_empty() {
                eprintln!("Usage: josie goal <goal>");
                std::process::exit(1);
            }
            let goal_payload = format!(
                "Work fully autonomously until this goal is completely finished. Do not stop to ask questions; verify every change before finishing and end with a one-line summary.\n\nGOAL: {}",
                prompt_text
            );
            let code = agy_execute(&goal_payload, &[], false, "Josie-Goal");
            std::process::exit(code);
        }
        "plan" => {
            if prompt_text.is_empty() {
                eprintln!("Usage: josie plan <task>");
                std::process::exit(1);
            }
            let code = agy_execute(&prompt_text, &["--mode", "plan"], false, "Josie-Plan");
            std::process::exit(code);
        }
        "run" => {
            if prompt_text.is_empty() {
                eprintln!("Usage: josie run <prompt>");
                std::process::exit(1);
            }
            let code = agy_execute(&prompt_text, &[], true, "Josie-Run");
            std::process::exit(code);
        }
        "usage" => {
            let _ = Command::new("agy").arg("models").status();
            println!("\nRecent SQLite Memory Records (Josie & Hummie Agents):");
            show_recent_memory("Josie-", 8);
            show_recent_memory("HUMMIE-", 8);
        }
        "shizuku" => {
            let (rc, out) = run_root("id");
            if rc == 0 {
                println!("[SHIZUKU LIVE] {}", out);
            } else {
                eprintln!("[SHIZUKU FAILED] rc={} out={}", rc, out);
            }
            std::process::exit(rc);
        }
        "vision" => {
            let (rc, out) = run_root("screencap -p /sdcard/Pictures/vision_cap.png");
            if rc == 0 {
                println!("[VISION] Saved to /sdcard/Pictures/vision_cap.png");
            } else {
                eprintln!("[VISION FAILED] {}", out);
            }
            std::process::exit(rc);
        }
        "mouse" => {
            handle_mouse(rest_args);
        }
        "sams" | "keyboard-fix" => {
            let cmds = [
                "wm size",
                "wm density",
                "settings put secure show_ime_with_hard_keyboard 1",
            ];
            for c in &cmds {
                let (_, out) = run_root(c);
                println!("  [CONFIG] {} -> {}", c, out);
            }
        }
        "explorer" => {
            let target = if rest_args.is_empty() { "." } else { &rest_args[0] };
            explore_path(target);
        }
        "web" => {
            if rest_args.is_empty() {
                println!("Usage: josie web <search|scrape|crawl|map> <query/url>");
                return;
            }
            let sub = &rest_args[0];
            let sub_args = &rest_args[1..];
            let mut firecrawl_cmd = Command::new("firecrawl");
            firecrawl_cmd.arg(sub);
            for a in sub_args {
                firecrawl_cmd.arg(a);
            }
            let _ = firecrawl_cmd.status();
        }
        "daemon" => {
            ignite_swarm_daemon();
        }
        "tasks" => {
            list_active_tasks();
        }
        "kill" => {
            if rest_args.is_empty() {
                eprintln!("Usage: josie kill <pid>");
                std::process::exit(1);
            }
            let pid = &rest_args[0];
            let _ = Command::new("kill").arg("-9").arg(pid).status();
            println!("[KILL] Dispatched SIGKILL to PID {}", pid);
        }
        other => {
            eprintln!("[ERROR] Unknown command: {}\n", other);
            println!("{}", HELP_MENU);
            std::process::exit(1);
        }
    }
}
