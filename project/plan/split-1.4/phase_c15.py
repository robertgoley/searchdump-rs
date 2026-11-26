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
            if not sug or not m['spans']:
                continue
            sp = m['spans'][0]
            f, line = sp['file_name'], sp['line_start']
            lines = open(f).read().split('\n')
            # in a `mod tests {` block (after its line)? insert after its `use super::*;`
            tests_at = next((i for i, l in enumerate(lines) if l.startswith('mod tests {')), None)
            u = sug[0]
            if tests_at is not None and line - 1 > tests_at:
                k = next(i for i in range(tests_at, len(lines)) if lines[i].strip().startswith('use super::*;'))
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

resolve()
# imports used only by a module's tests (through `use super::*`) are dropped by the non-test fix and
# re-added inside the tests module by the second resolve
subprocess.run(['cargo', 'fix', '--allow-dirty', '--allow-staged', '--bin', 'searchdump-multi', '-q'], check=True)
resolve()
subprocess.run(['cargo', 'fix', '--allow-dirty', '--allow-staged', '--bin', 'searchdump-multi', '--tests', '-q'], check=True)
subprocess.run(['cargo', 'fmt'], check=True)
print('ok')
