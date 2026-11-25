Fixes #N

### Item
<!-- The backlog item's ID and title as the backlog writes them, `K-47: <subject>`. The pull request's title is `[measured] K-47: <subject>` or `[needs testing] K-47: <subject>`, the tag first. -->

### What the change does
<!-- Say what changed in sentences a reviewer can check: the kernel, op or step, and what it does differently. The merge bar is three things: (1) the output is byte-identical or inside the item's ORD/PF bound (parity, §2.1); (2) the targeted kernel, op or step is faster in ms (AQL stamps, rad-kbench or profile-ops); (3) end-to-end tok/s and TTFT are not worse beyond noise. -->

### Tests I ran
<!-- The GPU-less checks of §6 step 2, a checklist with numbers, one run each:
- [x] `./build.sh` (or the CMake presets): builds
- [x] `tests/r4d_emu/`: 31 passed
- [x] libref / libavx: 12 passed
- [x] `tests/chain_scalar_audit.py`: clean
- [x] `hyv4flash_arch_test`: passed
- [x] `sched_test`: passed
- [x] the ctests that need no card: 58 passed -->

### Measurement
<!-- Start with one of:
`measured` and the path of the committed file, `notes/backlog/measurements/<item>.md` (under 40 KB);
`needs testing` (no GPU was available);
`not needed` and the reason.
The review counts no measurement from this body. The file holds the before commit (the merge base) and the after commit (the code commit), who measured, the GPUs (model, count, TP), the power cap, the ROCm or driver version, the container and the ngram placement, and at least 3 interleaved runs each, with the raw lines. -->
