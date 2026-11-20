#!/usr/bin/env python3
"""Offline protocol tests. No podman, engines or Rust binaries required."""
import argparse
import json
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import warnings
from unittest.mock import patch

import suite as S


def metric(wall=10):
    return dict(wall_s=wall, docs_per_s=100 / wall, cpu_s=2, user_s=1, sys_s=1,
                max_rss_kb=100, fds_peak=5, mb_per_s_in=10 / wall, mb_per_s_out=5 / wall,
                compression_ratio=2, input_bytes=100, output_bytes=50, exit_code=0, failed_writes=0)


def args(**kw):
    d = dict(tier="L0", place="big", engines=None, cases=None, skip_node=False, reps=5, threshold=10)
    d.update(kw)
    return argparse.Namespace(**d)


class ProtocolTests(unittest.TestCase):
    def test_full_plan_covers_every_engine_image(self):
        plan = S.make_plan(args())
        self.assertEqual(len(plan["engines"]), 16)
        self.assertEqual(len({c['id'] for c in S.CASES}), 37)
        lock = {line.split()[0] for line in (S.ROOT / 'scripts/it/images.lock').read_text().splitlines()
                if line and not line.startswith('#')}
        engines = {e['image'].split('@')[0] for e in plan['engines']}
        helpers = {i for i in lock if '/alpine:' in i or '/toxiproxy:' in i}
        self.assertEqual(engines, lock - helpers)
        self.assertTrue(all('@sha256:' in e['image'] for e in plan['engines']))
        self.assertEqual(S.make_plan(args(tier='L1'))['heap_mb'], 4096)

    def test_opensearch_23_heap_with_insights_disabled(self):
        # Insights off in up.sh; OS 2/3 still need 4g for L0 restore-big (src+dst resident).
        p = S.make_plan(args(tier='L0', place='box', engines='os2-plain,os3-secure,es717-plain,os13-plain'))
        self.assertEqual(p['heap_mb'], 1024)
        by = {e['engine']: e['heap_mb'] for e in p['engines']}
        self.assertEqual(by['os2-plain'], 4096)
        self.assertEqual(by['os3-secure'], 4096)
        self.assertEqual(by['es717-plain'], 1024)
        self.assertEqual(by['os13-plain'], 1024)
        p1 = S.make_plan(args(tier='L1', place='big', engines='os2-plain'))
        self.assertEqual(p1['engines'][0]['heap_mb'], 4096)

    def test_selection_and_exclusions(self):
        p = S.make_plan(args(engines='os3-secure,es710-plain', cases='copy,dump-pit-s4'))
        self.assertEqual(p['engine_order'], ['es710-plain', 'os3-secure'])
        pit = next(c for c in p['engines'][0]['cases'] if c['id'] == 'dump-pit-s4')
        self.assertNotEqual(pit['tools']['rs'], 'RUN')
        with self.assertRaises(ValueError):
            S.make_plan(args(engines='unknown'))
        with self.assertRaises(ValueError):
            S.make_plan(args(cases='copy,copy'))
        with self.assertRaises(ValueError):
            S.make_plan(args(tier='L1', place='shared'))
        self.assertEqual(S.make_plan(args(skip_node=True))['engines'][0]['cases'][0]['tools'], {'rs': 'RUN'})

    def test_aggregate(self):
        a = S.aggregate([metric(x) for x in (4, 1, 5, 2, 3)])
        self.assertEqual(a['wall_s'], 3)
        self.assertEqual(a['wall_p90_s'], 5)
        self.assertEqual((a['wall_min_s'], a['wall_max_s']), (1, 5))
        with self.assertRaises(ValueError):
            S.aggregate([dict(metric(), exit_code=1)])
        with self.assertRaises(ValueError):
            S.aggregate([])

    def test_compare_and_summary(self):
        with tempfile.TemporaryDirectory() as td:
            p = Path(td)
            a = S.row_summary('copy', {'rs': metric(), 'node': metric(20)}, metric(), 10, p)
            self.assertEqual(a['status'], 'PASS')
            self.assertEqual(a['rs_over_node'], .5)
            a = S.row_summary('copy', {'rs': metric(12), 'node': metric()}, metric(), 10, p)
            self.assertEqual(a['status'], 'REGRESSION')
            self.assertAlmostEqual(a['baseline_delta_pct'], 20)
            # Node being quicker does NOT fail a regression-free Rust result.
            a = S.row_summary('copy', {'rs': metric(), 'node': metric(5)}, metric(), 10, p)
            self.assertEqual(a['status'], 'PASS')
            self.assertEqual(a['node_comparison'], 'SLOWER')
            a = S.row_summary('copy', {'rs': metric()}, None, 10, p)
            self.assertIn('NO_BASELINE', a['status'])
            self.assertIsNone(a['node'])
            a = S.row_summary('copy', {}, metric(), 10, p)
            self.assertEqual(a['status'], 'FAIL')
            self.assertTrue(S.compare_metrics(metric(), dict(metric(), max_rss_kb=120), 10, p))
            self.assertFalse(S.compare_metrics(metric(), metric(11), 10, p))
            text = S.markdown({'engines': [dict(engine='os2-plain', summary=[a, dict(case='pit', status='SKIP')],
                                                failures=['synthetic error'])]})
            self.assertIn('| copy |', text)
            self.assertIn('synthetic error', text)
            self.assertIn('SKIP', text)

    def test_invalid_metrics_fail(self):
        with tempfile.TemporaryDirectory() as td:
            self.assertTrue(S.compare_metrics(metric(), dict(metric(), failed_writes=1), 10, td))
            self.assertTrue(S.compare_metrics(metric(), dict(metric(), exit_code=1), 10, td))

    def test_baseline_identity_no_start(self):
        # Identity mismatches must fail without provisioning or overwriting a baseline.
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            d = Path(td)
            p = S.make_plan(args(engines='es717-plain', cases='copy'))
            a = args(update_baselines=True, baseline_dir=d)
            host = S.os.environ.get('PERF_HOST_ID', S.platform.node())
            bpath = d / f'{host}-big-L0-es717-plain.json'
            S.write_json(bpath, {'identity': {}, 'cells': {'copy': metric()}})
            original = bpath.read_text()
            out = d / 'engine'
            out.mkdir()
            with patch.object(S.EngineRun, 'start') as start, patch.object(S.EngineRun, 'close'):
                res = S.execute_engine(a, p, p['engines'][0], {}, out)
                start.assert_not_called()
            self.assertTrue(res['failures'])
            self.assertEqual(original, bpath.read_text())

    def test_measure_wrapper_schema(self):
        with tempfile.TemporaryDirectory() as td:
            p = Path(td)
            out = p / 'data.ndjson'
            metrics = p / 'metrics.json'
            subprocess.run([str(S.LOAD / 'measure.sh'), '--out', str(metrics), '--docs', '2', '--output', str(out),
                            '--', sys.executable, '-c', f"from pathlib import Path; Path({str(out)!r}).write_text('{{}}\\n{{}}\\n')"],
                           check=True, capture_output=True)
            m = json.loads(metrics.read_text())
            with warnings.catch_warnings():
                warnings.simplefilter('ignore', ResourceWarning)  # legacy validator opens its schema inline
                self.assertEqual(S.M.validate(m), [])
            self.assertEqual(m['exit_code'], 0)
            self.assertEqual(m['docs'], 2)
            self.assertGreater(m['output_bytes'], 0)

    def test_ab_cases_are_opt_in_and_leave_the_manifest_alone(self):
        # 1.1-1: the default manifest (part of the baseline identity) is unchanged.
        base, ab = S.cases(), S.cases(ab=True)
        self.assertEqual(ab[:len(base)], base)
        extra = {c['id']: c for c in ab[len(base):]}
        self.assertEqual(set(extra), {'restore-norefresh', 'copy-norefresh'})
        self.assertEqual(extra['restore-norefresh']['kind'], 'restore')
        self.assertEqual(extra['copy-norefresh']['kind'], 'copy')
        for c in extra.values():
            self.assertEqual(c['flags'], ['--noRefresh'])
            self.assertTrue(c['node'])
        self.assertNotIn('restore-norefresh', [c['id'] for c in base])

    def test_measure_timer_resolution(self):
        # 1.1-5: wall time comes from a blocking wait4, not a 0.1 s poll. The old loop
        # reported >= 0.3 s for a 0.23 s command; allow scheduling slack but stay below that.
        with tempfile.TemporaryDirectory() as td:
            metrics = Path(td) / 'metrics.json'
            subprocess.run([str(S.LOAD / 'measure.sh'), '--out', str(metrics), '--docs', '0',
                            '--', sys.executable, '-c', 'import time; time.sleep(0.23)'],
                           check=True, capture_output=True)
            m = json.loads(metrics.read_text())
            self.assertEqual(m['timer'], 'blocking-wait4')
            self.assertGreaterEqual(m['wall_s'], 0.23)
            self.assertLess(m['wall_s'], 0.29)

    def test_measure_exit_code_and_rusage(self):
        with tempfile.TemporaryDirectory() as td:
            metrics = Path(td) / 'metrics.json'
            p = subprocess.run([str(S.LOAD / 'measure.sh'), '--out', str(metrics), '--docs', '0', '--',
                                sys.executable, '-c',
                                'import sys\nx = 0\nfor i in range(3000000): x += i\nsys.exit(7)'],
                               capture_output=True)
            m = json.loads(metrics.read_text())
            self.assertEqual(p.returncode, 7)
            self.assertEqual(m['exit_code'], 7)
            self.assertGreater(m['user_s'], 0)
            self.assertGreater(m['max_rss_kb'], 0)
            self.assertGreater(m['rss_peak_kb_sampled'], 0)

    def test_baseline_merge_and_failed_run_preserves(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            d = Path(td)
            p = S.make_plan(args(engines='es717-plain', cases='copy'))
            a = args(update_baselines=True, baseline_dir=d, reps=2)
            def invoke(name, failure=False):
                directory = d / name
                directory.mkdir()
                with patch.object(S.EngineRun, 'start'), patch.object(S.EngineRun, 'seed'), \
                     patch.object(S.EngineRun, 'cleanup_dataset'), patch.object(S.EngineRun, 'close'), \
                     patch.object(S.EngineRun, 'iteration', side_effect=RuntimeError('bad digest') if failure else None,
                                  return_value=metric()):
                    return S.execute_engine(a, p, p['engines'][0], {}, directory)
            res = invoke('ok')
            self.assertEqual(res['failures'], [])
            bpath = Path(res['baseline_updated'])
            base = json.loads(bpath.read_text())
            base['cells']['unselected'] = metric()
            S.write_json(bpath, base)
            res = invoke('partial')
            self.assertIn('unselected', json.loads(bpath.read_text())['cells'])
            before = bpath.read_text()
            res = invoke('failed', True)
            self.assertTrue(res['failures'])
            self.assertEqual(res['summary'][0]['status'], 'FAIL')
            self.assertEqual(before, bpath.read_text())

    def test_node_failure_is_recorded_not_an_rs_failure(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            d = Path(td)
            p = S.make_plan(args(engines='es717-plain', cases='dump-small'))
            a = args(update_baselines=True, baseline_dir=d, reps=2)
            calls = []
            def it(case, tool, rep):
                calls.append((tool, rep))
                if tool == 'node':
                    raise S.VerifyMismatch('document sets differ')
                return metric()
            with patch.object(S.EngineRun, 'start'), patch.object(S.EngineRun, 'seed'), \
                 patch.object(S.EngineRun, 'cleanup_dataset'), patch.object(S.EngineRun, 'close'), \
                 patch.object(S.EngineRun, 'iteration', side_effect=it):
                res = S.execute_engine(a, p, p['engines'][0], {}, d)
            self.assertEqual(res['failures'], [])
            row = res['summary'][0]
            self.assertEqual(row['status'], 'PASS (NO_BASELINE)')
            self.assertEqual(row['node_status'], 'NODE_DIFF')
            self.assertIsNone(row['node'])
            # Node stops after its first failure; rs completes warm-up plus every repetition.
            self.assertEqual([c for c in calls if c[0] == 'node'], [('node', -1)])
            self.assertEqual(len([c for c in calls if c[0] == 'rs']), 3)
            self.assertIn('NODE_DIFF', S.markdown(dict(engines=[res])))
            self.assertIn('baseline_updated', res)

    def test_timeout_kills_process_group(self):
        with tempfile.TemporaryDirectory() as td:
            marker = Path(td) / 'child-alive'
            # The wrapper forks a grandchild (like measure.py -> Node -> forked elasticdump).
            script = (f"import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',"
                      f"\"import time,pathlib; time.sleep(3); pathlib.Path({str(marker)!r}).write_text('x')\"]); time.sleep(60)")
            with self.assertRaises(S.ToolTimeout):
                S.checked([sys.executable, '-c', script], timeout_s=1)
            import time
            time.sleep(3.5)
            self.assertFalse(marker.exists())
        with self.assertRaises(subprocess.CalledProcessError):
            S.checked([sys.executable, '-c', 'raise SystemExit(3)'], timeout_s=30)

    def test_node_timeout_is_recorded_not_an_rs_failure(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            d = Path(td)
            p = S.make_plan(args(engines='es717-plain', cases='multi-restore-p1'))
            a = args(update_baselines=False, baseline_dir=d, reps=1)
            def it(case, tool, rep):
                if tool == 'node':
                    raise S.ToolTimeout('timed out')
                return metric()
            with patch.object(S.EngineRun, 'start'), patch.object(S.EngineRun, 'seed'), \
                 patch.object(S.EngineRun, 'cleanup_dataset'), patch.object(S.EngineRun, 'close'), \
                 patch.object(S.EngineRun, 'iteration', side_effect=it):
                res = S.execute_engine(a, p, p['engines'][0], {}, d)
            self.assertEqual(res['failures'], [])
            self.assertEqual(res['summary'][0]['node_status'], 'NODE_TIMEOUT')

    def test_engine_port_override_and_busy_port(self):
        import socket
        with patch.dict(S.os.environ, {}, clear=False):
            S.os.environ.pop('PERF_ENGINE_PORT', None)
            self.assertEqual(S.engine_port(), 19222)
            S.os.environ['PERF_ENGINE_PORT'] = '19333'
            self.assertEqual(S.engine_port(), 19333)
            S.os.environ['PERF_ENGINE_PORT'] = 'x'
            with self.assertRaises(ValueError):
                S.engine_port()
        with socket.socket() as srv:
            srv.bind(('127.0.0.1', 0))
            srv.listen(1)
            busy = srv.getsockname()[1]
            self.assertTrue(S.port_in_use(busy))
        self.assertFalse(S.port_in_use(busy))

    def test_node_auth_file_is_ini(self):
        text = S.node_auth_ini('admin', 'p;w#d="x"')
        self.assertEqual(text, 'user="admin"\npassword="p;w#d=\\"x\\""\n')
        node = Path(S.os.environ.get('NODE_ED_BIN', '/workspace/node-ed/node_modules/.bin')).parent / 'elasticdump'
        if (node / 'lib/add-auth.js').exists() and S.shutil.which('node'):
            with tempfile.NamedTemporaryFile('w', suffix='.ini') as f:
                f.write(text)
                f.flush()
                js = (f"console.log(require({str(node / 'lib/add-auth.js')!r})"
                      f"('http://h:1/x', {f.name!r}))")
                out = subprocess.run(['node', '-e', js], capture_output=True, text=True, check=True).stdout
            self.assertEqual(out.strip(), 'http://admin:p%3Bw%23d%3D%22x%22@h:1/x')

    def test_settings_target_is_left_absent(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            plan = S.make_plan(args(engines='es717-plain', cases='settings-restore'))
            er = S.EngineRun(args(), plan, plan['engines'][0], {}, Path(td))
            er.cl = unittest.mock.Mock()
            er.create_target('perf-src', 'perf-dst', meta='settings')
            # 1.3-R1: the DELETE is followed by a wait until the index is gone (GET _settings -> 404);
            # nothing creates the target
            self.assertEqual([c.args[0] for c in er.cl.req.call_args_list], ['DELETE', 'GET'])
            self.assertTrue((Path(td) / 'deletes.log').read_text().count('DELETE /perf-dst') == 1)

    def test_delete_index_waits_until_gone_and_logs(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            plan = S.make_plan(args(engines='es717-plain', cases='settings-restore'))
            er = S.EngineRun(args(), plan, plan['engines'][0], {}, Path(td))
            er.cl = unittest.mock.Mock()
            # acknowledged late: the index still answers twice before it is gone
            er.cl.req.side_effect = [{'acknowledged': False}, {'perf-dst': {}}, {'perf-dst': {}}, None]
            with patch.object(S.time, 'sleep'):
                er.delete_index('perf-dst', 'rep0')
            line = (Path(td) / 'deletes.log').read_text()
            self.assertIn('acknowledged=False gone=True polls=2', line)
            er.cl.req.side_effect = None
            er.cl.req.return_value = {'perf-dst': {}}
            with patch.object(S, 'DELETE_WAIT_S', 0.01), self.assertRaises(RuntimeError):
                er.delete_index('perf-dst', 'rep1')

    def test_multi_cli_child_flags_and_fresh_targets(self):
        with tempfile.TemporaryDirectory(dir=S.ROOT / 'results') as td:
            directory = Path(td)
            plan = S.make_plan(args(engines='es717-plain', cases='multi-restore-p4'))
            entry = plan['engines'][0]
            er = S.EngineRun(args(), plan, entry, {'searchdump-multi': 'multi'}, directory)
            er.url = 'http://127.0.0.1:1'
            er.tenv = {}
            er.rs_tls, er.node_tls = [], []
            er.sources = [dict(index='perf-src-00', stored=100)]
            er.count, er.raw_bytes = 100, 1000
            er.refdir = er.work / 'refs'
            er.refdir.mkdir()
            er.cl = unittest.mock.Mock()
            er.verify = unittest.mock.Mock()
            commands = []
            def measured(command, **_kw):
                commands.append(command)
                out = Path(command[command.index('--out') + 1])
                S.write_json(out, metric())
            with patch.object(S, 'checked', side_effect=measured), patch.object(S.M, 'validate', return_value=[]), \
                 patch.object(S.R, 'leak_check', return_value=({}, [])):
                er.iteration(entry['cases'][0], 'rs', 0)
            command = commands[0]
            child = command[command.index('multi'):]
            self.assertEqual(child[child.index('--') + 1:], ['--limit=2000', '--concurrency=1'])
            self.assertIn('--compression=none', child)
            self.assertIn('--parallel=4', child)
            # Existing target creation would make Rust multi refuse the load.
            self.assertFalse(any(c.args[0] == 'PUT' for c in er.cl.req.call_args_list))
            er.verify.assert_called_once()

    def test_split_wrapper_failure_stops(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / 'ran'
            command = [sys.executable, str(S.HERE / 'parts.py'), '--files', 'one', 'two', '--',
                       sys.executable, '-c', f"from pathlib import Path; Path({str(path)!r}).write_text('one'); raise SystemExit(7)"]
            self.assertEqual(subprocess.run(command).returncode, 7)
            self.assertEqual(path.read_text(), 'one')

    def test_metadata_normalization(self):
        cl = unittest.mock.Mock()
        cl.req.return_value = {'src': {'settings': {'index': {'number_of_shards': '1', 'uuid': 'x', 'version': '9',
                                                            'creation_date': '42', 'provided_name': 'src'}}}}
        self.assertEqual(S.meta_value(cl, 'src', 'settings'), {'number_of_shards': '1'})
        cl.req.return_value = {'src': {'mappings': {'properties': {'x': {'type': 'keyword'}}}}}
        self.assertIn('properties', S.meta_value(cl, 'src', 'mappings'))
        cl.req.assert_called_with('GET', '/src/_mapping')

    def test_down_scopes_container_and_volume_cleanup(self):
        with tempfile.TemporaryDirectory() as td:
            d = Path(td)
            fake = d / 'podman'
            log = d / 'calls'
            fake.write_text("#!/bin/bash\nprintf '%s ' \"$@\" >> \"$TEST_LOG\"; printf '\\n' >> \"$TEST_LOG\"\n"
                            "if [[ \"$1\" == ps ]]; then echo our-container; fi\n"
                            "if [[ \"$1 $2\" == 'volume ls' ]]; then echo our-volume; fi\n")
            fake.chmod(0o755)
            env = dict(S.os.environ, PATH=str(d) + ':' + S.os.environ['PATH'], TEST_LOG=str(log),
                       IT_DIR=str(d / 'private'), RUN_ID='suite-run')
            subprocess.run([str(S.ROOT / 'scripts/it/down.sh')], env=env, check=True)
            calls = log.read_text()
            self.assertIn('name=-suite-run$', calls)
            self.assertIn('label=searchdump-run=suite-run', calls)
            self.assertIn('rm -f -v our-container', calls)
            self.assertIn('volume rm our-volume', calls)

    def test_offline_dry_run_has_no_results_side_effects(self):
        with patch.object(S, 'prerequisites', side_effect=AssertionError('must not run')):
            p = subprocess.run([str(S.HERE / 'run-suite.sh'), '--dry-run', '--engines', 'os2-plain', '--skip-node'],
                               capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(p.stdout)['engine_order'], ['os2-plain'])


    def test_pit_table_step_by_step(self):
        """pit_table.py: one column per step, change vs the previous step, noise-aware bold."""
        import tempfile
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            steps = [('0', 'beta1', 'aaaa', 'a' * 40, 'h-beta1-timer2', 'base'),
                     ('1', '1.1-2', 'bbbb', 'b' * 40, 'h-1.1-bbbb', 'bulk(1.1-2): x'),
                     ('2', '1.1-1', 'cccc', 'c' * 40, 'h-1.1-cccc', 'bulk(1.1-1): y')]
            (t / 'steps.tsv').write_text(''.join('\t'.join(r) + '\n' for r in steps))
            walls = {'aaaa': (1.0, 2.0), 'bbbb': (0.99, 1.0), 'cccc': (0.98, 0.5)}
            for short, (dump, restore) in walls.items():
                res = t / f'res-{short}'
                res.mkdir()
                cases = [dict(case='dump-none', rs=dict(wall_s=dump, wall_cv_pct=1.0)),
                         dict(case='restore-none', rs=dict(wall_s=restore, wall_cv_pct=1.0),
                              node=dict(wall_s=4.0) if short == 'aaaa' else None)]
                (res / 'combined.json').write_text(json.dumps(dict(engines=[dict(
                    engine='os3-plain', engine_version='3.0.0', failures=[], summary=cases)])))
                (t / 'runs' / short).mkdir(parents=True)
                (t / 'runs' / short / 'os3-plain.done').write_text(f'rc=0 dir={res} harness=hhh\n')
            p = subprocess.run([sys.executable, str(S.HERE / 'pit_table.py'), '--state', str(t),
                                '--engines', 'os3-plain es710-plain'], capture_output=True, text=True, check=True)
        out = p.stdout
        self.assertIn('| restore-none | 4.000 | 2.000 | 1.000 (**-50%**) | 0.500 (**-50%**) | **-75%** |', out)
        self.assertIn('| dump-none | — | 1.000 | 0.990 (-1%) | 0.980 (-1%) | -2% |', out)
        self.assertIn('No finished runs yet.', out)  # es710-plain
        self.assertIn('| 1.1-1 | `cccc` | os3-plain | 0.495 | 1 | 0 | 2 |', out)


class ReleaseEvalTests(unittest.TestCase):
    """Offline tests for scripts/perf/release-eval.sh / release_eval.py. No engines."""

    def setUp(self):
        import release_eval as RE
        self.RE = RE

    def test_presets_and_engine_selection(self):
        RE = self.RE
        self.assertEqual(RE.PRESETS["quick"], ["es819-plain", "os3-plain"])
        self.assertEqual(RE.select_engines("all", None), list(S.ENGINE_ORDER))
        self.assertEqual(len(RE.select_engines("plain", None)), 8)
        self.assertEqual(len(RE.select_engines("secure", None)), 8)
        self.assertTrue(all(e.endswith("-plain") for e in RE.select_engines("plain", None)))
        self.assertEqual(RE.select_engines(None, "os3-plain,es710-plain"),
                         ["es710-plain", "os3-plain"])
        with self.assertRaises(ValueError):
            RE.select_engines("quick", "es819-plain")
        with self.assertRaises(ValueError):
            RE.select_engines("nope", None)

    def test_asset_names_and_checksums(self):
        RE = self.RE
        names = RE.asset_names("1.0.0-beta1", "x86_64-unknown-linux-gnu")
        self.assertEqual(names["archive"], "searchdump-rs-1.0.0-beta1-x86_64-unknown-linux-gnu.tar.gz")
        self.assertEqual(names["sha256"], names["archive"] + ".sha256")
        text = "abc\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  " + names["archive"] + "\n"
        digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        self.assertEqual(RE.parse_sha256_line(text, names["archive"]), digest)
        with tempfile.TemporaryDirectory() as td:
            archive = Path(td) / "a.tar.gz"
            archive.write_bytes(b"hello")
            good = RE.sha256_file(archive)
            RE.verify_checksum(archive, good)
            with self.assertRaises(ValueError):
                RE.verify_checksum(archive, "0" * 64)

    def test_normalize_tag_repo_and_host_id(self):
        RE = self.RE
        self.assertEqual(RE.normalize_tag("v1.0.0-beta1"), ("v1.0.0-beta1", "1.0.0-beta1"))
        self.assertEqual(RE.normalize_tag("1.0.0"), ("v1.0.0", "1.0.0"))
        self.assertEqual(RE.normalize_tag("latest"), ("latest", "latest"))
        self.assertEqual(RE.parse_github_repo("git@github.com:robertgoley/searchdump-rs.git"),
                         "robertgoley/searchdump-rs")
        self.assertEqual(RE.parse_github_repo("https://github.com/robertgoley/searchdump-rs.git"),
                         "robertgoley/searchdump-rs")
        self.assertIsNone(RE.parse_github_repo("https://example.com/foo.git"))
        self.assertTrue(RE.host_id_ok("box-x86_64-8c-16g"))
        self.assertFalse(RE.host_id_ok("has space"))
        self.assertFalse(RE.host_id_ok("has/slash"))
        self.assertTrue(RE.host_id_ok(RE.sanitize_host_id("My Host/1")))
        self.assertTrue(RE.sanitize_host_id("***").startswith("host"))

    def test_results_path_and_reaper_scope(self):
        RE = self.RE
        d = RE.results_dir(S.ROOT, "1.0.0-beta1", "box-x86_64-8c-16g")
        self.assertEqual(d, S.ROOT / "scripts/perf/results/release-eval/1.0.0-beta1/box-x86_64-8c-16g")
        with tempfile.TemporaryDirectory() as td:
            run = Path(td) / "eval-run"
            (run / "podman-root").mkdir(parents=True)
            (run / "xdg-runtime").mkdir()
            pats = RE.reaper_patterns(run)
            store = str((run / "podman-root").resolve())
            xdg = str((run / "xdg-runtime").resolve())
            self.assertTrue(pats["conmon"].startswith("conmon "))
            self.assertTrue(re.search(pats["conmon"], f"conmon --foo {store}/libpod"))
            self.assertTrue(re.search(pats["pasta"], f"pasta --config {xdg}/netns/x"))
            # Must not match another driver's isolated store / runtime.
            self.assertFalse(re.search(pats["conmon"], "conmon --root /home/box/.local/share/edrs-odt-podman/x"))
            self.assertFalse(re.search(pats["pasta"], "pasta --config /tmp/xdg-runtime-perf-b1/netns/x"))

    def test_fingerprint_has_no_secret_keys(self):
        RE = self.RE
        dirty = {"cpu_model": "x", "AWS_SECRET_ACCESS_KEY": "nope", "nested": {"password": "x", "ram_gb": 8}}
        clean = RE.strip_secrets(dirty)
        self.assertNotIn("AWS_SECRET_ACCESS_KEY", clean)
        self.assertNotIn("password", clean["nested"])
        self.assertEqual(clean["nested"]["ram_gb"], 8)
        fp = RE.host_fingerprint(host_id="selftest-host")
        self.assertEqual(fp["host_id"], "selftest-host")
        self.assertTrue(RE.host_id_ok(fp["host_id"]))
        blob = json.dumps(fp)
        for bad in ("AWS_SECRET", "GITHUB_TOKEN", "GH_TOKEN", "IT_ADMIN_PW", "password="):
            self.assertNotIn(bad, blob)
        with self.assertRaises(ValueError):
            RE.host_fingerprint(host_id="bad id")

    def test_commit_only_results_dir(self):
        RE = self.RE
        with tempfile.TemporaryDirectory() as td:
            repo = Path(td) / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-q"], cwd=repo, check=True, env={**S.os.environ, "GIT_AUTHOR_NAME": "t",
                            "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"})
            # git init may not set user; configure locally
            subprocess.run(["git", "config", "user.email", "t@t"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "t"], cwd=repo, check=True)
            (repo / "keep.txt").write_text("keep\n")
            dest = repo / "scripts/perf/results/release-eval/1.0.0-beta1/host1"
            dest.mkdir(parents=True)
            (dest / "report.md").write_text("# report\n")
            (repo / "unrelated.txt").write_text("no\n")
            subprocess.run(["git", "add", "keep.txt"], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-q", "-m", "init"], cwd=repo, check=True)
            # Stage an unrelated file that must not be committed.
            subprocess.run(["git", "add", "unrelated.txt"], cwd=repo, check=True)
            original_root = RE.ROOT
            try:
                RE.ROOT = repo
                info = RE.git_commit_results(dest, RE.commit_message("1.0.0-beta1", "host1"), push=False)
            finally:
                RE.ROOT = original_root
            self.assertTrue(info["committed"])
            self.assertEqual(info["message"], "perf: release-eval 1.0.0-beta1 results from host1")
            self.assertIn("git push origin HEAD:", info["push_command"])
            # The commit should contain only the results path, not unrelated.txt
            names = subprocess.check_output(["git", "show", "--name-only", "--pretty=", "HEAD"], cwd=repo, text=True)
            self.assertIn("scripts/perf/results/release-eval/1.0.0-beta1/host1/report.md", names)
            self.assertNotIn("unrelated.txt", names)
            # unrelated.txt should still be staged (not lost, not committed)
            st = subprocess.check_output(["git", "status", "--short"], cwd=repo, text=True)
            self.assertIn("unrelated.txt", st)

    def test_dry_run_shell_prints_plan_without_side_effects(self):
        with tempfile.TemporaryDirectory() as td:
            run_dir = Path(td) / "run"
            results = Path(td) / "results"
            p = subprocess.run(
                [str(S.HERE / "release-eval.sh"), "--dry-run", "--bin", str(Path(td) / "missing-bins"),
                 "--tag", "v1.0.0-beta1", "--preset", "quick", "--run-dir", str(run_dir),
                 "--results-dir", str(results), "--host-id", "selftest-dry"],
                cwd=S.ROOT, capture_output=True, text=True)
            # Missing --bin is a dry-run warning, not an error; other preflight may still pass.
            self.assertIn("release-eval plan", p.stdout)
            self.assertIn("es819-plain", p.stdout)
            self.assertIn("os3-plain", p.stdout)
            self.assertIn("selftest-dry", p.stdout)
            self.assertIn('"dry_run": true', p.stdout)
            # Must not create the results tree or a podman store.
            self.assertFalse(results.exists())
            self.assertFalse((run_dir / "podman-root").exists())
            # And must not have invoked podman pull / run (stdout/stderr should not show it).
            self.assertNotIn("podman pull", p.stdout + p.stderr)
            self.assertNotIn("podman run", p.stdout + p.stderr)


if __name__ == '__main__':
    unittest.main()
