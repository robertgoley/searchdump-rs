#!/usr/bin/env python3
"""Split step 15 (L4-S15): drop the temporary glob imports and make the visibility explicit.
1. every `pub` item/field in the binary becomes `pub(crate)` (`pub mod sizing` -> `mod sizing`), and
   `#![warn(unreachable_pub)]` goes into main.rs;
2. main.rs loses `use x::*;`, modules lose `use crate::*;` and get the root's external `use` block;
3. then `cargo check --message-format=json` is run in a loop and each unresolved name gets the import
   rustc suggests (crate paths first), in the module (or its tests module) where it is missing;
4. `cargo fix --allow-dirty --bin searchdump-multi --tests` removes the unused imports (then cargo fmt).
usage: phase_c15.py   (from the worktree root)"""
import json, os, re, subprocess, sys
D = 'src/bin/searchdump-multi'
files = []
for root, _, fs in os.walk(D):
    for f in fs:
        if f.endswith('.rs'):
            files.append(os.path.join(root, f))
main = f'{D}/main.rs'
src = open(main).read()
ext_uses = [l for l in src.split('\n') if l.startswith('use ') and not re.match(r'use \w+::\*;$', l)]

def vis(text):
    text = re.sub(r'(?m)^(\s*)pub (?=(async fn|fn|struct|enum|const|static|type|trait|use|mod) )', r'\1pub(crate) ', text)
    text = re.sub(r'(?m)^(\s+)pub (?=[a-z_][a-z0-9_]*: )', r'\1pub(crate) ', text)
    text = re.sub(r'(?m)^(\s*)pub\(crate\) (fn|async fn) main\(', r'\1\2 main(', text)
    # tuple struct field: Secret(pub String)
    text = re.sub(r'\(pub (?=[A-Z])', '(pub(crate) ', text)
    return text

for f in files:
    t = open(f).read()
    t = vis(t)
    # moved tests used `crate::NAME` through the root globs: name it directly (imported below)
    t = re.sub(r'\bcrate::([A-Za-z_]\w*)\b(?!::)', r'\1', t)
    # tls tests: `use super::{ClusterTls, ..}` became `use crate::{..}` in the move; the items live in tls
    if f.endswith('/tls.rs'):
        t = t.replace('use crate::{', 'use super::{')
    if f == main:
        t = re.sub(r'(?m)^use \w+::\*;\n', '', t)
        t = t.replace('pub(crate) mod sizing;', 'mod sizing;')
        t = t.replace('#![forbid(unsafe_code)]\n', '#![forbid(unsafe_code)]\n#![warn(unreachable_pub)]\n', 1)
        t = t.replace('// split (L4-S03..): one module per concern; the glob imports are narrowed in step 15\n', '')
    elif 'use crate::*;\n' in t:
        t = t.replace('use crate::*;\n', '\n'.join(ext_uses) + '\n', 1)
    elif f.endswith('sizing.rs'):
        pass
    open(f, 'w').write(t)

def check():
    r = subprocess.run(['cargo', 'check', '--bin', 'searchdump-multi', '--tests', '--message-format=json', '-q'],
                       capture_output=True, text=True)
    msgs = []
    for line in r.stdout.split('\n'):
        try:
            m = json.loads(line)
        except Exception:
            continue
        if m.get('reason') == 'compiler-message' and m['message']['level'] == 'error':
            msgs.append(m['message'])
    return msgs

def suggestions(msg):
    out = []
    def walk(c):
        for s in c.get('spans', []):
            r = s.get('suggested_replacement')
            if r and r.strip().startswith('use '):
                out.append(r.strip())
        for ch in c.get('children', []):
            walk(ch)
    walk(msg)
    def rank(u):
        return (0 if u.startswith('use crate::') else 1 if u.startswith('use std::') else 2, len(u))
    return sorted(set(out), key=rank)

