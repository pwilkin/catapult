use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Well-known providers of GGUF models on HuggingFace
pub const KNOWN_GGUF_OWNERS: &[(&str, &str)] = &[
    ("unsloth", "Dominant quantizer, Unsloth Dynamic 2.0 quants"),
    ("bartowski", "High-quality imatrix quants, large catalog"),
    ("ggml-org", "Official llama.cpp org, reference quants"),
    ("lmstudio-community", "LM Studio curated models"),
    ("mradermacher", "Prolific community quantizer, i1 variants"),
    ("MaziyarPanahi", "Diverse GGUF collection"),
    ("mmnga", "Various quants including Japanese models"),
    ("QuantFactory", "Quantized models collection"),
];

/// Curated list of recommended models with metadata
pub const RECOMMENDED_MODELS: &[RecommendedModelDef] = &[
    // ── Lightweight (runs on anything) ──────────────────────
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3.5-4B-GGUF",
        filename: "Qwen3.5-4B-Q4_K_M.gguf",
        name: "Qwen 3.5 4B",
        description: "Lightweight but capable. Runs on minimal hardware.",
        params_b: 4,
        family: "Qwen 3.5",
        quant: "Q4_K_M",
        context: None,
    },
    // ── Mid-range (8-16 GB VRAM sweet spot) ─────────────────
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3.5-9B-GGUF",
        filename: "Qwen3.5-9B-Q4_K_M.gguf",
        name: "Qwen 3.5 9B",
        description: "Dense 9B model. Strong all-around performance.",
        params_b: 9,
        family: "Qwen 3.5",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/gpt-oss-20b-GGUF",
        filename: "gpt-oss-20b-Q4_K_M.gguf",
        name: "GPT-OSS 20B (MoE, 3.6B active)",
        description: "OpenAI's open-weight MoE model. Apache 2.0 license.",
        params_b: 20,
        family: "GPT-OSS",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3.5-35B-A3B-GGUF",
        filename: "Qwen3.5-35B-A3B-Q4_K_M.gguf",
        name: "Qwen 3.5 35B MoE (3B active)",
        description: "Most downloaded GGUF model. MoE with 3B active params — fast and smart.",
        params_b: 35,
        family: "Qwen 3.5",
        quant: "Q4_K_M",
        context: None,
    },
    // ── Coding ──────────────────────────────────────────────
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF",
        filename: "Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf",
        name: "Qwen3 Coder 30B MoE (3B active)",
        description: "Coding-focused MoE model with 3B active params.",
        params_b: 30,
        family: "Qwen3 Coder",
        quant: "Q4_K_M",
        context: None,
    },
    // ── Large (16-32 GB VRAM) ───────────────────────────────
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3.5-27B-GGUF",
        filename: "Qwen3.5-27B-Q4_K_M.gguf",
        name: "Qwen 3.5 27B",
        description: "Hybrid DeltaNet architecture. High capability dense model.",
        params_b: 27,
        family: "Qwen 3.5",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/gemma-3-27b-it-GGUF",
        filename: "gemma-3-27b-it-Q4_K_M.gguf",
        name: "Gemma 3 27B Instruct",
        description: "Google's latest 27B model. Well-rounded and capable.",
        params_b: 27,
        family: "Gemma 3",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/GLM-4.7-Flash-GGUF",
        filename: "GLM-4.7-Flash-Q4_K_M.gguf",
        name: "GLM 4.7 Flash (30B MoE, 3B active)",
        description: "Zhipu AI's fast MoE model. 3B active params, very efficient.",
        params_b: 30,
        family: "GLM 4.7",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/Nemotron-3-Nano-30B-A3B-GGUF",
        filename: "Nemotron-3-Nano-30B-A3B-Q4_K_M.gguf",
        name: "Nemotron 3 Nano (30B MoE, 3B active)",
        description: "NVIDIA's hybrid Mamba-2/Transformer MoE. Very fast inference.",
        params_b: 30,
        family: "Nemotron 3",
        quant: "Q4_K_M",
        context: None,
    },
    // ── Extra-large (48+ GB) ────────────────────────────────
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3-Coder-Next-GGUF",
        filename: "Qwen3-Coder-Next-Q4_K_M.gguf",
        name: "Qwen3 Coder Next (80B MoE, 3B active)",
        description: "Largest coding MoE. Needs ~48 GB but only 3B active params.",
        params_b: 80,
        family: "Qwen3 Coder",
        quant: "Q4_K_M",
        context: None,
    },
    RecommendedModelDef {
        repo_id: "unsloth/Qwen3.5-122B-A10B-GGUF",
        filename: "Qwen3.5-122B-A10B-Q4_K_M.gguf",
        name: "Qwen 3.5 122B MoE (10B active)",
        description: "Frontier-class MoE model. Needs ~76 GB. Top benchmark scores.",
        params_b: 122,
        family: "Qwen 3.5",
        quant: "Q4_K_M",
        context: None,
    },
];

