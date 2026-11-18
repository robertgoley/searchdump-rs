# L3-017: grow-back evaluation for 1.3 decision 2 (Robert t227u/t228u)

grokbox, 2026-11-17 20:15-21:36 ET, isolated podman store, port 19415, containers removed after each case.
Driver: `scripts/load/growback_reps.py` (L3-016). 4 reps per variant, variant order rotated per rep.
- **load**: one index `pmc` (20,000 x ~22 KB) via `searchdump-multi --direction load` (one child).
- **multi8**: eight indexes `pmc-0..7` (8 x 2,500 docs) with `--parallel 8`. This is the small-heap analogue of
  Suite A multi-restore-p8.

Binaries:
- `bin/1.2.0`: the release.
- `bin/l3gb`: L3-016, the 1.3 floor (L3-014) plus the counters.
- `bin/l3gb2`: l3gb plus the L3-017 bulk-floor fix (see "Sustained 429").

Variants (`SEARCHDUMP_LIMITS_EXPERIMENT`):
- 1.2.0 auto;
- 1.3 floor (default, unset);
- floor+growback (`growback`);
- floor+retryfirst+growback (`retryfirst,growback`).

All with `--serverLimits=auto`, except the `-off` sustained cases.

What each run records:
- wall time and exit status;
- documents counted after the load, against the expected count;
- size 429s, splits, retryfirst resends, grows and failed grows, plus the failed-grow time and bytes. These come
  from the new end-of-run `bulk size summary` line. 1.2.0 has no summary line, so its counts come from its
  warnings;
- lowest and final bulk cap;
- parent-breaker trips;
- the node's heap peak, polled every 0.5 s.

Raw rows: `L3-017/<case>.json`.

## Headline: median wall time (s) per engine/heap and workload

| engine / heap | workload | 1.2.0 auto | 1.3 floor | floor+gb | floor+rf+gb | failed grows/run (gb / rf+gb) |
|---|---|---:|---:|---:|---:|---|
| OS 3.9.0 256m | load | 15.7 | 15.0 | **22.6** | 14.1 | 2.8 / 0 |
| OS 3.9.0 256m | multi8 | 5.5 | 5.3 | **7.5** | 5.0 | 3.2 / 0 |
| OS 3.9.0 384m | load | 11.4 | 14.9 | 13.7 | 10.4 | 1.5 / 0.2 |
| OS 3.9.0 384m | multi8 | 6.4 (1 exit 1) | 5.2 | 2.9 | 5.1 | 1.2 / 0 |
| OS 3.9.0 512m | load | 15.2 | 15.2 | 16.0 | **20.4** | 1.8 / 2.0 |
| OS 3.9.0 512m | multi8 | 5.2 | 10.8 | 8.1 | 6.5 | 0.8 / 0 |
| OS 2 256m | load | 4.7 | 4.7 | 4.7 | 4.8 | 0 / 0 (no 429) |
| OS 2 256m | multi8 | 4.3 | 5.7 | 9.5 | 4.1 | 0 / 0 |
| ES 8.19.23 256m | load | 4.5 | 4.4 | 4.4 | 4.4 | 0 / 0 (no 429) |
| ES 8.19.23 256m | multi8 | 1.2 | 1.2 | 1.2 | 1.2 | 0 / 0 (no 429) |

What the table shows:
- **Documents.** Every run in every case wrote and counted all documents (20,000).
- **Exit status.** One run exited 1: 1.2.0 on OS3 384m multi8, rep 2. All 20,000 documents were written, but the
  parent reported a failure after 33 splits and 48 breaker trips. This is the L3-008 false failure (the parent's
  read-only call got a 429). None of the 1.3 runs (L3-008 retry) failed.
- **Noise.** The OS 3 engines are bimodal: a run either trips the breaker repeatedly or not at all, so the min-max
  ranges are wide (for example 4.4-25.4 s). Four reps separate large effects only.
- **Floor vs 1.2.0.**
  - The floor kept the lowest cap at 1024 KB everywhere. 1.2.0 went down to 44-788 KB (44 KB on OS3 384m load).
  - Medians are within noise on most cells.
  - OS3 512m multi8 has a 70.7 s outlier in the floor column: rep 0 with 10.8 splits per run.
