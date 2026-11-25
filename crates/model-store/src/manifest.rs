//! The compiled-in model table (ADR 0006, ADR 0012, D-52, D-60).
//!
//! This is the only place that names where a model comes from. The webview, the
//! settings and the environment never supply a url, a host, a path or a checksum:
//! the shell resolves a model id against `ENTRIES` and nothing else.
//!
//! The url strings and the pinned host list below contain the names of the model
//! sources. Guardrail 3 allows that in this file only, and only in those two
//! places. The sources are provisional until the M0-19 decision.

/// One file of a pack: where it comes from and what it must hash to. The
/// downloader task (M1-04.4) replaces the single-file `ModelSpec` of `lib.rs` with
/// this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Path of the file inside the pack folder. Relative, no `..`.
    pub rel_path: &'static str,
    /// `https` url of the file on a pinned host.
    pub url: &'static str,
    /// Pinned SHA-256 of the file, 64 hex characters.
    pub sha256_hex: &'static str,
    /// Exact size in bytes. It is also the size cap of the download.
    pub size_bytes: u64,
}

/// A set of files that are downloaded, stored and deleted together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pack {
    pub id: &'static str,
    pub files: &'static [ModelFile],
}

/// One model the owner sees in Setup. Its id is a `SetupItemId` of the models group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// Short English licence line for the licence screen.
    pub licence: &'static str,
    /// True when the licence forbids commercial use (D-47).
    pub non_commercial: bool,
    /// Id of the pack that holds this model's files.
    pub pack: &'static str,
}

/// The hosts a download may contact: the hosts of the urls below and the content
/// host that they redirect to. A host that is not here fails the download.
pub(crate) const PINNED_HOSTS: &[&str] = &[
    "huggingface.co",
    "us.aws.cdn.hf.co",
    "raw.githubusercontent.com",
];

