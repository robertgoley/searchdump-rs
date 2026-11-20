# Items to review for 1.3

## 1.3-R1: multi-restore p8 "target index deleted and re-created" on ES 8.6 (1.1.0 RC)

**Status:** **closed 2026-11-17 (1.3 decision 5, Robert t223u): harness-caused, mitigated.** The L3-002 harness DELETE wait is kept (checks `acknowledged`, polls until 404, logs every DELETE). Evidence: 63 ES 8.6.1 plain `multi-restore-p8` restores on grokbox with 0 failures (L3-009: x20 old harness 1.1.0, x20 old harness 1.2.0, x20+ new harness 1.2.0; 336/336 DELETEs applied before the first poll). Not a searchdump bug (code read: per-child targets, no parent DELETE/PUT). The pause / memory-pressure variants and per-child uuid logging were not run and are not needed for closing. Not fixed in 1.1.0 (code frozen) and not part of 1.2.

### What happened
- Run: 1.1.0 RC L0 suite on grokbox, engine `elasticsearch8-plain` (ES 8.6.1, plain HTTP),
  result dir `results/perf/20261115T042919-1092769/` (on the box only), 2026-11-15 10:01 ET.
- Case `multi-restore-p8`, tool `rs`, rep 0 (the warmup rep passed seconds earlier):
  `searchdump-multi --direction=load --includeType=data --parallel=8 --prefix=perf-dst- --compression=none --noDefaultDataFlags -- --limit=2000 --concurrency=1`,
  16 indexes x 6,250 docs (100k docs, about 100 MB).
- 5 of 16 children (`perf-src-00`, `-01`, `-09`, `-10`, `-11`) exited 1 with:
  `the target index 'perf-dst-perf-src-NN' was deleted during the restore and re-created ...`
  The other 11 wrote all 6,250 docs with 0 failed / 0 rejected.
- The engine total was 36 PASS / 1 FAIL. The same case passed on every other engine run so far,
  including `elasticsearch8-secure` (same ES 8.6.1).

### What the message means
`src/pipeline/bulk.rs` (TST-BRK-SRV-008): after `UUID_CAPTURE_AFTER` of writes, a restore notes the single target
index's `index.uuid` (`note_target_uuid`), and at the end checks it again (`check_target_index`).
A different uuid at the end means the index was deleted and auto-created again mid-run. So, from the server's
view, the index those children were writing to really did change uuid during the run. searchdump reported this
correctly instead of claiming a good restore. Its exit 1 is the intended behavior.

### Who could have deleted it (not confirmed; read-only look, no rerun)
- **The harness itself, most likely.** `scripts/perf/suite.py` deletes every `perf-dst-*` target before each rep
  (line ~673) and again in the rep's `finally` cleanup (line ~787). It uses `ok404=True` and does not check
  `"acknowledged": true`. If ES acknowledged a DELETE late (master ack timeout under load), the old index can
  still exist when the next rep's first bulks arrive. Those bulks write into the old index (its uuid gets noted),
  then the delete is applied, and the next bulk auto-creates a new index with a new uuid. That matches the
  "deleted and re-created" form exactly, and only some of the 16 indexes being hit.
- **Box pause/resume.** The box had just resumed (no writes 09:19 to 10:01 ET) before this case ran. A
  just-resumed ES node publishing cluster state slowly makes the late-ack window above more likely. On
  2026-11-15 the box paused between sessions, and podman lost track of running engine containers after a resume
  (see RC NOTES / results/local/1.1-rc.md).
- **Less likely: a searchdump-multi bug** (for example two children targeting one index, or the parent touching
  targets). With `--prefix=perf-dst-` each child has its own target, and the parent does no deletes in a
  data-only load. This needs confirming in code, not assuming.

### What to review in 1.3
1. Harness: after each DELETE of a target, check `acknowledged` and wait until `HEAD /<index>` returns 404 (bounded)
   before starting the next rep; log the DELETE responses with timestamps in the case dir.
2. Harness: record each child's noted and final uuid (or have `searchdump` log both in its stats/log at
   info level) so a repeat can tell harness deletes from tool behavior.
3. Reproduce on ES 8.6 plain with 8 workers: loop `multi-restore-p8` 20x back-to-back, then again with an artificial
   pause between reps. Also try with the box under memory pressure.
4. Code read of `searchdump-multi` load mode: confirm no shared targets between children and no parent-side
   DELETE / index creation with `--includeType=data`.
5. If the cause is the harness, fix it there and keep the detector as is. If ES can briefly report a stale uuid
   with no real delete, consider having the detector re-check (one retry after a short delay) before failing,
   without weakening TST-BRK-SRV-008.

### Evidence to keep
- `multi-restore-p8-rs-0-children/perf-src-*.data.log` and `.data.stats.json` in the RC result dir above.
- RC `summary.md` for `elasticsearch8-plain` (line "Failure: multi-restore-p8/rs/rep0 ...").

### 1.3 results (local release/1.3, grokbox 2026-11-17)
- Items 1-2 (harness): done in L3-002 (`EngineRun.delete_index`: checks `acknowledged`, polls until 404, bounded
  `PERF_DELETE_WAIT_S`, `deletes.log` per result dir). Recording each child's noted/final uuid is not done.
- Item 3: ES 8.6.1 plain `multi-restore-p8` x20 with the old harness on 1.1.0 and 1.2.0 and x20 with the new
  harness on 1.2.0: 0 failures, 336/336 DELETEs applied before the first poll (`results/local/L3-009.md`). The
  pause / memory-pressure variants were not run.
- Item 4: confirmed by code read: per-child targets with `--prefix`; no parent DELETE/PUT for data-only loads.