pub struct RecommendedModelDef {
    pub repo_id: &'static str,
    pub filename: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub params_b: u32,
    pub family: &'static str,
    pub quant: &'static str,
    pub context: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModel {
    pub repo_id: String,
    pub name: String,
    pub author: String,
    pub tags: Vec<String>,
    pub files: Vec<HfFile>,
    pub downloads: u64,
    pub likes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFile {
    pub filename: String,
    pub size_bytes: u64,
    pub quant: Option<String>,
    pub download_url: String,
    #[serde(default)]
    pub is_split: bool,
    #[serde(default)]
    pub split_parts: Vec<HfFilePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFilePart {
    pub filename: String,
    pub size_bytes: u64,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HfApiModel {
    id: String,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    author: Option<String>,
    tags: Option<Vec<String>>,
    downloads: Option<u64>,
    likes: Option<u64>,
    siblings: Option<Vec<HfApiFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HfApiFile {
    rfilename: String,
    size: Option<u64>,
}

pub async fn search_models(
    client: &reqwest::Client,
    query: &str,
    owner: Option<&str>,
) -> Result<Vec<HfModel>> {
    let mut url = format!(
        "https://huggingface.co/api/models?search={}&filter=gguf&limit=30&sort=downloads",
        urlencoding_simple(query)
    );
    if let Some(owner) = owner {
        url = format!(
            "https://huggingface.co/api/models?author={}&search={}&filter=gguf&limit=50&sort=downloads",
            owner,
            urlencoding_simple(query)
        );
    }

    let response = client
        .get(&url)
        .header("User-Agent", "catapult-launcher/0.1")
        .send()
        .await
        .context("Failed to search HuggingFace")?;

    if !response.status().is_success() {
        anyhow::bail!("HuggingFace API error: {}", response.status());
    }

    let models: Vec<HfApiModel> = response.json().await.context("Failed to parse HF response")?;
    Ok(models.into_iter().map(convert_model).collect())
}

/// File entry from the HuggingFace tree API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HfTreeEntry {
    #[serde(rename = "type")]
    entry_type: String,
    oid: Option<String>,
    size: Option<u64>,
    path: String,
}

/// Check if a filename is an imatrix/importance matrix file (not a model).
pub fn is_imatrix_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.contains("imatrix") || lower.contains("importance_matrix")
}

/// Parse a split GGUF filename like `model-00001-of-00003.gguf`.
/// Returns (base_name, part_number, total_parts).
pub fn parse_split_filename(filename: &str) -> Option<(String, u32, u32)> {
    let name = filename.rsplit('/').next().unwrap_or(filename);
    let re = regex::Regex::new(r"^(.+)-(\d{5})-of-(\d{5})\.gguf$").ok()?;
    let caps = re.captures(name)?;
    let base = caps.get(1)?.as_str().to_string();
    let part: u32 = caps.get(2)?.as_str().parse().ok()?;
    let total: u32 = caps.get(3)?.as_str().parse().ok()?;
    Some((base, part, total))
}

/// Grouping key for split files: includes directory prefix so files in
/// different subdirs don't get merged.
fn split_group_key(path: &str) -> Option<String> {
    let dir_prefix = match path.rfind('/') {
        Some(pos) => &path[..=pos],
        None => "",
    };
    let (base, _, total) = parse_split_filename(path)?;
    Some(format!("{}{}-{:05}", dir_prefix, base, total))
}

/// Consolidate split GGUF files into single entries and strip subdirectory
/// prefixes from filenames (download URLs keep the full path).
fn consolidate_files(files: Vec<HfFile>) -> Vec<HfFile> {
    use std::collections::BTreeMap;

    let mut singles: Vec<HfFile> = Vec::new();
    let mut split_groups: BTreeMap<String, Vec<HfFile>> = BTreeMap::new();

    for file in files {
        if let Some(key) = split_group_key(&file.filename) {
            split_groups.entry(key).or_default().push(file);
        } else {
            // Strip directory prefix for flat download
            let basename = file.filename.rsplit('/').next().unwrap_or(&file.filename).to_string();
            singles.push(HfFile {
                filename: basename,
                is_split: false,
                split_parts: vec![],
                ..file
            });
        }
    }

    for (_key, mut parts) in split_groups {
        parts.sort_by_key(|f| {
            parse_split_filename(&f.filename).map(|(_, n, _)| n).unwrap_or(0)
        });

        let total_size: u64 = parts.iter().map(|p| p.size_bytes).sum();
        let quant = parts[0].quant.clone();

        let split_parts: Vec<HfFilePart> = parts
            .iter()
            .map(|p| {
                let basename = p.filename.rsplit('/').next().unwrap_or(&p.filename).to_string();
                HfFilePart {
                    filename: basename,
                    size_bytes: p.size_bytes,
                    download_url: p.download_url.clone(),
                }
            })
            .collect();

        let first = &parts[0];
        let first_basename = first.filename.rsplit('/').next().unwrap_or(&first.filename).to_string();

        singles.push(HfFile {
            filename: first_basename,
            size_bytes: total_size,
            quant,
            download_url: first.download_url.clone(),
            is_split: true,
            split_parts,
        });
    }

    singles
}

/// Recursively fetch the HuggingFace tree API, following directories up to max_depth.
fn fetch_tree_recursive<'a>(
    client: &'a reqwest::Client,
    repo_id: &'a str,
    path: &'a str,
    max_depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<HfTreeEntry>>> + Send + 'a>> {
    Box::pin(async move {
        if max_depth == 0 {
            return Ok(vec![]);
        }

        let url = if path.is_empty() {
            format!("https://huggingface.co/api/models/{}/tree/main", repo_id)
        } else {
            format!(
                "https://huggingface.co/api/models/{}/tree/main/{}",
                repo_id, path
            )
        };

        let response = client
            .get(&url)
            .header("User-Agent", "catapult-launcher/0.1")
            .send()
            .await
            .context("Failed to fetch repo tree")?;

        if !response.status().is_success() {
            // Don't fail on subdirectory errors — just skip
            return Ok(vec![]);
        }

        let entries: Vec<HfTreeEntry> = response.json().await.unwrap_or_default();
        let mut result = Vec::new();

        for entry in entries {
            if entry.entry_type == "directory" {
                let sub = fetch_tree_recursive(client, repo_id, &entry.path, max_depth - 1).await?;
                result.extend(sub);
            } else {
                result.push(entry);
            }
        }

        Ok(result)
    })
}

pub async fn get_repo_files(client: &reqwest::Client, repo_id: &str) -> Result<Vec<HfFile>> {
    // Recursively fetch tree (depth 3 covers subdirectory-organized repos)
    let entries = fetch_tree_recursive(client, repo_id, "", 3).await?;

    let files: Vec<HfFile> = entries
        .into_iter()
        .filter(|e| e.entry_type == "file" && e.path.ends_with(".gguf"))
        .filter(|e| !is_imatrix_file(&e.path))
        .map(|e| {
            let quant = extract_quant(&e.path);
            let download_url = format!(
                "https://huggingface.co/{}/resolve/main/{}",
                repo_id, e.path
            );
            HfFile {
                filename: e.path,
                size_bytes: e.size.unwrap_or(0),
                quant,
                download_url,
                is_split: false,
                split_parts: vec![],
            }
        })
        .collect();

    Ok(consolidate_files(files))
}

fn convert_model(m: HfApiModel) -> HfModel {
    let repo_id = m.id.clone();
    let author = m.author.unwrap_or_else(|| {
        repo_id.split('/').next().unwrap_or("unknown").to_string()
    });
    let name = repo_id.split('/').last().unwrap_or(&repo_id).to_string();

    let files: Vec<HfFile> = m
        .siblings
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.rfilename.ends_with(".gguf"))
        .filter(|f| !is_imatrix_file(&f.rfilename))
        .map(|f| {
            let quant = extract_quant(&f.rfilename);
            let download_url = format!(
                "https://huggingface.co/{}/resolve/main/{}",
                repo_id, f.rfilename
            );
            HfFile {
                filename: f.rfilename,
                size_bytes: f.size.unwrap_or(0),
                quant,
                download_url,
                is_split: false,
                split_parts: vec![],
            }
        })
        .collect();

    let files = consolidate_files(files);

    HfModel {
        repo_id,
        name,
        author,
        tags: m.tags.unwrap_or_default(),
        files,
        downloads: m.downloads.unwrap_or(0),
        likes: m.likes.unwrap_or(0),
    }
}

pub fn extract_quant(filename: &str) -> Option<String> {
    // Match patterns like Q4_K_M, Q8_0, F16, IQ2_XXS, MXFP4, etc.
    let re = regex::Regex::new(r"(?i)(MXFP\d|IQ\d[_A-Z]*|Q\d[_KM0-9A-Z]+|F16|F32|BF16)").ok()?;
    re.find(filename).map(|m| m.as_str().to_uppercase())
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => '+'.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

// ── presets.ini support ───────────────────────────────────────────────────────

/// Sampling parameters parsed from a HuggingFace repo's `presets.ini`.
/// Fields are `None` when not present in the file.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HfPresetParams {
    pub temperature: Option<f32>,
    pub top_k: Option<i32>,
    pub top_p: Option<f32>,
    pub min_p: Option<f32>,
    pub n_predict: Option<i32>,
    pub seed: Option<u64>,
    pub repeat_penalty: Option<f32>,
    pub repeat_last_n: Option<i32>,
}

impl HfPresetParams {
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_k.is_none()
            && self.top_p.is_none()
            && self.min_p.is_none()
            && self.n_predict.is_none()
            && self.seed.is_none()
            && self.repeat_penalty.is_none()
            && self.repeat_last_n.is_none()
    }
}