def resolve():
    for it in range(40):
        msgs = check()
        if not msgs:
            print('clean after', it, 'rounds'); break
        added = 0
        for m in msgs:
            sug = suggestions(m)
            if not sug:
                continue
            def our_spans(c):
                for x in c.get('spans', []):
                    if x['file_name'].startswith('src/'):
                        yield x
                for ch in c.get('children', []):
                    yield from our_spans(ch)
            cands = list(our_spans(m))
            sp = next((x for x in cands if x.get('is_primary')), cands[0] if cands else None)
            if sp is None:
                continue
            f, line = sp['file_name'], sp['line_start']
            lines = open(f).read().split('\n')
            # in a `mod tests {` block (after its line)? insert after its `use super::*;`
            tests_at = next((i for i, l in enumerate(lines) if l.startswith('mod tests {')), None)
            u = sug[0]
            if tests_at is not None and line - 1 > tests_at:
                k = next((i for i in range(tests_at, len(lines)) if lines[i].strip().startswith('use super::*;')), tests_at)
                ins = '    ' + u
            else:
                k = max(i for i, l in enumerate(lines[:tests_at] if tests_at else lines) if l.startswith('use ')) if any(l.startswith('use ') for l in lines) else 0
                ins = u
            if ins.strip() in (l.strip() for l in lines):
                continue
            lines.insert(k + 1, ins)
            open(f, 'w').write('\n'.join(lines))
            added += 1
        if not added:
            for m in msgs[:10]:
                print(m['rendered'])
            sys.exit('no suggestion to apply')
    else:
        sys.exit('did not converge')

def remove_unused(tests):
    """apply rustc's own removal suggestions for unused imports (cargo fix skips some of them)"""
    cmd = ['cargo', 'check', '--bin', 'searchdump-multi', '--message-format=json', '-q'] + (['--tests'] if tests else [])
    r = subprocess.run(cmd, capture_output=True, text=True)
    edits = {}
    for line in r.stdout.split('\n'):
        try:
            m = json.loads(line)
        except Exception:
            continue
        if m.get('reason') != 'compiler-message' or m['message'].get('code', {}) is None:
            continue
        msg = m['message']
        if (msg.get('code') or {}).get('code') != 'unused_imports':
            continue
        for sp in msg.get('spans', []):
            if sp['file_name'].startswith('src/'):
                edits.setdefault(sp['file_name'], set()).add((sp['byte_start'], sp['byte_end']))
    for f, es in edits.items():
        b = open(f, 'rb').read().decode()
        for (s0, e0) in sorted(es, reverse=True):
            ls = b.rfind('\n', 0, s0) + 1
            le = b.index('\n', e0)
            line = b[ls:le]
            if line.strip() == 'use ' + b[s0:e0] + ';':          # whole `use path;` line
                b = b[:ls] + b[le + 1:]
            elif b[s0 - 1:s0] == '{' and b[e0:e0 + 2] == '};':   # `use a::{B};`: the last name
                b = b[:ls] + b[le + 1:]
            elif b[e0:e0 + 2] == ', ':                            # `{A, B}`: drop `A, `
                b = b[:s0] + b[e0 + 2:]
            elif b[s0 - 2:s0] == ', ':                            # drop `, B`
                b = b[:s0 - 2] + b[e0:]
            else:
                raise SystemExit(f'cannot remove {b[s0:e0]!r} in {f}: {line!r}')
        # `use a::{B};` -> fine for rustfmt; nothing else to tidy
        open(f, 'w').write(b)
    return sum(len(v) for v in edits.values())

resolve()
# imports used only by a module's tests (through `use super::*`): removed for the binary, then re-added
# inside the tests module by resolve(); finally drop what the test build does not use either
for _ in range(5):
    n = remove_unused(False)
    resolve()
    n += remove_unused(True)
    resolve()
    if n == 0:
        break
subprocess.run(['cargo', 'fmt'], check=True)
# storage.rs: `Context` and `fs` are used only by the opendal (storage-URL) code
p = f'{D}/storage.rs'
t = open(p).read()
a = 'use anyhow::{Context, Result, bail};\nuse std::fs;\n'
assert a in t, 'storage.rs imports changed'
t = t.replace(a, 'use anyhow::{Result, bail};\n#[cfg(feature = "opendal")]\nuse anyhow::Context;\n#[cfg(feature = "opendal")]\nuse std::fs;\n')
open(p, 'w').write(t)
subprocess.run(['cargo', 'fmt'], check=True)
print('ok')
