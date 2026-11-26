//! Running child processes: logs, ids (L4-002), stop/cancel and runs file.

use crate::*;

/// 1.1-8: how long a child gets after SIGTERM (on Ctrl-C) before it is killed.
pub const CHILD_GRACE: Duration = Duration::from_secs(5);

pub struct Runner {
    pub bin: PathBuf,
    /// L4-001: environment set for every child (the key passphrase; never on argv).
    pub child_env: Vec<(String, String)>,
    /// L4-001: variables removed from every child's environment (would override `child_env`).
    pub child_env_remove: Vec<String>,
    pub log_dir: PathBuf,
    pub common: Vec<String>,
    pub data_flags: Vec<String>,
    /// Set on Ctrl-C: no new step starts.
    pub cancelled: Arc<AtomicBool>,
    /// 1.1-8: cancelled on Ctrl-C together with `cancelled`; running children are stopped
    /// (SIGTERM, then SIGKILL after `grace`).
    pub cancel: CancellationToken,
    pub grace: Duration,
    /// 1.1-7: data runs get `--statsFile` (the binary is searchdump, see [`takes_stats_file`]).
    pub stats: bool,
    /// 1.2: data runs get `--serverLimits=off` (the child is searchdump 1.2+; the parent sized
    /// them already).
    pub server_limits: bool,
    /// L4-002: every child run of this process, in start order (see [`ChildRun`]).
    pub runs: Mutex<Vec<ChildRun>>,
}

/// L4-002: one child run. The `id` (a v4 UUID) is in the parent's start/end lines, in the
/// child's environment (`$SEARCHDUMP_CHILD_ID`: searchdump puts it on every log line and in its
/// `--statsFile`), in the manifest entry of the index and in `LOGDIR/children.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChildRun {
    pub id: String,
    pub index: String,
    /// The step: the type (`settings`, `data`, ...) or the data part's label.
    pub step: String,
    pub log: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Start time, seconds since the Unix epoch.
    pub started: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// running, ok, failed or interrupted.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<String>,
    /// The target index the child wrote and its uuid at the end (restores, from its stats).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_index: Option<serde_json::Value>,
}

/// L4-002: what the manifest keeps per index: each step's child id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChildRef {
    pub step: String,
    pub id: String,
    pub status: String,
}

/// L4-002: the `target_index` of a child's `--statsFile`, if any.
pub(crate) fn stats_target_index(stats_file: Option<&Path>) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(stats_file?).ok()?).ok()?;
    v.get("target_index").filter(|t| !t.is_null()).cloned()
}