- **Heap peaks.** 92-95 % on OS 3 for all variants. Neither the floor nor grow-back raised them.

Full table (all counters): see "All counters" at the end.

## Failed grow-back penalty

A failed grow is a grow that the next size 429 answers for a request larger than the cap before the grow. Its
penalty is measured in the child:
- the round trip of the refused request;
- plus the jittered back-off after it. Multi children use `--retryDelay=2000`, so the back-off is 1-3 s;
- plus, with `retryfirst`, the resend at the same size.

The refused bytes are all sent again.

| case | variant | failed grows/run | ms per failed grow | KB refused per failed grow | penalty ms/run | penalty / wall | median vs floor |
|---|---|---:|---:|---:|---:|---:|---:|
| OS3 256m load | floor+gb | 2.8 | 1,867 | 3,707 | 5,134 | 24 % | 1.51x |
| OS3 256m multi8 | floor+gb | 3.2 | 2,209 | 1,589 | 7,178 | (8 children) | 1.41x |
| OS3 384m load | floor+gb | 1.5 | 2,126 | 6,318 | 3,189 | 20 % | 0.92x |
| OS3 384m load | floor+rf+gb | 0.2 | 1,591 | 9,594 | 398 | 4 % | 0.70x |
| OS3 384m multi8 | floor+gb | 1.2 | 1,923 | 2,367 | 2,404 | (8 children) | 0.55x |
| OS3 512m load | floor+gb | 1.8 | 2,018 | 9,099 | 3,531 | 22 % | 1.05x |
| OS3 512m load | floor+rf+gb | 2.0 | 3,920 | 21,491 | 7,841 | 40 % | 1.34x |
| OS3 512m multi8 | floor+gb | 0.8 | 2,358 | 2,548 | 1,768 | (8 children) | 0.75x |
| **sustained** ES 8.19 ip 2mb, `off`, load | floor+gb | **5.0** | 2,088 | 2,042 | 10,438 | 13 % | **1.43x** |
| **sustained** ES 8.19 ip 2mb, `off`, load | floor+rf+gb | **5.0** | 4,094 | 4,083 | 20,472 | 44 % | **1.94x** |
| sustained OS3 256m `off`, load | floor+gb | 0.2 | 2,266 | 13,931 | 566 | 8 % | 0.68x |

How to read it:
- **Size of the penalty.** About **2 s per failed grow** without retryfirst and about **4 s with retryfirst**
  (the resend is refused too, which means two back-offs). The back-off dominates; the 429 itself comes back in
  milliseconds. Each failed grow also costs about 2-21 MB sent and refused.
- **Penalty / wall.** This is child time over wall time. For multi8 it is summed over 8 parallel children, so it
  cannot be compared with wall time and is not given as a share.
- **Extra breaker trips.** On OS3 256m, floor+gb drew more parent-breaker trips than the floor alone: 21.5 vs 9.5
  per run (load) and 15.0 vs 7.5 (multi8). A failed grow is not only a 429: the larger bulk re-fills the heap.
- **The back-off works as designed.** Under the sustained 429, failed grows stayed at 5 per run (8, 16, 32, ...
  accepted bulks between tries), not one every 9 bulks. No variant looped or failed; all runs wrote 20,000 docs.

## Sustained 429 (part c)

1. **ES 8.19.23 256m with `indexing_pressure.memory.limit=2mb`, `--serverLimits=auto`.** No 429 at all on the
   load: auto reads the 2 MB limit and sizes bulks from it. This shows the probe is working, so it is not a
   sustained case.
2. **The same with `--serverLimits=off`** (16 MB bulks against a 2 MB limit, a real sustained 429; load, 4 reps):

   | variant | median s | size 429s/run | splits | grows | failed grows | lowest cap |
   |---|---:|---:|---:|---:|---:|---|
   | 1.2.0 off | 23.9 | 9.0 | 9.0 | 0 | 0 | 1021 KB |
   | 1.3 floor off | 23.6 | 9.0 | 9.0 | 0 | 0 | 1024 KB |
   | floor+gb off | **33.8** | 14.0 | 14.0 | 5.0 | 5.0 | 1024 KB |
   | floor+rf+gb off | **45.8** | 20.0 | 14.0 | 5.0 | 5.0 | 1024 KB |

   Grow-back costs +43 % here and retryfirst+growback +94 %. Neither failed or looped, and all 20,000 documents
   were written in every run.