pub(crate) static PACKS: &[Pack] = &[
    Pack {
        id: "asr",
        files: &[
            ModelFile {
                rel_path: "config.json",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/config.json",
                sha256_hex: "bf62b79a84618ca9ed989f31968d8f10297ccb9c1d76b0b286baa37c03c300ea",
                size_bytes: 1309,
            },
            ModelFile {
                rel_path: "decoder.onnx",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/onnx/decoder_model_merged.onnx",
                sha256_hex: "c583ce183f56a5634dce43b9c7d947d9e5e6023c8cd4fa602f94370da9f48fbc",
                size_bytes: 208446607,
            },
            ModelFile {
                rel_path: "encoder.onnx",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/onnx/encoder_model.onnx",
                sha256_hex: "dba237ee167a37803a527198151c3df8664bb222dfb6dff9b5eedfa06be379f5",
                size_bytes: 82435426,
            },
            ModelFile {
                rel_path: "generation_config.json",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/generation_config.json",
                sha256_hex: "1a4bbfad4dfed0ceceea7edfe95f3eb87c031fc82535f61b0418957c14b93d7d",
                size_bytes: 3801,
            },
            ModelFile {
                rel_path: "preprocessor_config.json",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/preprocessor_config.json",
                sha256_hex: "a6a76d28c93edb273669eb9e0b0636a2bddbb1272c3261e47b7ca6dfdbac1b8d",
                size_bytes: 339,
            },
            ModelFile {
                rel_path: "tokenizer.json",
                url: "https://huggingface.co/onnx-community/PhoWhisper-base-ONNX/resolve/41a94b878c46ec2ab28e0ebea0915369f0758380/tokenizer.json",
                sha256_hex: "ef1cbdefa54ee88b08dccac83fc994fa69e45988abf0f46fecfeb2e08ac9e306",
                size_bytes: 3654002,
            },
            ModelFile {
                rel_path: "vad/vad.onnx",
                url: "https://raw.githubusercontent.com/snakers4/silero-vad/5cd7945676eb32225748052e2e6a0580e4686a08/src/silero_vad/data/silero_vad.onnx",
                sha256_hex: "1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3",
                size_bytes: 2327524,
            },
        ],
    },
    Pack {
        id: "translation",
        files: &[
            ModelFile {
                rel_path: "config.json",
                url: "https://huggingface.co/Xenova/opus-mt-vi-en/resolve/541dfd72ec07e8f1563795e0224408ad8d729e80/config.json",
                sha256_hex: "c62a203a0183755af37b96a33cc125e7f74f97c1e90aaae1d1f404fd8a42eea8",
                size_bytes: 1389,
            },
            ModelFile {
                rel_path: "decoder.onnx",
                url: "https://huggingface.co/Xenova/opus-mt-vi-en/resolve/541dfd72ec07e8f1563795e0224408ad8d729e80/onnx/decoder_model_merged.onnx",
                sha256_hex: "a0b29e9729f8b2c3467f9601c37bf7a4b490aaca386ac4c7571e8a8190e2e05f",
                size_bytes: 212729612,
            },
            ModelFile {
                rel_path: "encoder.onnx",
                url: "https://huggingface.co/Xenova/opus-mt-vi-en/resolve/541dfd72ec07e8f1563795e0224408ad8d729e80/onnx/encoder_model.onnx",
                sha256_hex: "2a8ec4bea7eca3e45195cb4c498371af13e2c923fab3f107199fcbdb419c7bea",
                size_bytes: 186873644,
            },
            ModelFile {
                rel_path: "generation_config.json",
                url: "https://huggingface.co/Xenova/opus-mt-vi-en/resolve/541dfd72ec07e8f1563795e0224408ad8d729e80/generation_config.json",
                sha256_hex: "44f2c9011a9087c5ec93a1671066a4a59a7a8c00c03e84a6ecac0d840926566b",
                size_bytes: 293,
            },
            ModelFile {
                rel_path: "tokenizer.json",
                url: "https://huggingface.co/Xenova/opus-mt-vi-en/resolve/541dfd72ec07e8f1563795e0224408ad8d729e80/tokenizer.json",
                sha256_hex: "c41cc5afd26c053770b60e1031577bb7a8e2f3c2eeacde2a34f5c15abfd4eafd",
                size_bytes: 5169708,
            },
        ],
    },
    Pack {
        id: "voice",
        files: &[
            ModelFile {
                rel_path: "codec/decode_full.onnx",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/codec/moss_audio_tokenizer_decode_full.onnx",
                sha256_hex: "0fbbafe3fd4afa2a019af5c5ced204af6e2d1db044fa40f021525d2aee95b4ac",
                size_bytes: 681902,
            },
            ModelFile {
                rel_path: "codec/decode_shared.data",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/codec/moss_audio_tokenizer_decode_shared.data",
                sha256_hex: "e69d52e0f4e84ca27850557ee54face46632d3a5a16c89bd246c7c408466dcad",
                size_bytes: 44198912,
            },
            ModelFile {
                rel_path: "codec/decode_step.onnx",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/codec/moss_audio_tokenizer_decode_step.onnx",
                sha256_hex: "9527c86a29e1837edec1f74db57d5eeaadb3a715af3382703566460afed25855",
                size_bytes: 351400,
            },
            ModelFile {
                rel_path: "config.json",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/config.json",
                sha256_hex: "3676da6a9f4dba7a8f2d106d5c1fb02c491391dc19c91dae893133a7e219d666",
                size_bytes: 378,
            },
            ModelFile {
                rel_path: "local_frame_decode.onnx",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/local_frame_decode.onnx",
                sha256_hex: "3c4540ef4e69dcf604dc6f60f6f99f119189694ff5f31113def20406e274f025",
                size_bytes: 186743011,
            },
            ModelFile {
                rel_path: "null_voice_emb.npy",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/null_voice_emb.npy",
                sha256_hex: "ec014c14f79e8fc16f4ca6ea557f33fa2cbd7d448b5de5f467aace9ca0321e9f",
                size_bytes: 30848,
            },
            ModelFile {
                rel_path: "prefix_step.onnx",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/prefix_step.onnx",
                sha256_hex: "b7544c9fdd3535b21fe8dea855407180ad2084f9da298f8a153893522496565b",
                size_bytes: 348303894,
            },
            ModelFile {
                rel_path: "silence_frame.npy",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/silence_frame.npy",
                sha256_hex: "bb2628efa99ae02819b04ff7509ef43ddb3213a9fb0916e4e88b8d1a7c3a1810",
                size_bytes: 256,
            },
            ModelFile {
                rel_path: "text_encoder.onnx",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/onnx/text_encoder.onnx",
                sha256_hex: "d37557a4abe07953a02686a176664486e73838389a14ff4a3f36b47ac2bccd52",
                size_bytes: 323096725,
            },
            ModelFile {
                rel_path: "tokenizer.json",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/tokenizer.json",
                sha256_hex: "4fd646e8a1fd6694cb9c876c914a516ef8dd84b4f84db2605be1a7388885ae91",
                size_bytes: 191542,
            },
            ModelFile {
                rel_path: "voices/preset.bin",
                url: "https://huggingface.co/zeroweight-ai/ZeroTTS/resolve/18d6681518a4becaed3c125ceb3c7db59905fe86/voices/hamy/voice.bin",
                sha256_hex: "e137f9544c20c491b3d139aeebb1d8549f8bd6f84f035a2df78830d3baec025d",
                size_bytes: 30720,
            },
        ],
    },
];

pub(crate) static ENTRIES: &[ModelEntry] = &[
    ModelEntry {
        id: "sr",
        name: "Speech recognition",
        licence: "BSD-3-Clause (recognition), MIT (speech gate)",
        non_commercial: false,
        pack: "asr",
    },
    ModelEntry {
        id: "tr",
        name: "Translation",
        licence: "Apache-2.0. Training data is CC BY-NC-SA: non-commercial use only",
        non_commercial: true,
        pack: "translation",
    },
    ModelEntry {
        id: "enV",
        name: "English voice",
        licence: "MIT (weights and code), Apache-2.0 (audio codec)",
        non_commercial: false,
        pack: "voice",
    },
    ModelEntry {
        id: "viV",
        name: "Vietnamese voice",
        licence: "MIT (weights and code), Apache-2.0 (audio codec)",
        non_commercial: false,
        pack: "voice",
    },
];