impl Runner {
    /// L4-002: the child runs of `index`, for its manifest entry.
    pub fn refs_for(&self, index: &str) -> Vec<ChildRef> {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|r| r.index == index)
            .map(|r| ChildRef {
                step: r.step.clone(),
                id: r.id.clone(),
                status: r.status.clone(),
            })
            .collect()
    }

    /// L4-002: write every child run to `LOGDIR/children.json` (the run's report).
    pub fn save_runs(&self) -> Result<PathBuf> {
        let path = self.log_dir.join("children.json");
        let runs = self.runs.lock().unwrap_or_else(|e| e.into_inner()).clone();
        fs::write(&path, serde_json::to_vec_pretty(&runs)?)
            .with_context(|| format!("write {}", path.display()))?;
        Ok(path)
    }

    fn update_run(&self, id: &str, f: impl FnOnce(&mut ChildRun)) {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = runs.iter_mut().find(|r| r.id == id) {
            f(r);
        }
    }

    /// Run searchdump once; output goes to LOGDIR/INDEX.KIND.log.
    pub async fn run(
        &self,
        index: &str,
        kind: &str,
        input: &str,
        output: &str,
        extra: &[String],
    ) -> Result<ChildStats> {
        self.run_as(index, kind, kind, input, output, extra).await
    }

    /// Like `run`, logging to LOGDIR/INDEX.LABEL.log (one log per data part).
    ///
    /// 1.1-8: the child is a `tokio::process` child, awaited without holding a runtime thread.
    /// On cancel it gets SIGTERM, then SIGKILL after `grace`; a child still running when its
    /// future is dropped is killed (`kill_on_drop`).
    ///
    /// 1.1-7: returns the counts the child reported ([`child_stats`]).
    pub async fn run_as(
        &self,
        index: &str,
        kind: &str,
        label: &str,
        input: &str,
        output: &str,
        extra: &[String],
    ) -> Result<ChildStats> {
        if self.cancelled.load(Ordering::SeqCst) || self.cancel.is_cancelled() {
            bail!("interrupted");
        }
        let log_path = self.log_dir.join(format!("{index}.{label}.log"));
        let log = fs::File::create(&log_path)
            .with_context(|| format!("create {}", log_path.display()))?;
        let stats_path = (self.stats && kind == "data")
            .then(|| self.log_dir.join(format!("{index}.{label}.stats.json")));
        if let Some(p) = &stats_path {
            // never read the counts of an earlier run
            let _ = fs::remove_file(p);
        }
        // L4-002: this child's run id
        let id = uuid::Uuid::new_v4().to_string();
        let started_at = Instant::now();
        let mut child = {
            let mut cmd = tokio::process::Command::new(&self.bin);
            cmd.env(searchdump_rs::CHILD_ID_ENV, &id);
            cmd.arg(format!("--input={input}"))
                .arg(format!("--output={output}"))
                .arg(format!("--type={kind}"))
                .arg("--overwrite")
                .args(&self.common)
                .args(extra)
                .args(
                    stats_path
                        .iter()
                        .map(|p| format!("--statsFile={}", p.display())),
                )
                .envs(self.child_env.iter().map(|(k, v)| (k, v)))
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .kill_on_drop(true);
            // 1.1-8: its own process group, so a terminal Ctrl-C reaches only this process.
            // The child then gets exactly one signal (our SIGTERM below); a SIGINT from the
            // terminal followed by our SIGTERM would be its second signal, a force-quit that
            // skips finishing in-flight requests and cleaning up staged output.
            for k in &self.child_env_remove {
                cmd.env_remove(k);
            }
            #[cfg(unix)]
            cmd.process_group(0);
            cmd.spawn()
                .with_context(|| format!("could not start {} (child {id})", self.bin.display()))?
        };
        let pid = child.id();
        eprintln!(
            "child {id} start: {index} {label} (pid {}, log {})",
            pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
            log_path.display()
        );
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ChildRun {
                id: id.clone(),
                index: index.to_string(),
                step: label.to_string(),
                log: log_path.display().to_string(),
                pid,
                started: chrono_stamp(),
                status: "running".into(),
                ..Default::default()
            });
        let end = |status: &str, exit: String| {
            let secs = started_at.elapsed().as_secs_f64();
            eprintln!("child {id} end: {index} {label} {status} ({exit}) in {secs:.1}s");
            self.update_run(&id, |r| {
                r.status = status.to_string();
                r.exit = Some(exit.clone());
                r.seconds = Some((secs * 10.0).round() / 10.0);
                r.target_index = stats_target_index(stats_path.as_deref());
            });
        };
        let waited = tokio::select! {
            s = child.wait() => Some(s),
            _ = self.cancel.cancelled() => None,
        };
        let status = match waited {
            Some(s) => match s {
                Ok(s) => s,
                Err(e) => {
                    end("failed", format!("wait: {e}"));
                    return Err(anyhow!(e))
                        .with_context(|| format!("wait for {} (child {id})", self.bin.display()));
                }
            },
            None => {
                let how = stop_child(&mut child, self.grace).await;
                end("interrupted", how.clone());
                eprintln!(
                    "interrupted: stopped searchdump --type={kind} for {index} ({how}, child {id}); log: {}",
                    log_path.display()
                );
                bail!(
                    "interrupted: searchdump --type={kind} was stopped ({how}, child {id}); last lines of {}:\n{}",
                    log_path.display(),
                    tail(&log_path, 15)
                );
            }
        };
        if !status.success() {
            end("failed", status.to_string());
            bail!(
                "searchdump --type={kind} failed ({status}, child {id}); last lines of {}:\n{}",
                log_path.display(),
                tail(&log_path, 15)
            );
        }
        end("ok", status.to_string());
        Ok(child_stats(stats_path.as_deref(), &log_path))
    }
}

/// 1.1-8: stop a running child: SIGTERM (searchdump then finishes its in-flight requests and
/// exits), SIGKILL when it is still running after `grace`. Says what happened.
pub(crate) async fn stop_child(child: &mut tokio::process::Child, grace: Duration) -> String {
    if let Ok(Some(status)) = child.try_wait() {
        return format!("it had already exited, {status}");
    }
    if let Some(pid) = child.id() {
        send_term(pid).await;
    }
    match tokio::time::timeout(grace, child.wait()).await {
        Ok(Ok(status)) => format!("SIGTERM, {status}"),
        Ok(Err(e)) => format!("SIGTERM, then waiting failed: {e}"),
        Err(_) => {
            let _ = child.start_kill();
            match child.wait().await {
                Ok(status) => format!("SIGKILL after {grace:?} without exiting, {status}"),
                Err(e) => format!("SIGKILL after {grace:?}, then waiting failed: {e}"),
            }
        }
    }
}

