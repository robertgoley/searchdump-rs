#!/usr/bin/env python3
"""Split step 14 (L4-S14): the child data-arg builders: flags::child_data_flags (plan flags merged with the
default/user data flags), dump::dump_data_args (codec, --fileSize/--maxRows, --pageBytes, size note, sizing
record, data flags; the --pageBytes landing point) and load::load_data_args (codec + data flags). The blocks
move verbatim out of dump_one/load_one. usage: phase_b14.py SRC_DIR"""
import sys, re
D = sys.argv[1]

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
print('ok')
