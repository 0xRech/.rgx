# RGX benchmark snapshots

This page records observed benchmark runs for **RGX v0.4.0-alpha.2**. The built-in benchmark compares RGX, RGX Private, and ZIP/Deflate on the same data set and machine. 7-Zip is included automatically when a `7z`, `7zz`, or `7za` executable is available.

> These are real single-run wall-clock measurements, not a controlled laboratory benchmark. The Windows and macOS runs used different machines and slightly different input sets, so compare methods **within each platform run**, not Windows directly against macOS.

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

## Reproducing a run

```bash
rgx benchmark ./your-data --private
```

For useful comparisons, keep the input set, machine, storage device, power mode, and competing tool settings constant. Run each case multiple times when publishing performance claims and report the median or distribution rather than relying on one pass.
