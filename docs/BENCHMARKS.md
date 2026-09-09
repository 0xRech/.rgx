# RGX benchmark snapshots

This page records benchmark snapshots relevant to the current **RGX v0.5.0-alpha1** source. The Windows and macOS measurements below are retained real-machine baselines from **v0.4.0-alpha.2**; the recipient-mode section records the **v0.5.0-alpha1** synthetic implementation test.

> Benchmark snapshots are not universal performance claims. Compare methods **within the same run and platform**. Hardware, storage, cache state, data composition, and run order can materially affect timings.

## Windows x86-64

**Input:** 1.25 GiB / 3,113 files  
**Deduplicated logical data:** 71.99 MiB  
**Deduplicated share:** 5.61%  
**7-Zip:** not installed for this run

| Method | Archive size | Pack | Extract | Pack MiB/s | Extract MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| RGX | 289.34 MiB | 24.71 s | 15.57 s | 51.9 | 82.4 |
| RGX Private | 289.35 MiB | 14.08 s | 30.13 s | 91.1 | 42.6 |
| ZIP (Deflate) | 318.65 MiB | 37.95 s | 34.26 s | 33.8 | 37.5 |

Observed in this run:

- RGX produced an archive **9.2% smaller than ZIP/Deflate**.
- RGX packing throughput was about **1.54× ZIP**.
- RGX extraction throughput was about **2.20× ZIP**.
- RGX Private added only **0.01 MiB** to the archive size relative to plain RGX.
- The unusually faster Private packing result should not be interpreted as an encryption speed advantage; cache state, file-system effects, and run ordering can materially affect a single wall-clock measurement.

## macOS Apple Silicon

**Input:** 1.09 GiB / 3,413 files  
**Deduplicated logical data:** 229.99 MiB  
**Deduplicated share:** 20.56%  
**7-Zip:** not installed for this run

| Method | Archive size | Pack | Extract | Pack MiB/s | Extract MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| RGX | 248.82 MiB | 10.73 s | 2.12 s | 104.3 | 526.9 |
| RGX Private | 248.83 MiB | 11.01 s | 5.11 s | 101.7 | 219.0 |
| ZIP (Deflate) | 304.49 MiB | 22.03 s | 2.95 s | 50.8 | 379.1 |

Observed in this run:

- RGX produced an archive **18.3% smaller than ZIP/Deflate**.
- RGX packing throughput was about **2.05× ZIP**.
- RGX extraction throughput was about **1.39× ZIP**.
- RGX Private packing throughput was about **2.00× ZIP**, while Private extraction was slower than plain ZIP in this particular run because authenticated decryption adds work.
- RGX Private again added only **0.01 MiB** to the archive size relative to plain RGX.

## v0.5.0-alpha1 recipient implementation test

A deterministic synthetic Linux test used **109.02 MiB / 1,392 files**, mixing small text files, random 1 MiB files, repeated files, and compressible patterns. It produced a **77.77% deduplicated logical share**. Three runs were made on the same GitHub Actions runner; the table reports medians.

| Method | Archive size | Pack median | Extract median |
| --- | ---: | ---: | ---: |
| RGX | 24.18 MiB | 0.63 s | 0.18 s |
| RGX Recipient | 24.18 MiB | 0.64 s | 0.46 s |
| RGX Private | 24.18 MiB | 0.74 s | 0.52 s |
| ZIP (Deflate) | 96.30 MiB | 3.26 s | 0.19 s |

On this synthetic dataset, native Recipient Mode packed about **13.5% faster** and extracted about **11.5% faster** than password-based Private Mode. The result is useful as an implementation snapshot, not as a universal speed claim.

## Reproducing a run

```bash
rgx benchmark ./your-data --private
```

For useful comparisons, keep the input set, machine, storage device, power mode, and competing tool settings constant. Run each case multiple times when publishing performance claims and report the median or distribution rather than relying on one pass.
