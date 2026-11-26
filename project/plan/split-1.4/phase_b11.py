#!/usr/bin/env python3
"""Split step 11 (L4-S11): run_local/setup.rs + Ctx::new, cut from run_local/mod.rs by text anchors.
The moved blocks are verbatim except `&args` -> `args` (args is `&Args` in the helpers) and the
Ctrl-C locals becoming `Interrupt` fields. usage: phase_b11.py SRC_DIR (src/bin/searchdump-multi)"""
import sys, re
D = sys.argv[1]
p = f'{D}/run_local/mod.rs'
s = open(p).read()

TESTS = r'''
#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Args {
        let mut v = vec!["m", "--input=http://h:9200", "--output=/tmp/x"];
        v.extend_from_slice(extra);
        Args::try_parse_from(v).unwrap()
    }

    #[test]
    fn validate_local_messages() {
        let err = |a: &[&str]| format!("{:#}", validate_local(&args(a)).unwrap_err());
        assert!(validate_local(&args(&[])).is_ok());
        assert_eq!(
            err(&["--direction=load", "--maxRows=5"]),
            "--fileSize/--maxRows apply to dumps; load finds split parts on its own"
        );
        assert_eq!(
            err(&["--direction=load", "--pageBytes=1mb"]),
            "--pageBytes applies to dumps (it caps the search pages read from the cluster)"
        );
        assert_eq!(
            err(&["--direction=load", "--nodeCompat"]),
            "--nodeCompat applies to dumps; load reads Node-named directories on its own"
        );
        assert!(err(&["--nodeCompat", "--layout=index"]).starts_with("--nodeCompat needs --layout flat"));
        assert!(err(&["--nodeCompat", "--compression=zstd3"]).contains("not zstd"));
        assert!(err(&["--nodeCompat", "--fileSize=1mb"]).starts_with("--nodeCompat cannot split data"));
        assert!(err(&["--nodeCompat", "--matchType=datastream"]).contains("data streams"));
    }

    #[test]
    fn node_compat_moves_the_template_type_to_the_index() {
        let t = run_types(&args(&["--nodeCompat"])).unwrap();
        assert_eq!(t.index, vec!["template", "settings", "mapping", "data"]);
        assert!(t.cluster.is_empty());
        let t = run_types(&args(&[])).unwrap();
        assert_eq!(t.index, vec!["settings", "mapping", "data"]);
    }
}
'''

def cut(a, b):
    i = s.index(a); j = s.index(b, i)
    return s[i:j]

def body(block, ref=True):
    # one indentation level is kept (fn body); `&args` -> `args` for args: &Args
    if ref:
        block = re.sub(r'&args(?![\w.])', 'args', block)
    return block.rstrip('\n') + '\n'

A = cut('    if args.direction == Direction::Load && (args.file_size', '    let mut default_types')
B = cut('    let mut default_types', '    let sel = Selector::new')
C = cut('    let sel = Selector::new', '    let creds_in')
Dd = cut('    let creds_in', '    let log_dir = match')
E = cut('    let log_dir = match', '    // L4-001: the cluster\'s TLS')
F = cut('    // L4-001: the cluster\'s TLS', '    let mut common = tls.child_flags(side);')
G = cut('    let mut common = tls.child_flags(side);', '    let cancelled = Arc::new')
H = cut('    let cancelled = Arc::new', '    let bin = find_bin')
I = cut('    let bin = find_bin', '    let pacer = Pacer::new')
J = cut('    let pacer = Pacer::new', '    let started = Instant::now();')

# sanity: the blocks are contiguous and cover the head of run_local
head_start = s.index(A); head_end = s.index(J) + len(J)
assert s[head_start:head_end] == A + B + C + Dd + E + F + G + H + I + J

E2 = body(E).replace('    let log_dir = match', '    match', 1)
assert E2.rstrip().endswith('    };')
E2 = E2.rstrip()[:-1] + '\n'
H2 = H.replace('    let cancelled = Arc::new(AtomicBool::new(false));\n    let cancel = CancellationToken::new();\n', '')
assert H2 != H
H2 = H2.replace('        let c = cancelled.clone();\n        let token = cancel.clone();\n',
                '        let c = interrupt.cancelled.clone();\n        let token = interrupt.cancel.clone();\n')
assert 'interrupt.cancel.clone()' in H2
I2 = I.replace('        cancelled: cancelled.clone(),\n        cancel: cancel.clone(),\n',
               '        cancelled: interrupt.cancelled.clone(),\n        cancel: interrupt.cancel.clone(),\n')