3. **ES 8.19 256m with `indexing_pressure.memory.limit=1mb`, auto, load.** No 429: 1.2.0 7.5 s, floor 4.9 s,
   gb 5.0 s, rf+gb 4.9 s.
4. **ES 8.19 256m with `indexing_pressure.memory.limit=2mb`, auto, multi8.** This found a floor problem, fixed in
   L3-017. With the L3-014 floor, each of 8 workers got a 1 MiB bulk, and 8 x 1 MiB overran the node-wide 2 MB
   limit: 7.8 splits per run, 1.3 floor median 6.3 s against 1.2.0 2.0 s (3x slower).

   **Fix** (`limits::bulk_bytes_with`): the per-request bulk floor is now `min(1 MiB, budget / workers)`, at least
   256 KiB. The floor still applies in full when the indexing-pressure budget allows it. For example, OS3 256m:
   25 MB x 0.5 / 8 workers = 1.5 MB, so it is unchanged and the OS3 rows above stand.

   Before/after, rerun with the fixed build (`bin/l3gb2`), 4 reps:

   | variant | median s | min-max | size 429s/run | splits |
   |---|---:|---|---:|---:|
   | 1.2.0 auto | 1.4 | 1.3-2.0 | 0 | 0 |
   | 1.3 floor, before (L3-014) | 6.2 | 5.1-6.4 | 8.0 | 8.0 |
   | **1.3 floor, after (L3-017)** | **1.5** | 1.5-1.6 | **0** | 0 |
   | after + gb | 1.4 | 1.3-1.6 | 0 | 0 |
   | after + rf + gb | 1.4 | 1.3-1.5 | 0 | 0 |
5. **OS3 256m `off`, load.** 1.2.0 7.4 s, floor 10.1 s, gb 6.9 s, rf+gb 8.9 s. There are few 429s (0.8-2.2 per
   run), so this is not truly sustained, and the medians are within the bimodal noise.

## Other observations (not caused by the 429 policy)

There were two very long multi runs where the child counters were normal:
- **1.2.0, OS2 256m multi8, rep 0: 558.6 s.** The children finished in about 2 s (20:42:43); the parent only
  exited at 20:51:59.
- **floor+gb off, sustained, rep 2: 227 s.** Its counters were the same as the other reps (14 splits, 5 failed
  grows, about 12 s of penalty).

I did not keep the parent output, so the cause is unknown. It looks like the 1.3-G stalls (the parent or the engine
waits, not the data path). Follow-up: rerun with the parent's stderr saved.

## Recommendation for decision 2

1. **Do not make grow-back the default.**
   - On the case it was meant for (OS3 256m), grow-back alone is slower than the floor alone: 1.51x on load and
     1.41x on multi8.
   - Each failed grow costs about 2 s, plus extra breaker trips on a heap that is already full.
   - Under a sustained 429, grow-back costs +43 %.
   - It helps only where the heap has room to spare (OS3 384m multi8 0.55x, 512m multi8 0.75x), and those cells
     are the noisiest.
2. **Do not make retryfirst+growback the default either.**
   - It is the best variant on OS3 256m/384m (0.70-0.98x of floor).
   - It is 1.34x slower on OS3 512m load and 1.94x under a sustained 429, because the resend is refused again
     with a second back-off.
   - It mostly works by never splitting, so its grow-back rarely runs.
3. **Ship 1.3 with the floor only** (decision 3, plus this L3-017 worker-share fix). Keep `retryfirst` and
   `growback` as opt-in experiments.
4. If something should become a default later:
   - make it **cause-aware**: resend at the same size only for a parent-breaker 429 whose "real usage" is above the
     limit (garbage), and never for indexing-pressure 429s, which are sustained by nature;
   - make a grow probe cheaper: a back-off of about 250 ms for the grow probe instead of `--retryDelay`.

   That needs one more round of these reps. This is optional and not started.