/// Fetch and parse `presets.ini` from a HuggingFace repo.
/// Returns `Ok(None)` if the file doesn't exist.
pub async fn fetch_presets_ini(
    client: &reqwest::Client,
    repo_id: &str,
) -> Result<Option<HfPresetParams>> {
    let url = format!(
        "https://huggingface.co/{}/resolve/main/presets.ini",
        repo_id
    );
    let resp = client.get(&url).send().await?;
    if resp.status() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Ok(None);
    }
    let text = resp.text().await?;
    Ok(Some(parse_presets_ini(&text)))
}

fn parse_presets_ini(content: &str) -> HfPresetParams {
    let mut params = HfPresetParams::default();
    for line in content.lines() {
        let line = line.trim();
        // Skip comments and section headers
        if line.starts_with('#') || line.starts_with(';') || line.starts_with('[') || line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim().to_lowercase();
            let val = val.trim();
            match key.as_str() {
                "temperature" | "temp" => {
                    params.temperature = val.parse().ok();
                }
                "top_k" | "top-k" => {
                    params.top_k = val.parse().ok();
                }
                "top_p" | "top-p" => {
                    params.top_p = val.parse().ok();
                }
                "min_p" | "min-p" => {
                    params.min_p = val.parse().ok();
                }
                "n_predict" | "max_new_tokens" | "max_tokens" | "max-new-tokens" => {
                    params.n_predict = val.parse().ok();
                }
                "seed" => {
                    params.seed = val.parse().ok();
                }
                "repeat_penalty" | "repeat-penalty" | "repetition_penalty" => {
                    params.repeat_penalty = val.parse().ok();
                }
                "repeat_last_n" | "repeat-last-n" => {
                    params.repeat_last_n = val.parse().ok();
                }
                _ => {}
            }
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_quant_standard_patterns() {
        assert_eq!(extract_quant("model-Q4_K_M.gguf"), Some("Q4_K_M".to_string()));
        assert_eq!(extract_quant("model-Q8_0.gguf"), Some("Q8_0".to_string()));
        assert_eq!(extract_quant("model-F16.gguf"), Some("F16".to_string()));
        assert_eq!(extract_quant("model-BF16.gguf"), Some("BF16".to_string()));
        assert_eq!(extract_quant("model-F32.gguf"), Some("F32".to_string()));
        assert_eq!(extract_quant("model-IQ2_XXS.gguf"), Some("IQ2_XXS".to_string()));
        assert_eq!(extract_quant("model-IQ4_XS.gguf"), Some("IQ4_XS".to_string()));
        assert_eq!(extract_quant("model-Q5_K_S.gguf"), Some("Q5_K_S".to_string()));
        assert_eq!(extract_quant("model-Q6_K.gguf"), Some("Q6_K".to_string()));
    }

    #[test]
    fn extract_quant_mxfp() {
        assert_eq!(extract_quant("gpt-oss-20b-MXFP4.gguf"), Some("MXFP4".into()));
        assert_eq!(extract_quant("model-mxfp4.gguf"), Some("MXFP4".into()));
    }

    #[test]
    fn extract_quant_no_match() {
        assert_eq!(extract_quant("model.gguf"), None);
        assert_eq!(extract_quant("README.md"), None);
        assert_eq!(extract_quant(""), None);
    }

    #[test]
    fn extract_quant_case_insensitive() {
        assert_eq!(extract_quant("model-q4_k_m.gguf"), Some("Q4_K_M".to_string()));
        assert_eq!(extract_quant("model-f16.gguf"), Some("F16".to_string()));
    }

    #[test]
    fn parse_split_filename_valid() {
        let (base, part, total) = parse_split_filename("model-Q4_K_M-00001-of-00003.gguf").unwrap();
        assert_eq!(base, "model-Q4_K_M");
        assert_eq!(part, 1);
        assert_eq!(total, 3);

        let (base, part, total) = parse_split_filename("model-00003-of-00005.gguf").unwrap();
        assert_eq!(base, "model");
        assert_eq!(part, 3);
        assert_eq!(total, 5);
    }

    #[test]
    fn parse_split_filename_with_subdir() {
        // Should parse basename only, ignoring directory prefix
        let (base, part, total) = parse_split_filename("Q4_K_M/model-Q4_K_M-00001-of-00003.gguf").unwrap();
        assert_eq!(base, "model-Q4_K_M");
        assert_eq!(part, 1);
        assert_eq!(total, 3);
    }

    #[test]
    fn parse_split_filename_not_split() {
        assert!(parse_split_filename("model-Q4_K_M.gguf").is_none());
        assert!(parse_split_filename("model.gguf").is_none());
        assert!(parse_split_filename("").is_none());
    }

    #[test]
    fn imatrix_detection() {
        assert!(is_imatrix_file("imatrix.dat"));
        assert!(is_imatrix_file("model-imatrix-Q4_K_M.gguf"));
        assert!(is_imatrix_file("importance_matrix.dat"));
        assert!(!is_imatrix_file("model-Q4_K_M.gguf"));
        assert!(!is_imatrix_file("model-00001-of-00003.gguf"));
    }

    #[test]
    fn consolidate_groups_split_files() {
        let files = vec![
            HfFile {
                filename: "Q4_K_M/model-Q4_K_M-00002-of-00003.gguf".into(),
                size_bytes: 200,
                quant: Some("Q4_K_M".into()),
                download_url: "https://example.com/Q4_K_M/model-Q4_K_M-00002-of-00003.gguf".into(),
                is_split: false,
                split_parts: vec![],
            },
            HfFile {
                filename: "Q4_K_M/model-Q4_K_M-00001-of-00003.gguf".into(),
                size_bytes: 200,
                quant: Some("Q4_K_M".into()),
                download_url: "https://example.com/Q4_K_M/model-Q4_K_M-00001-of-00003.gguf".into(),
                is_split: false,
                split_parts: vec![],
            },
            HfFile {
                filename: "Q4_K_M/model-Q4_K_M-00003-of-00003.gguf".into(),
                size_bytes: 200,
                quant: Some("Q4_K_M".into()),
                download_url: "https://example.com/Q4_K_M/model-Q4_K_M-00003-of-00003.gguf".into(),
                is_split: false,
                split_parts: vec![],
            },
            HfFile {
                filename: "single-Q8_0.gguf".into(),
                size_bytes: 500,
                quant: Some("Q8_0".into()),
                download_url: "https://example.com/single-Q8_0.gguf".into(),
                is_split: false,
                split_parts: vec![],
            },
        ];

        let result = consolidate_files(files);
        assert_eq!(result.len(), 2, "should consolidate 3 split parts + 1 single into 2 entries");

        let single = result.iter().find(|f| !f.is_split).unwrap();
        assert_eq!(single.filename, "single-Q8_0.gguf");
        assert_eq!(single.size_bytes, 500);

        let split = result.iter().find(|f| f.is_split).unwrap();
        assert_eq!(split.filename, "model-Q4_K_M-00001-of-00003.gguf");
        assert_eq!(split.size_bytes, 600); // 200 * 3
        assert_eq!(split.split_parts.len(), 3);
        // Parts should be sorted by number
        assert!(split.split_parts[0].filename.contains("00001"));
        assert!(split.split_parts[1].filename.contains("00002"));
        assert!(split.split_parts[2].filename.contains("00003"));
        // Subdirectory should be stripped from filenames
        assert!(!split.filename.contains('/'));
        assert!(!split.split_parts[0].filename.contains('/'));
    }

    // ── presets.ini parsing ──────────────────────────────────────────────────

    #[test]
    fn presets_ini_basic_fields() {
        let ini = "temperature = 0.7\ntop_k = 50\ntop_p = 0.9\nmin_p = 0.02\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.top_k, Some(50));
        assert_eq!(p.top_p, Some(0.9));
        assert_eq!(p.min_p, Some(0.02));
    }

    #[test]
    fn presets_ini_aliases() {
        // temp → temperature, top-k → top_k, max_new_tokens → n_predict
        let ini = "temp = 0.6\ntop-k = 30\ntop-p = 0.85\nmin-p = 0.01\nmax_new_tokens = 512\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.temperature, Some(0.6));
        assert_eq!(p.top_k, Some(30));
        assert_eq!(p.top_p, Some(0.85));
        assert_eq!(p.min_p, Some(0.01));
        assert_eq!(p.n_predict, Some(512));
    }

    #[test]
    fn presets_ini_repeat_and_seed() {
        let ini = "repeat_penalty = 1.1\nrepeat_last_n = 64\nseed = 42\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.repeat_penalty, Some(1.1));
        assert_eq!(p.repeat_last_n, Some(64));
        assert_eq!(p.seed, Some(42));
    }

    #[test]
    fn presets_ini_skips_comments_and_sections() {
        let ini = "# This is a comment\n[sampling]\ntemperature = 0.8\n; another comment\ntop_k = 40\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.temperature, Some(0.8));
        assert_eq!(p.top_k, Some(40));
        assert_eq!(p.top_p, None);
    }

    #[test]
    fn presets_ini_empty_is_empty() {
        let p = parse_presets_ini("");
        assert!(p.is_empty());
        let p2 = parse_presets_ini("# just a comment\n[section]\n");
        assert!(p2.is_empty());
    }

    #[test]
    fn presets_ini_not_empty_when_field_set() {
        let mut p = HfPresetParams::default();
        assert!(p.is_empty());
        p.temperature = Some(0.5);
        assert!(!p.is_empty());
    }

    #[test]
    fn presets_ini_unknown_keys_ignored() {
        let ini = "temperature = 0.7\nsome_unknown_key = 99\nchat_template = llama3\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.temperature, Some(0.7));
        // Everything else remains None
        assert_eq!(p.top_k, None);
    }

    #[test]
    fn presets_ini_repetition_penalty_alias() {
        let ini = "repetition_penalty = 1.15\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.repeat_penalty, Some(1.15));
    }

    #[test]
    fn presets_ini_max_tokens_alias() {
        let ini = "max_tokens = 256\n";
        let p = parse_presets_ini(ini);
        assert_eq!(p.n_predict, Some(256));
    }
}
