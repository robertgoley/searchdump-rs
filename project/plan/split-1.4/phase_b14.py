#!/usr/bin/env python3
"""Split step 14 (L4-S14): the child data-arg builders: flags::child_data_flags (plan flags merged with the
default/user data flags), dump::dump_data_args (codec, --fileSize/--maxRows, --pageBytes, size note, sizing
record, data flags; the --pageBytes landing point) and load::load_data_args (codec + data flags). The blocks
move verbatim out of dump_one/load_one. usage: phase_b14.py SRC_DIR"""
import sys, re
D = sys.argv[1]

TESTS = r'''
#[cfg(test)]
mod tests {
    use super::*;

    /// A run context without a cluster: `argv` after the URLs, the data flags as run_local builds them.
    pub(crate) fn test_ctx(argv: &[&str], server_limits: bool) -> Ctx {
        let mut v = vec!["m", "--input=http://h:9200", "--output=/tmp/x"];
        v.extend_from_slice(argv);
        let args = Args::try_parse_from(v).unwrap();
        let data_flags = merge_data_flags(
            if args.no_default_data_flags {
                &[]
            } else {
                DATA_FLAGS
            },
            &args.extra,
        );
        let runner = Runner {
            runs: Mutex::new(vec![]),
            child_env: vec![],
            child_env_remove: vec![],
            bin: PathBuf::from("searchdump"),
            log_dir: std::env::temp_dir(),
            common: vec![],
            data_flags,
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel: CancellationToken::new(),
            grace: CHILD_GRACE,
            stats: server_limits,
            server_limits,
        };
        let sel = Selector::new("^.*$", None, false).unwrap();
        Ctx::new(args, ClusterTls::default(), Types::default(), sel, runner)
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// L4-S14: the dump data step's exact argv (codec, splits, --pageBytes, plan flags, user flags).
    #[test]
    fn dump_data_args_are_pinned() {
        let mut e = IndexEntry::default();
        let c = test_ctx(&["--fileSize=10mb", "--pageBytes=2mb", "--", "--limit=50"], true);
        let got = dump_data_args(&c, "ix", Compression::Gzip, &mut e);
        assert_eq!(got, s(&["--fsCompress", "--fileSize=10mb", "--pageBytes=2000000", "--maxBulkBytes=16mb", "--serverLimits=off", "--scrollTime=10m", "--retryAttempts=5", "--retryDelay=2000", "--timeout=300000", "--limit=50"]));
        assert!(e.sizing.is_some(), "the plan is recorded in the manifest entry");
        // a non-searchdump child: no --pageBytes, no server-limit flags
        let c = test_ctx(&["--maxRows=7", "--pageBytes=2mb"], false);
        let got = dump_data_args(&c, "ix", Compression::Zstd3, &mut e);
        assert_eq!(got, s(&["--compressionType=zstd", "--compressionLevel=3", "--maxRows=7", "--limit=1000", "--maxBulkBytes=16mb", "--scrollTime=10m", "--retryAttempts=5", "--retryDelay=2000", "--timeout=300000"]));
        // --noDefaultDataFlags
        let c = test_ctx(&["--noDefaultDataFlags"], true);
        let got = dump_data_args(&c, "ix", Compression::None, &mut e);
        assert_eq!(got, s(&["--serverLimits=off"]));
    }

    /// L4-S14: the load data step's exact argv (codec and the load plan's flags + user flags).
    #[test]
    fn load_data_args_are_pinned() {
        let c = test_ctx(&["--direction=load"], true);
        assert_eq!(load_data_args(&c, Compression::Gzip), s(&["--fsCompress", "--limit=1000", "--maxBulkBytes=16mb", "--serverLimits=off", "--scrollTime=10m", "--retryAttempts=5", "--retryDelay=2000", "--timeout=300000"]));
        let c = test_ctx(&["--direction=load", "--", "--limit=9"], false);
        assert_eq!(load_data_args(&c, Compression::None), s(&["--maxBulkBytes=16mb", "--scrollTime=10m", "--retryAttempts=5", "--retryDelay=2000", "--timeout=300000", "--limit=9"]));
    }
}
'''

def dedent(block, n):
    out = []
    for l in block.split('\n'):
        if l.strip() == '': out.append('')
        else:
            assert l.startswith(' ' * n), repr(l); out.append(l[n:])
    return '\n'.join(out)

MERGE_OLD_D = '''extra.extend(merge_data_flags_owned(
                    &ctx.plan().flags(&size, ctx.runner.server_limits),
                    &ctx.runner.data_flags,
                ));'''
MERGE_OLD_L = '''extra.extend(merge_data_flags_owned(
                        &ctx.plan().flags(&size, ctx.runner.server_limits),
                        &ctx.runner.data_flags,
                    ));'''

# dump.rs
p = f'{D}/dump.rs'; s = open(p).read()
a = '                let mut extra = strings(comp.flags());\n'
i = s.index(a); j = s.index(MERGE_OLD_D, i) + len(MERGE_OLD_D) + 1
block = s[i:j]
s = s[:i] + '                let extra = dump_data_args(ctx, index, comp, &mut entry);\n' + s[j:]
block = dedent(block, 12).replace(MERGE_OLD_D.replace('\n            ', '\n'), 'extra.extend(child_data_flags(ctx.plan(), &size, &ctx.runner));')
assert 'child_data_flags' in block, block
s = s.rstrip('\n') + f'''

/// The data step's arguments for one index: the codec, `--fileSize`/`--maxRows`, `--pageBytes` (L4-003,
/// searchdump children only), and the data flags from this index's size plan merged with the
/// default/user flags. Prints the size note and records the plan in the manifest entry.
pub(crate) fn dump_data_args(
    ctx: &Ctx,
    index: &str,
    comp: Compression,
    entry: &mut IndexEntry,
) -> Vec<String> {{
{block.rstrip()}
    extra
}}
'''
open(p, 'w').write(s)

# load.rs
p = f'{D}/load.rs'; s = open(p).read()
a = '                    let mut extra = strings(c.flags());\n'
i = s.index(a); j = s.index(MERGE_OLD_L, i) + len(MERGE_OLD_L) + 1
s = s[:i] + '                    let mut extra = load_data_args(ctx, c);\n' + s[j:]
fn = '''
/// The data step's arguments for one part: its codec and the load plan's data flags merged with the
/// default/user flags.
pub(crate) fn load_data_args(ctx: &Ctx, c: Compression) -> Vec<String> {
    let mut extra = strings(c.flags());
    let size = ctx.plan().load();
    extra.extend(child_data_flags(ctx.plan(), &size, &ctx.runner));
    extra
}
'''
k = s.index('#[cfg(test)]\nmod tests {')
s = s[:k] + fn.lstrip('\n') + '\n' + s[k:]
open(p, 'w').write(s)

# flags.rs
p = f'{D}/flags.rs'; s = open(p).read()
fn = '''/// 1.2: a data child's sizing flags from `plan` for `size` (only the flags `runner`'s binary accepts),
/// followed by the default/user data flags, the later ones winning.
pub(crate) fn child_data_flags(
    plan: &sizing::Plan,
    size: &sizing::IndexSize,
    runner: &Runner,
) -> Vec<String> {
    merge_data_flags_owned(&plan.flags(size, runner.server_limits), &runner.data_flags)
}

'''
k = s.index('#[cfg(test)]\nmod tests {')
s = s[:k] + fn + s[k:]
open(p, 'w').write(s)
# argv-pinning tests (values recorded from the moved code, which is verbatim)
p = f'{D}/dump.rs'
open(p, 'a').write('\n' + TESTS)
print('ok')