## All counters (gb_table.py)

| case | variant | n | median s | min-max s | ok (exit 0 + count) | size 429s/run | splits/run | retryfirst/run | grows/run | failed grows/run | lowest cap KB | final cap KB (median) | parent trips/run | heap peak % |
|---|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| os3-256m-load | 1.2.0-auto | 4 | 15.7 | 14.2-17.1 | 4/4 | 4.5 | 4.5 | 0.0 | 0.0 | 0.0 | 169 | 298 | 13.5 | 94 |
| os3-256m-load | 1.3-floor | 4 | 15.0 | 4.5-17.1 | 4/4 | 3.8 | 3.8 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 9.5 | 94 |
| os3-256m-load | 1.3-floor+gb | 4 | 22.6 | 12.8-27.5 | 4/4 | 8.8 | 8.8 | 0.0 | 5.2 | 2.8 | 1024 | 2048 | 21.5 | 94 |
| os3-256m-load | 1.3-floor+rf+gb | 4 | 14.1 | 6.7-17.3 | 4/4 | 4.2 | 0.0 | 4.2 | 0.0 | 0.0 | - | 6400 | 9.8 | 94 |
| os3-256m-multi8 | 1.2.0-auto | 4 | 5.5 | 1.7-6.9 | 4/4 | 6.5 | 6.5 | 0.0 | 0.0 | 0.0 | 388 | 794 | 7.2 | 92 |
| os3-256m-multi8 | 1.3-floor | 4 | 5.3 | 4.2-6.7 | 4/4 | 7.5 | 7.5 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 7.5 | 85 |
| os3-256m-multi8 | 1.3-floor+gb | 4 | 7.5 | 5.9-9.6 | 4/4 | 12.2 | 12.2 | 0.0 | 8.5 | 3.2 | 1024 | 1024 | 15.0 | 86 |
| os3-256m-multi8 | 1.3-floor+rf+gb | 4 | 5.0 | 4.0-6.7 | 4/4 | 9.0 | 0.2 | 8.8 | 0.2 | 0.0 | 1024 | 1600 | 9.0 | 88 |
| os3-384m-load | 1.2.0-auto | 4 | 11.4 | 4.4-22.7 | 4/4 | 3.0 | 3.0 | 0.0 | 0.0 | 0.0 | 44 | 22 | 10.0 | 94 |
| os3-384m-load | 1.3-floor | 4 | 14.9 | 4.4-25.4 | 4/4 | 5.2 | 5.2 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 15.5 | 95 |
| os3-384m-load | 1.3-floor+gb | 4 | 13.7 | 4.4-31.3 | 4/4 | 4.8 | 4.8 | 0.0 | 3.5 | 1.5 | 1024 | 1024 | 17.5 | 94 |
| os3-384m-load | 1.3-floor+rf+gb | 4 | 10.4 | 4.5-17.1 | 4/4 | 2.8 | 1.0 | 1.8 | 1.0 | 0.2 | 2401 | 1200 | 9.5 | 94 |
| os3-384m-multi8 | 1.2.0-auto | 4 | 6.4 | 1.6-12.9 | 3/4 | 13.8 | 13.8 | 0.0 | 0.0 | 0.0 | 71 | 140 | 19.5 | 88 |
| os3-384m-multi8 | 1.3-floor | 4 | 5.2 | 1.4-13.9 | 4/4 | 8.8 | 8.8 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 15.0 | 93 |
| os3-384m-multi8 | 1.3-floor+gb | 4 | 2.9 | 1.3-10.9 | 4/4 | 5.8 | 5.8 | 0.0 | 4.2 | 1.2 | 1024 | 512 | 10.2 | 85 |
| os3-384m-multi8 | 1.3-floor+rf+gb | 4 | 5.1 | 4.1-6.9 | 4/4 | 7.0 | 0.0 | 7.0 | 0.0 | 0.0 | - | 2400 | 7.8 | 91 |
| os3-512m-load | 1.2.0-auto | 4 | 15.2 | 14.2-16.0 | 4/4 | 5.2 | 5.2 | 0.0 | 0.0 | 0.0 | 188 | 374 | 12.8 | 94 |
| os3-512m-load | 1.3-floor | 4 | 15.2 | 13.1-21.7 | 4/4 | 6.0 | 6.0 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 18.0 | 94 |
| os3-512m-load | 1.3-floor+gb | 4 | 16.0 | 14.4-17.4 | 4/4 | 5.5 | 5.5 | 0.0 | 4.5 | 1.8 | 1114 | 3895 | 17.2 | 94 |
| os3-512m-load | 1.3-floor+rf+gb | 4 | 20.4 | 17.2-20.8 | 4/4 | 8.0 | 4.2 | 3.8 | 3.0 | 2.0 | 1602 | 4332 | 19.5 | 94 |
| os3-512m-multi8 | 1.2.0-auto | 4 | 5.2 | 2.8-8.3 | 4/4 | 4.5 | 4.5 | 0.0 | 0.0 | 0.0 | 398 | 412 | 6.2 | 80 |
| os3-512m-multi8 | 1.3-floor | 4 | 10.8 | 4.4-70.7 | 4/4 | 10.8 | 10.8 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 14.2 | 94 |
| os3-512m-multi8 | 1.3-floor+gb | 4 | 8.1 | 4.8-10.9 | 4/4 | 9.5 | 9.5 | 0.0 | 6.5 | 0.8 | 1024 | 1024 | 13.5 | 91 |
| os3-512m-multi8 | 1.3-floor+rf+gb | 4 | 6.5 | 4.5-7.0 | 4/4 | 3.5 | 0.2 | 3.2 | 0.2 | 0.0 | 1588 | 3188 | 4.2 | 94 |
| os2-256m-load | 1.2.0-auto | 4 | 4.7 | 4.6-4.9 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 78 |
| os2-256m-load | 1.3-floor | 4 | 4.7 | 4.5-4.9 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 78 |
| os2-256m-load | 1.3-floor+gb | 4 | 4.7 | 4.6-4.8 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 78 |
| os2-256m-load | 1.3-floor+rf+gb | 4 | 4.8 | 4.7-5.0 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 75 |
| os2-256m-multi8 | 1.2.0-auto | 4 | 4.3 | 3.9-558.6 | 4/4 | 4.0 | 4.0 | 0.0 | 0.0 | 0.0 | 788 | 793 | 4.0 | 87 |
| os2-256m-multi8 | 1.3-floor | 4 | 5.7 | 3.6-14.5 | 4/4 | 3.0 | 3.0 | 0.0 | 0.0 | 0.0 | 1024 | 512 | 3.0 | 86 |
| os2-256m-multi8 | 1.3-floor+gb | 4 | 9.5 | 4.2-16.7 | 4/4 | 4.5 | 4.5 | 0.0 | 4.5 | 0.0 | 1024 | 1600 | 4.5 | 95 |
| os2-256m-multi8 | 1.3-floor+rf+gb | 4 | 4.1 | 3.3-23.7 | 4/4 | 4.2 | 0.0 | 4.2 | 0.0 | 0.0 | - | 1600 | 4.2 | 84 |
| es819-256m-load | 1.2.0-auto | 4 | 4.5 | 4.4-4.5 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 83 |
| es819-256m-load | 1.3-floor | 4 | 4.4 | 4.4-4.5 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 83 |
| es819-256m-load | 1.3-floor+gb | 4 | 4.4 | 4.3-4.5 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 83 |
| es819-256m-load | 1.3-floor+rf+gb | 4 | 4.4 | 4.4-4.5 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 84 |
| es819-256m-multi8 | 1.2.0-auto | 4 | 1.2 | 1.2-1.5 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 79 |
| es819-256m-multi8 | 1.3-floor | 4 | 1.2 | 1.1-1.2 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 74 |
| es819-256m-multi8 | 1.3-floor+gb | 4 | 1.2 | 1.2-1.3 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 74 |
| es819-256m-multi8 | 1.3-floor+rf+gb | 4 | 1.2 | 1.2-1.3 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 79 |
| sustained-es819-256m-ip2mb-load | 1.2.0-auto | 4 | 5.8 | 5.2-6.1 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 83 |
| sustained-es819-256m-ip2mb-load | 1.3-floor | 4 | 4.8 | 4.5-4.8 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 81 |
| sustained-es819-256m-ip2mb-load | 1.3-floor+gb | 4 | 4.7 | 4.4-4.8 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 85 |
| sustained-es819-256m-ip2mb-load | 1.3-floor+rf+gb | 4 | 4.8 | 4.4-4.9 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 80 |
| sustained-es819-256m-ip2mb-multi8 | 1.2.0-auto | 4 | 2.0 | 1.6-5.8 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 80 |
| sustained-es819-256m-ip2mb-multi8 | 1.3-floor | 4 | 6.3 | 5.9-15.1 | 4/4 | 7.8 | 7.8 | 0.0 | 0.0 | 0.0 | 1024 | 1000 | 0.0 | 83 |
| sustained-es819-256m-ip2mb-multi8 | 1.3-floor+gb | 4 | 5.5 | 4.6-6.0 | 4/4 | 7.8 | 7.8 | 0.0 | 6.2 | 0.0 | 1024 | 1000 | 0.0 | 83 |
| sustained-es819-256m-ip2mb-multi8 | 1.3-floor+rf+gb | 4 | 5.2 | 4.8-5.4 | 4/4 | 8.0 | 1.5 | 6.5 | 1.2 | 0.0 | 1024 | 1000 | 0.0 | 85 |
| sustained-es819-256m-ip2mb-off-load | 1.3-floor-off | 4 | 23.6 | 21.6-25.3 | 4/4 | 9.0 | 9.0 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 0.0 | 84 |
| sustained-es819-256m-ip2mb-off-load | 1.3-floor+gb-off | 4 | 33.8 | 31.7-227.2 | 4/4 | 14.0 | 14.0 | 0.0 | 5.0 | 5.0 | 1024 | 1024 | 0.0 | 84 |
| sustained-es819-256m-ip2mb-off-load | 1.3-floor+rf+gb-off | 4 | 45.8 | 42.8-53.9 | 4/4 | 20.0 | 14.0 | 6.0 | 5.0 | 5.0 | 1024 | 1024 | 0.0 | 85 |
| sustained-es819-256m-ip2mb-off-load | 1.2.0-off | 4 | 23.9 | 20.6-25.0 | 4/4 | 9.0 | 9.0 | 0.0 | 0.0 | 0.0 | 1021 | 1021 | 0.0 | 83 |
| sustained-es819-256m-ip1mb-auto-load | 1.2.0-auto | 4 | 7.5 | 6.5-8.3 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 84 |
| sustained-es819-256m-ip1mb-auto-load | 1.3-floor | 4 | 4.9 | 4.7-5.4 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 84 |
| sustained-es819-256m-ip1mb-auto-load | 1.3-floor+gb | 4 | 5.0 | 4.9-5.0 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 83 |
| sustained-es819-256m-ip1mb-auto-load | 1.3-floor+rf+gb | 4 | 4.9 | 4.7-5.2 | 4/4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | - | 0 | 0.0 | 84 |
| sustained-os3-256m-off-load | 1.3-floor-off | 4 | 10.1 | 6.1-14.6 | 4/4 | 2.2 | 2.2 | 0.0 | 0.0 | 0.0 | 1024 | 1024 | 2.5 | 94 |
| sustained-os3-256m-off-load | 1.3-floor+gb-off | 4 | 6.9 | 4.7-8.5 | 4/4 | 0.8 | 0.8 | 0.0 | 1.0 | 0.2 | 6966 | 6966 | 0.8 | 92 |
| sustained-os3-256m-off-load | 1.3-floor+rf+gb-off | 4 | 8.9 | 6.9-10.0 | 4/4 | 1.8 | 0.0 | 1.8 | 0.0 | 0.0 | - | 15625 | 1.8 | 92 |
| sustained-os3-256m-off-load | 1.2.0-off | 4 | 7.4 | 4.7-10.7 | 4/4 | 1.0 | 1.0 | 0.0 | 0.0 | 0.0 | 3899 | 5851 | 1.0 | 88 |