assert I2 != I
I2 = I2.replace('    let runner = Runner {', '    Ok(Runner {').rstrip('\n')
assert I2.endswith('    };')
I2 = I2[:-len('    };')] + '    })\n'
J2 = J.replace('    let ctx = Arc::new(Ctx {', '    Ctx {').rstrip('\n')
assert J2.endswith('    });')
J2 = J2[:-len('    });')] + '    }\n'

setup = f'''//! `run_local` set-up (split step 11): validation, types, selection, log directory, TLS, Ctrl-C and the
//! child runner, in the order `run_local` has always run them.

use crate::*;

/// Usage errors of a local run (exit 2 through `main`), checked before anything else.
pub(crate) fn validate_local(args: &Args) -> Result<()> {{
{body(A)}    Ok(())
}}

/// The per-index and cluster-wide types of this run (defaults per direction, Node type flags, the
/// `--nodeCompat` template shuffle and its warnings).
pub(crate) fn run_types(args: &Args) -> Result<Types> {{
{body(B)}    Ok(types)
}}

/// `--match` / `--exclude` / `--includeHidden`, and the `--layout group` capture check.
pub(crate) fn selector(args: &Args) -> Result<Selector> {{
{body(C)}    Ok(sel)
}}

/// The warning for a password in a cluster URL (it reaches every child's command line).
pub(crate) fn warn_url_credentials(args: &Args) {{
{body(Dd).rstrip(chr(10))}
}}

/// `--logDir`, or a new name under the temp directory.
pub(crate) fn log_dir(args: &Args) -> PathBuf {{
{E2}}}

/// L4-001: the cluster side and its TLS settings (with the NODE_TLS warning and the passphrase
/// registered for scrubbing).
pub(crate) fn cluster_tls(args: &Args) -> Result<(Side, ClusterTls)> {{
{body(F)}    Ok((side, tls))
}}

/// 1.1-8: Ctrl-C sets `cancelled` and cancels the token (running children get SIGTERM, then
/// SIGKILL after the grace time; no new ones start).
pub(crate) struct Interrupt {{
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) cancel: CancellationToken,
}}

impl Interrupt {{
    /// Installs the Ctrl-C task. It runs before the binary probes, as it always has.
    pub(crate) fn install() -> Self {{
        let interrupt = Interrupt {{
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
        }};
{re.sub(r"(?m)^(?=.)", "    ", H2).rstrip(chr(10))}
        interrupt
    }}

    pub(crate) fn cancelled(&self) -> bool {{
        self.cancelled.load(Ordering::SeqCst)
    }}
}}

/// The child runner: TLS/auth flags, default data flags, the searchdump binary and what it accepts.
pub(crate) fn build_runner(
    args: &Args,
    tls: &ClusterTls,
    side: Side,
    log_dir: &Path,
    interrupt: &Interrupt,
) -> Result<Runner> {{
{body(G)}{body(I2)}}}
'''
# build_runner: `log_dir.clone()` on &Path -> to_path_buf; keep text otherwise
setup = setup.replace('        log_dir: log_dir.clone(),', '        log_dir: log_dir.to_path_buf(),')
setup += TESTS
open(f'{D}/run_local/setup.rs', 'w').write(setup)

new_head = '''    setup::validate_local(&args)?;
    let types = setup::run_types(&args)?;
    let sel = setup::selector(&args)?;
    setup::warn_url_credentials(&args);
    let log_dir = setup::log_dir(&args);
    let (side, tls) = setup::cluster_tls(&args)?;
    let interrupt = setup::Interrupt::install();
    let runner = setup::build_runner(&args, &tls, side, &log_dir, &interrupt)?;
    let ctx = Arc::new(Ctx::new(args, tls, types, sel, runner));
'''
s = s[:head_start] + new_head + s[head_end:]
old_tail = '    if cancelled.load(Ordering::SeqCst) {\n        return Ok(130);\n    }\n'
assert old_tail in s
s = s.replace(old_tail, '    if interrupt.cancelled() {\n        return Ok(130);\n    }\n')
s = s.replace('use crate::*;\n', 'use crate::*;\n\nmod setup;\n', 1)
open(p, 'w').write(s)

# Ctx::new in ctx.rs
cp = f'{D}/ctx.rs'
c = open(cp).read()
anchor = 'impl Ctx {\n'
assert anchor in c
ctx_new = f'''impl Ctx {{
    /// The run context: `--interval` pacing and the meta directory are derived from `args`.
    pub(crate) fn new(
        args: Args,
        tls: ClusterTls,
        types: Types,
        sel: Selector,
        runner: Runner,
    ) -> Ctx {{
{re.sub(r"(?m)^(?=.)", "    ", J2).rstrip(chr(10))}
    }}

'''
c = c.replace(anchor, ctx_new, 1)
open(cp, 'w').write(c)
print('ok')