/// SIGTERM to `pid` through `kill(1)`: this binary forbids unsafe code, so no `libc::kill`.
/// Elsewhere there is no SIGTERM; the child is killed once the grace period ends.
pub(crate) async fn send_term(pid: u32) {
    #[cfg(unix)]
    {
        let _ = tokio::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(not(unix))]
    let _ = pid;
}

pub(crate) fn tail(path: &Path, n: usize) -> String {
    let Ok(f) = fs::File::open(path) else {
        return String::new();
    };
    let lines: Vec<String> = BufReader::new(f).lines().map_while(Result::ok).collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.1-8: a Runner whose "searchdump" is a shell script.
    #[cfg(unix)]
    fn script_runner(dir: &Path, body: &str, grace: Duration) -> Runner {
        use std::os::unix::fs::PermissionsExt;
        // a new file each time: never rewrite a script a child may still be executing
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let bin = dir.join(format!("fake-searchdump-{n}.sh"));
        fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Runner {
            runs: Mutex::new(vec![]),
            child_env: vec![],
            child_env_remove: vec![],
            bin,
            log_dir: dir.to_path_buf(),
            common: vec![],
            data_flags: vec![],
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
            grace,
            stats: false,
            server_limits: false,
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_stops_a_running_child_with_sigterm_then_sigkill() {
        let d = tempfile::tempdir().unwrap();
        // exits on SIGTERM
        let r = Arc::new(script_runner(
            d.path(),
            "exec sleep 30",
            Duration::from_secs(5),
        ));
        let started = Instant::now();
        let task = {
            let r = r.clone();
            tokio::spawn(async move { r.run("idx", "data", "in", "out", &[]).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.cancel.cancel();
        let err = tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .expect("stopped within the grace period")
            .unwrap()
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("interrupted") && msg.contains("SIGTERM"),
            "{msg}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
        // ignores SIGTERM: killed once the (short) grace period ends
        let r = Arc::new(script_runner(
            d.path(),
            "trap '' TERM\nexec sleep 30",
            Duration::from_millis(300),
        ));
        let task = {
            let r = r.clone();
            tokio::spawn(async move { r.run("idx", "data", "in", "out", &[]).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        r.cancel.cancel();
        let err = tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .expect("killed after the grace period")
            .unwrap()
            .unwrap_err();
        assert!(format!("{err:#}").contains("SIGKILL"), "{err:#}");
        // a cancelled runner starts nothing new
        let err = r.run("idx", "data", "in", "out", &[]).await.unwrap_err();
        assert_eq!(format!("{err:#}"), "interrupted");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_child_reports_its_log_tail() {
        let d = tempfile::tempdir().unwrap();
        let r = script_runner(
            d.path(),
            "echo \"args: $*\"\necho boom-line >&2\nexit 3",
            CHILD_GRACE,
        );
        let err = r
            .run("idx", "mapping", "http://in/idx", "/tmp/out.json", &[])
            .await
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("searchdump --type=mapping failed"), "{msg}");
        assert!(msg.contains("boom-line"), "{msg}");
        assert!(
            msg.contains("--type=mapping") && msg.contains("--overwrite"),
            "{msg}"
        );
        assert!(d.path().join("idx.mapping.log").is_file());
        r.run("idx", "data", "in", "out", &[]).await.unwrap_err();
        // success
        let ok = script_runner(d.path(), "exit 0", CHILD_GRACE);
        ok.run("idx", "data", "in", "out", &[]).await.unwrap();
    }

    /// 1.1-8: 16 sleeping children on a 2-thread runtime do not starve other tasks (with
    /// `block_in_place` every child held a runtime thread).
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn many_children_do_not_starve_the_runtime() {
        let d = tempfile::tempdir().unwrap();
        let r = Arc::new(script_runner(d.path(), "exec sleep 1", CHILD_GRACE));
        let ticks = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let ticker = {
            let ticks = ticks.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    ticks.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        let mut set = tokio::task::JoinSet::new();
        for i in 0..16 {
            let r = r.clone();
            set.spawn(async move { r.run(&format!("i{i}"), "data", "in", "out", &[]).await });
        }
        let started = Instant::now();
        while let Some(j) = set.join_next().await {
            j.unwrap().unwrap();
        }
        let took = started.elapsed();
        ticker.abort();
        assert!(
            took < Duration::from_secs(5),
            "children ran one after another: {took:?}"
        );
        let n = ticks.load(Ordering::Relaxed);
        assert!(n >= 10, "the ticker ran {n} times in {took:?}");
    }

    /// 1.1-7: the runner passes --statsFile to data runs of searchdump and returns its counts;
    /// other runs (and a binary that is not searchdump) fall back to the log line.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runner_returns_child_counts() {
        let d = tempfile::tempdir().unwrap();
        let script = r#"for a in "$@"; do
  case "$a" in
    --statsFile=*) printf '{"complete":true,"written":7,"failed":1}' > "${a#--statsFile=}" ;;
  esac
done
echo "Total Writes: 5""#;
        let mut r = script_runner(d.path(), script, CHILD_GRACE);
        r.stats = true;
        let s = r.run("idx", "data", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.failed), (Some(7), 1));
        assert_eq!(s.source, Some(CountSource::StatsFile));
        // not a data run: no --statsFile, the log line
        let s = r.run("idx", "mapping", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.source), (Some(5), Some(CountSource::LogLine)));
        r.stats = false;
        let s = r.run("idx", "data", "in", "out", &[]).await.unwrap();
        assert_eq!((s.written, s.source), (Some(5), Some(CountSource::LogLine)));
    }

    /// L4-002: each child gets its own id in `$SEARCHDUMP_CHILD_ID`; the run is recorded with
    /// it (status, exit, target index from its stats file), failures name it, and the manifest
    /// refs and `children.json` carry it. L4-001: the passphrase reaches the child only through
    /// its environment, and the overriding variables are removed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn l4_002_child_runs_carry_their_id() {
        let d = tempfile::tempdir().unwrap();
        let script = r#"echo "id=$SEARCHDUMP_CHILD_ID pass=${SEARCHDUMP_OUTPUT_TLS_PASS:-none} shared=${SEARCHDUMP_TLS_PASS:-unset} args=$*"
for a in "$@"; do
  case "$a" in
    --statsFile=*) printf '{"complete":true,"written":3,"failed":0,"target_index":{"name":"t","uuid_final":"U1"}}' > "${a#--statsFile=}" ;;
    --type=mapping) exit 3 ;;
  esac
done
echo "Total Writes: 3""#;
        let mut r = script_runner(d.path(), script, CHILD_GRACE);
        r.stats = true;
        r.child_env = vec![(
            "SEARCHDUMP_OUTPUT_TLS_PASS".to_string(),
            "FAKE-pp-child".to_string(),
        )];
        r.child_env_remove = vec!["SEARCHDUMP_TLS_PASS".to_string()];
        r.run("idx", "data", "in", "out", &[]).await.unwrap();
        let e = r
            .run("idx", "mapping", "in", "out", &[])
            .await
            .unwrap_err()
            .to_string();
        let runs = r.runs.lock().unwrap().clone();
        assert_eq!(runs.len(), 2);
        let (ok, bad) = (&runs[0], &runs[1]);
        assert_eq!(uuid::Uuid::parse_str(&ok.id).unwrap().get_version_num(), 4);
        assert_ne!(ok.id, bad.id);
        assert_eq!((ok.status.as_str(), bad.status.as_str()), ("ok", "failed"));
        assert_eq!(ok.target_index.as_ref().unwrap()["uuid_final"], "U1");
        assert!(bad.target_index.is_none());
        assert!(e.contains(&format!("child {}", bad.id)), "{e}");
        let log = fs::read_to_string(&ok.log).unwrap();
        assert!(log.contains(&format!("id={}", ok.id)), "{log}");
        assert!(log.contains("pass=FAKE-pp-child"), "{log}");
        assert!(log.contains("shared=unset"), "{log}");
        assert!(
            !log.split("args=").nth(1).unwrap().contains("FAKE"),
            "{log}"
        );
        let refs = r.refs_for("idx");
        assert_eq!(refs.len(), 2);
        assert_eq!(
            (refs[0].step.as_str(), refs[0].id.as_str()),
            ("data", ok.id.as_str())
        );
        assert!(r.refs_for("other").is_empty());
        let report = r.save_runs().unwrap();
        let back: Vec<ChildRun> =
            serde_json::from_str(&fs::read_to_string(report).unwrap()).unwrap();
        assert_eq!(back, runs);
        assert!(!serde_json::to_string(&back).unwrap().contains("FAKE"));
    }
}
