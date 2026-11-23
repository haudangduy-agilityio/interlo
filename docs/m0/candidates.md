This file is the private licence record for NOTICE. It names the models that were studied; do not copy these names into other docs.

# M0-02: Candidate sheet and licence registry

Status date: 2026-11-16. Owner agreed to the lighter set on 2026-11-16.

Sizes come from public model listings and are not measured. Licence status:

- **read**: the official LICENSE file or the official licence statement was read.
- **tag-read**: the model card licence field was read, but no LICENSE file was read.
- **open**: not yet read.

Rule for M0: a candidate is used by the product only after its status is `read` or `tag-read` and no conflict is open. Non-commercial terms are allowed for personal use and are flagged (M-Q40). Unknown terms are not allowed.

## Candidate table

| Stage | Candidate | Size (approx.) | Licence | Status | Notes |
|---|---|---|---|---|---|
| Speech gate (VAD) | Silero VAD, ONNX | about 2 MB | Code: MIT (LICENSE read, Copyright 2020-present Silero Team). README: "Published under permissive license (MIT)". | read | One badge alt text in the README says CC BY-NC 4.0. The visible badge says MIT. Keep MIT, note the badge in the registry. |
| Vietnamese recognition | PhoWhisper small | 244M params, about 466 MB (Whisper-size estimate) | BSD-3-Clause (LICENSE read, Copyright 2024 VinAI; model card front matter `bsd-3-clause`) | read | Model files are on Hugging Face. No LICENSE file linked on the model page; the GitHub LICENSE covers the code. |
| Vietnamese recognition | PhoWhisper base | 74M params, about 142 MB (estimate) | BSD-3-Clause (same family) | tag-read | Same family card as small. Confirm the card of the base repo before use. |
| Vietnamese recognition | PhoWhisper tiny | 39M params, about 75 MB (estimate) | BSD-3-Clause (same family) | tag-read | Speed test only. |
| Vietnamese recognition | PhoWhisper medium / large | 769M / 1.55B params, about 1.5 GB / 2.9 GB | not read | open | Only if small and base fail M0-05. |
| Translation vi to en | opus-mt-vi-en (Marian, Helsinki-NLP) | about 289 MB (PyTorch weights) | Apache-2.0 (model card tag). The card has no licence text. | tag-read | Read the OPUS data licences before shipping (the models are trained on OPUS corpora with their own terms). |
| Translation en to vi | opus-mt-en-vi | about 289 MB (community safetensors match) | Apache-2.0 (model card tag) | tag-read | Same note as vi-en. |
| Translation vi to en (alternative) | VinAI vinai-translate-vi2en | 448M params; 1.69 GB (32-bit); about 274 MB quantized ONNX (community export) | GNU AGPL-3.0 (VinAI GitHub README) | **excluded from shipping** | Copyleft. Personal runs only, if the owner accepts it. |
| Vietnamese voice | ZeroTTS (owner's choice) | weights fp32 about 900 MB (README). GGUF f32 about 772 MB; q8_0 about 206 MB; q4_0 about 124 MB. ONNX int8 not found. | Code: MIT (LICENSE read, Copyright 2026 ZeroWeight AI). Weights: MIT (model card). Card says the CC-BY-NC-4.0 dataset licence "does not apply to these weights". onnx/codec decoder graphs: Apache-2.0 (from OpenMOSS MOSS-Audio-Tokenizer-Nano). | read (code), tag-read (weights) |  The product runtime is ONNX (D-14); GGUF is a measurement-only path. |
| English voice | ZeroTTS (owner's choice; owner reports it reads English well) | same model as the Vietnamese voice | MIT (weights and code, read) | read | Offline only. edge-tts rejected: it sends text to a network service during meetings (G6). Status: English voice decided by the owner; whole English sentences verified by listening at M0-07; no fallback voice (D-30). Evidence checked 2026-11-17 is in the section "ZeroTTS evidence" below. |

## Corrections to the earlier sheet

- ZeroTTS weights: the earlier note said MIT from a mirror. The official model card says MIT.
- Silero VAD: the earlier conflict was a badge alt text. The LICENSE and visible text say MIT.
- PhoWhisper: the earlier BSD-3-Clause tag is now confirmed from the LICENSE file.

## ZeroTTS evidence (checked 2026-11-17)

Sources: the project's GitHub page and its Hugging Face model card. Applies to the Vietnamese voice row and the English voice row above.

| Item | What the sources say |
|---|---|
| Licence | MIT |
| Languages in the model card | Vietnamese only (`vi`) |
| README wording | "Built for Vietnamese - tones, code-switched English" |
| Benchmark | Word error rate 0.97 percent on code-switch text. Raw text from the README; the model card does not verify it. |
| English benchmark | None for whole English sentences |
| Voice cloning | Not available in the released package. Eight preset voices with gender tags. |
| Speed | First audio about 70 ms on CPU; real time factor 0.5 |
| Size | fp32 ONNX about 900 MB |
| English text | The README tells users to turn off Vietnamese text normalisation for English text |
| Status | English voice decided by the owner; whole English sentences verified by listening at M0-07; no fallback voice (D-30) |

## Totals for the lighter set (estimate)

| Part | Choice | Size |
|---|---|---|
| Speech gate | Silero VAD | about 2 MB |
| Recognition | PhoWhisper small (or base) | about 466 MB (or about 142 MB) |
| Translation both ways | opus-mt vi-en and en-vi | about 578 MB |
| Vietnamese voice | ZeroTTS q8 | about 206 MB |
| English voice | not chosen | not counted |
| **Total** | | **about 0.9 to 1.3 GB, plus the English voice** |

Well under the 6 GB model cap (M-Q39). Data volume: 54 GiB free on 2026-11-16.

## Open checks

1. ZeroTTS ONNX int8 file size: check the zeroweight-ai/ZeroTTS repo files (G-12).
2. English voice choice (M0-07): owner listens (G-13).
3. PhoWhisper base and tiny cards (G-10).

## M1-35 provenance (downloaded 2026-11-23, owner approved the list)

The files that `bench/M1-35/` loads. Hashes are in `bench/M1-35/models.sha256`. The translation and
recognition files are community exports of the official weights, used for this spike only (the final model
source is decided at M1-04 and M0-19). Columns: task, role, source repository, files and bytes, revision.

| Task | Role | Source repository | Files and bytes | Revision |
|---|---|---|---|---|
| M1-35 | voice activity detection | github.com/snakers4/silero-vad (tag v6.2.3) | `src/silero_vad/data/silero_vad.onnx` 2327524 | 5cd7945676eb32225748052e2e6a0580e4686a08 |
| M1-35 | translation | huggingface.co/Xenova/opus-mt-vi-en | encoder 186873644, merged decoder 212729612, tokenizer 5169708, config 1389, generation config 293; total 404774646 | 541dfd72ec07e8f1563795e0224408ad8d729e80 |
| M1-35 | speech recognition | huggingface.co/onnx-community/PhoWhisper-base-ONNX | encoder 82435426, merged decoder 208446607, tokenizer 3654002, config 1309, generation config 3801, preprocessor config 339; total 294541484 | 41a94b878c46ec2ab28e0ebea0915369f0758380 |
| M1-35 | voice | huggingface.co/zeroweight-ai/ZeroTTS | text encoder 323096725, prefix step 348303894, local frame decode 186743011, codec decode full 681902, codec decode step 351400, codec shared weights 44198912, preset voice `voices/hamy/voice.bin` 30720, null voice 30848, silence frame 256, tokenizer 191542, config 378; total 903629588. `onnx/voice_encoder.onnx` (139853093) not downloaded: the reference code does not call it | 18d6681518a4becaed3c125ceb3c7db59905fe86 |

Notes from the run: the voice model card says the model is built for Vietnamese and is not evaluated as an
English voice (see the English voice row above and M0-07). The download hosts were `raw.githubusercontent.com`,
`huggingface.co` and its CDN `us.aws.cdn.hf.co`.