#[cfg(test)]
mod tests {
    use crate::{entries, entry_for, pack_of, packs, pinned_hosts};
    use meeting_core::policy::PrivacyPolicy;
    use std::collections::BTreeSet;

    #[test]
    fn table_holds_four_ids_in_order_and_three_packs() {
        let ids: Vec<&str> = entries().iter().map(|e| e.id).collect();
        assert_eq!(ids, ["sr", "tr", "enV", "viV"]);
        let pack_ids: Vec<&str> = packs().iter().map(|p| p.id).collect();
        assert_eq!(pack_ids, ["asr", "translation", "voice"]);
    }

    #[test]
    fn voice_pack_is_shared() {
        let en = entry_for("enV").unwrap();
        let vi = entry_for("viV").unwrap();
        assert_eq!(en.pack, "voice");
        assert_eq!(vi.pack, "voice");
        assert_eq!(pack_of(en).id, pack_of(vi).id);
    }

    #[test]
    fn sr_pack_has_the_voice_activity_file() {
        let sr = entry_for("sr").unwrap();
        let pack = pack_of(sr);
        let vad: Vec<_> = pack
            .files
            .iter()
            .filter(|f| f.rel_path == "vad/vad.onnx")
            .collect();
        assert_eq!(vad.len(), 1);
        assert!(vad[0].size_bytes > 0);
    }

    #[test]
    fn every_entry_names_an_existing_pack() {
        for e in entries() {
            assert!(
                packs().iter().any(|p| p.id == e.pack),
                "{} names the missing pack {}",
                e.id,
                e.pack
            );
        }
        for p in packs() {
            assert!(
                entries().iter().any(|e| e.pack == p.id),
                "pack {} has no entry",
                p.id
            );
        }
    }

    fn safe_rel_path(p: &str) -> bool {
        !p.is_empty()
            && !p.starts_with('/')
            && !p.contains('\\')
            && p.split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
            && !p
                .split('/')
                .any(|part| part.ends_with(".part") || part == ".receipt.json")
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/-".contains(&b))
    }

    #[test]
    fn every_file_is_pinned_https_listed_and_safe_path() {
        assert!(!packs().is_empty(), "the table is empty");
        for pack in packs() {
            assert!(!pack.files.is_empty(), "pack {} has no files", pack.id);
            let mut seen = BTreeSet::new();
            for f in pack.files {
                assert!(
                    crate::is_sha256_hex(f.sha256_hex),
                    "{} has a bad pin",
                    f.rel_path
                );
                assert!(f.size_bytes > 0, "{} has no size", f.rel_path);
                assert!(safe_rel_path(f.rel_path), "{} is not safe", f.rel_path);
                assert!(
                    seen.insert(f.rel_path),
                    "{} appears twice in {}",
                    f.rel_path,
                    pack.id
                );
                assert!(
                    !pack
                        .files
                        .iter()
                        .any(|o| o.rel_path.starts_with(&format!("{}/", f.rel_path))),
                    "{} is both a file and a folder in {}",
                    f.rel_path,
                    pack.id
                );
                assert!(
                    !f.url.contains(['@', '?', '#']),
                    "{} has userinfo, a query or a fragment",
                    f.url
                );
                assert!(
                    f.url
                        .split('/')
                        .any(|seg| seg.len() == 40 && seg.bytes().all(|b| b.is_ascii_hexdigit())),
                    "{} is not pinned to a 40-hex revision",
                    f.url
                );
                let host = crate::https_host(f.url).unwrap();
                assert!(
                    pinned_hosts().contains(&host.as_str()),
                    "{host} is not a pinned host ({})",
                    f.rel_path
                );
            }
        }
    }

    #[test]
    fn the_pinned_hosts_are_valid_unique_and_lower_case() {
        assert!(!pinned_hosts().is_empty());
        let mut policy = PrivacyPolicy::local_only();
        for h in pinned_hosts() {
            policy.allow_model_host(h).unwrap();
            assert_eq!(*h, h.to_ascii_lowercase(), "{h} is not lower case");
        }
        assert_eq!(
            pinned_hosts().iter().collect::<BTreeSet<_>>().len(),
            pinned_hosts().len(),
            "a host is listed twice"
        );
    }

    #[test]
    fn only_an_exact_id_resolves() {
        for id in [
            "SR",
            "sr ",
            "",
            "../sr",
            "mic",
            "https://models.example.com/x.bin",
            "ENV",
            "viv",
        ] {
            assert!(entry_for(id).is_none(), "{id:?} must not resolve");
        }
        for id in ["sr", "tr", "enV", "viV"] {
            assert_eq!(entry_for(id).unwrap().id, id);
        }
    }

    #[test]
    fn only_the_translation_model_is_non_commercial() {
        let flagged: Vec<&str> = entries()
            .iter()
            .filter(|e| e.non_commercial)
            .map(|e| e.id)
            .collect();
        assert_eq!(flagged, ["tr"]);
        assert!(entries().iter().all(|e| !e.licence.trim().is_empty()));
    }
}
