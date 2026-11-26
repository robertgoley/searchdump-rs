#!/usr/bin/env python3
"""Generate the split layout of searchdump-multi for steps <= N from the pristine file.
usage: split.py N OUTDIR   (writes OUTDIR/main.rs and module files)"""
import re, sys, os, json

STEP = int(sys.argv[1]); OUT = sys.argv[2]
# the pre-split source: release/1.4 at a4dc857 (L4-007), the last commit before the split
import subprocess
_root = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=os.path.dirname(os.path.abspath(__file__)), text=True).strip()
SRC = subprocess.check_output(['git', 'show', 'a4dc857:src/bin/searchdump-multi.rs'], cwd=_root, text=True).split('\n')
if SRC[-1] == '': SRC.pop()

KW = re.compile(r'^(pub(\([a-z]+\))? )?(async )?(fn|struct|enum|const|static|impl|mod|type|use)\b')
def chunks(lines, indent):
    pre = ' ' * indent
    starts = []; in_header = False
    for i, l in enumerate(lines):
        if not l.startswith(pre) or (len(l) > indent and l[indent] == ' ') or l.strip() == '':
            continue
        s = l[indent:]
        if s.startswith(('///', '//', '#[', '#![')):
            if not in_header:
                starts.append(i); in_header = True
        elif KW.match(s):
            if not in_header:
                starts.append(i)
            in_header = False
    out = []
    for k, s in enumerate(starts):
        e = starts[k + 1] if k + 1 < len(starts) else len(lines)
        body = lines[s:e]
        key = None
        for l in body:
            m = KW.match(l[indent:]) if l.startswith(pre) else None
            if m and not l[indent:].startswith(' '):
                t = l[indent:]
                m2 = re.match(r'^(?:pub(?:\([a-z]+\))? )?(?:async )?(fn|struct|enum|const|static|mod|type|use)\s+([A-Za-z0-9_:*{}]+)', t)
                if m2: key = m2.group(2).rstrip(':')
                else:
                    m3 = re.match(r'^impl(?:<[^>]*>)?\s+(?:([\w:]+)\s+for\s+)?([\w]+)', t)
                    key = 'impl ' + ((m3.group(1).split('::')[-1] + ' for ') if m3.group(1) else '') + m3.group(2)
                break
        out.append((key, body))
    return out

TEST_START = next(i for i, l in enumerate(SRC) if l == '#[cfg(test)]' and SRC[i+1] == 'mod tests {')
FIRST_ITEM = next(i for i, l in enumerate(SRC) if l.startswith('pub const MANIFEST'))
header = SRC[:FIRST_ITEM]            # docs, forbid, use lines
items = chunks(SRC[FIRST_ITEM:TEST_START], 0)
tests_body = SRC[TEST_START + 2:]
assert tests_body[-1] == '}'
tests_body = tests_body[:-1]
tchunks = chunks(tests_body, 4)

# (step, module, [item keys])
MODS = [
    (3, 'cli', ['Direction', 'Layout', 'Compression', 'impl Compression', 'Lifecycle', 'Order', 'MatchType', 'Args', 'impl Args']),
    (4, 'sizing', ['sizing']),
    (4, 'flags', ['DATA_FLAGS', 'flag_name', 'merge_data_flags', 'merge_data_flags_owned', 'LIFECYCLE_DROP', 'strings', 'cluster_flags']),
    (5, 'selection', ['Selector', 'impl Selector', 'TargetKind', 'Target', 'select_targets', 'sanitize_dir', 'check_index_name', 'rel_dir']),
    (5, 'types', ['TYPE_ORDER', 'CLUSTER_TYPES', 'Types', 'node_type_flags', 'resolve_types']),
    (6, 'files', ['CLUSTER_DIR', 'item_file', 'node_item_file', 'item_file_for', 'META_SUFFIXES', 'parse_data_file', 'rs_named', 'meta_file', 'read_text', 'data_parts', 'is_part', 'count_records', 'mappings_body', 'remove_data_files', 'file_name', 'nonempty', 'meta_dir']),
    (6, 'manifest', ['MANIFEST', 'IndexEntry', 'ClusterEntry', 'Manifest', 'impl Manifest']),
    (6, 'discover', ['Found', 'discover']),
    (7, 'tls', ['Secret', 'impl Debug for Secret', 'parse_secret', 'Side', 'ClusterTls', 'NODE_TLS_ENV', 'impl ClusterTls']),
    (7, 'cluster', ['split_url', 'PARENT_429_RETRIES', 'parent_call_retryable', 'parent_retry_delay', 'Cluster', 'impl Cluster']),
    (8, 'child/stats', ['CountSource', 'ChildStats', 'impl ChildStats', 'parse_stats_file', 'parse_total_writes', 'read_tail', 'child_stats']),
    (8, 'child/probe', ['takes_stats_file', 'accepts_server_limits', 'accepts_stats_file', 'find_bin']),
    (8, 'child/runner', ['CHILD_GRACE', 'Runner', 'ChildRun', 'ChildRef', 'stats_target_index', 'impl Runner', 'stop_child', 'send_term', 'tail']),
    (9, 'ctx', ['Outcome', 'Ctx', 'impl Ctx', 'Pacer', 'impl Pacer']),
    (9, 'planner', ['plan_sizes', 'note_size', 'limits_max_page', 'sizing_json']),
    (10, 'dump', ['debug_counts', 'dump_one', 'dump_cluster']),
    (10, 'load', ['load_cluster', 'load_one', 'with_part']),
    (10, 'storage', ['run', 'staging_dir', 'chrono_stamp']),
    (10, 'run_local/mod', ['run_local']),
]
TESTS = {
    'cli': [],
    'sizing': ['server_limits_off_reproduces_1_1_0_flags', 'page_limit_shrinks_for_large_documents', 'auto_plan_sizes_from_fixture_limits'],
    'flags': ['extra_flags_override_default_data_flags', 'run_control_flags_unchanged', 'split_flag_detection_and_cluster_flags'],
    'selection': ['selection_include_exclude_hidden', 'group_breakout', 'targets_by_index_alias_and_datastream'],
    'types': ['names_and_types', 'node_ignore_flags'],
    'files': ['mappings_body_shapes', 'record_counts_all_codecs', 'node_names_and_meta_dir'],
    'manifest': [],
    'discover': ['discover_layouts_and_manifest', 'split_part_names_and_discovery'],
    'tls': ['tls_args', 'l4_001_cluster_tls_flags_and_precedence', 'l4_001_passphrase_goes_to_children_only_through_the_environment'],
    'cluster': ['parent_retries_only_read_only_calls_on_429_and_503', 'credentials_order_and_redaction'],
    'child/stats': ['child_counts_from_stats_file_or_log_line'],
    'child/probe': ['stats_file_only_for_searchdump'],
    'child/runner': ['script_runner', 'cancel_stops_a_running_child_with_sigterm_then_sigkill', 'a_failed_child_reports_its_log_tail', 'many_children_do_not_starve_the_runtime', 'runner_returns_child_counts', 'l4_002_child_runs_carry_their_id'],
    'ctx': ['interval_spaces_out_starts'],
    'planner': [],
    'dump': [], 'load': ['a_failed_load_step_names_its_part_only_when_there_are_several'],
    'storage': [], 'run_local/mod': [],
}
DOC = {
    'cli': 'Command line: `Args` and its value enums.',
    'sizing': 'Data-flag sizing for children (`--serverLimits`, ROADMAP-1.2 L2-001..L2-005).',
    'flags': 'Child flag lists: default data flags, merging user flags, cluster-wide type flags.',
    'selection': 'Which indices, aliases and data streams are selected, and their directories.',
    'types': 'Item types (`--types` / `--ignoreType`) and the Node-compatible type flags.',
    'files': 'File naming (RS and Node layouts), split parts, record counts and the meta directory.',
    'manifest': 'The `multielasticdump.json` manifest.',
    'discover': 'Finding loadable indices and parts in a dump directory.',
    'tls': 'Cluster TLS options and secrets (L4-001), and how they reach children.',
    'cluster': 'The parent\'s own cluster client: listing, stats, probes and retries.',
    'child/stats': 'Document counts reported by children (stats file or log line).',
    'child/probe': 'Finding the searchdump binary and probing which flags it accepts.',
    'child/runner': 'Running child processes: logs, ids (L4-002), stop/cancel and runs file.',
    'ctx': 'Shared run context and start pacing.',
    'planner': 'Per-index size plans (`--serverLimits`) and their manifest records.',
    'dump': 'Dumping one index or the cluster-wide types.',
    'load': 'Loading one index (all its parts) or the cluster-wide types.',
    'storage': 'Remote storage URLs (s3://, b2://, sftp://, fs://) around a local run.',
    'run_local/mod': 'One dump, load or copy over a local directory: the orchestration.',
}

byname = {}
for k, b in items:
    assert k not in byname, k
    byname[k] = b
tbyname = {}
for k, b in tchunks:
    if k is None: continue
    tbyname.setdefault(k, b) if k not in ('super::*',) else None
    if k != 'super::*': tbyname[k] = b

active = [(s, m, ks) for s, m, ks in MODS if s <= STEP]
moved = set(k for _, _, ks in active for k in ks)
tmoved = set(t for _, m, _ in active for t in TESTS[m])
for k in moved: assert k in byname, ('missing item', k)
for k in tmoved: assert k in tbyname, ('missing test', k)

def vis(body, indent=0):
    """private top-level items (and fields of private structs) become pub(crate)."""
    out = []; in_priv_struct = False
    for l in body:
        if l.startswith(('fn ', 'async fn ', 'struct ', 'enum ', 'const ', 'static ', 'type ')):
            in_priv_struct = l.startswith('struct ') and l.rstrip().endswith('{')
            l = 'pub(crate) ' + l
        elif in_priv_struct:
            if l == '}': in_priv_struct = False
            elif re.match(r'^    [a-z_][a-z0-9_]*: ', l): l = '    pub(crate) ' + l[4:]
        out.append(l)
    return out

def dedent_mod(body):
    # `pub mod sizing {` ... `}` -> body one level out
    i = next(j for j, l in enumerate(body) if l.startswith('pub mod sizing {'))
    j = max(k for k, l in enumerate(body) if l == '}')
    docs = body[:i]
    inner = body[i + 1:j]
    res = []
    for l in inner:
        if l.startswith('    '): res.append(l[4:])
        elif l.strip() == '': res.append('')
        else: raise SystemExit('sizing: unexpected line ' + l)
    return docs, res

FIX = [
    ('cli', '    fn flags(self) -> &\'static [&\'static str] {', '    pub(crate) fn flags(self) -> &\'static [&\'static str] {'),
    ('cli', '    pub fn of_file(', '    // only the tests call it (a crate-root `pub` item was exempt from dead_code)\n    #[allow(dead_code)]\n    pub fn of_file('),
    ('ctx', '    fn plan(&self)', '    pub(crate) fn plan(&self)'),
    ('ctx', '    fn stats_of(&self', '    pub(crate) fn stats_of(&self'),
    ('cluster', '    child_base: String,', '    pub(crate) child_base: String,'),
]
def fix(m, body):
    txt = '\n'.join(body)
    for fm, a, b in FIX:
        if fm == m:
            assert a in txt, (m, a)
            txt = txt.replace(a, b)
    return txt.split('\n')

files = {}
mod_decls = []
for s, m, ks in active:
    if m == 'sizing':
        docs, inner = dedent_mod(byname['sizing'])
        body = ['//! ' + DOC[m], '//!'] + ['//!' + d[3:] if d.startswith('///') else d for d in docs] + inner
        # sizing items keep their `pub`; module-level `use super::` paths stay valid (super = crate root)
    else:
        body = ['//! ' + DOC[m], '', 'use crate::*;', '']
        for k in ks:
            body += vis(byname[k])
    while body and body[-1] == '': body.pop()
    ts = TESTS[m]
    if ts:
        body += ['', '#[cfg(test)]', 'mod tests {'] + ([] if m == 'sizing' else ['    use super::*;']) + ['']
        for t in ts:
            tb = [l if 'use super::*' in l else l.replace('super::', 'crate::') for l in tbyname[t]]
            while tb and tb[-1] == '': tb.pop()
            body += tb + ['']
        body.pop(); body.append('}')
    files[m] = fix(m, body)
    mod_decls.append(m)

# main.rs
main = list(header)
if STEP >= 2:
    main = [l.replace('use reqwest::Method;', 'use http::Method;') for l in main]
# module declarations after the use block
top = [m.split('/')[0] for m in mod_decls]
seen = []
for t in top:
    if t not in seen: seen.append(t)
decl = []
for t in seen:
    if t == 'sizing':
        decl.append('pub mod sizing;')
    else:
        decl.append(f'mod {t};')
for t in seen:
    if t != 'sizing':
        decl.append(f'use {t}::*;')
if decl:
    while main and main[-1] == '': main.pop()
    main += [''] + ['// split (L4-S03..): one module per concern; the glob imports are narrowed in step 15'] + decl + ['']
for k, b in items:
    if k not in moved:
        main += b
while main and main[-1] == '': main.pop()
rest_tests = [(k, b) for k, b in tchunks if k not in tmoved]
main += ['', '#[cfg(test)]', 'mod tests {'] if any(k not in (None, 'super::*') for k, _ in rest_tests) else []
if main[-1] == 'mod tests {':
    for k, b in rest_tests:
        main += b
    while main[-1] == '': main.pop()
    main.append('}')
files['main'] = main
if 'child/stats' in mod_decls:
    files['child/mod'] = ['//! Child `searchdump` processes: probing the binary, running it, reading its counts.', '',
                          'mod probe;', 'mod runner;', 'mod stats;', '', 'pub(crate) use probe::*;', 'pub(crate) use runner::*;', 'pub(crate) use stats::*;']
for m, body in files.items():
    p = os.path.join(OUT, m + '.rs')
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, 'w').write('\n'.join(body) + '\n')
print(json.dumps({m: len(b) for m, b in files.items()}))
