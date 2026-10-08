use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};
use unicode_normalization::UnicodeNormalization;

#[derive(Deserialize)]
struct Document {
    model: Model,
}
#[derive(Deserialize)]
struct Model {
    vocab: HashMap<String, i32>,
    merges: Vec<Merge>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Merge {
    Pair(Vec<String>),
    Text(String),
}

pub(crate) struct ClipTokenizer {
    vocab: HashMap<String, i32>,
    ranks: HashMap<(String, String), usize>,
}
impl ClipTokenizer {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: Document = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut ranks = HashMap::new();
        for (rank, merge) in document.model.merges.into_iter().enumerate() {
            let pair = match merge {
                Merge::Pair(pair) => pair,
                Merge::Text(text) => text.splitn(2, ' ').map(str::to_owned).collect(),
            };
            if pair.len() == 2 {
                ranks.insert((pair[0].clone(), pair[1].clone()), rank);
            }
        }
        Ok(Self {
            vocab: document.model.vocab,
            ranks,
        })
    }
    pub fn encode(&self, text: &str) -> Vec<i32> {
        static WORDS: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"<\|startoftext\|>|<\|endoftext\|>|'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+").unwrap()
        });
        let normalized = text
            .nfc()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let mut ids = vec![49406];
        'words: for word in WORDS.find_iter(&normalized) {
            for piece in self.bpe(word.as_str()) {
                if let Some(id) = self.vocab.get(&piece) {
                    ids.push(*id);
                }
                if ids.len() >= 76 {
                    break 'words;
                }
            }
        }
        ids.push(49407);
        ids.resize(77, 0);
        ids
    }
    fn bpe(&self, word: &str) -> Vec<String> {
        static BYTE_CHARS: LazyLock<[char; 256]> = LazyLock::new(|| {
            let mut chars = ['\0'; 256];
            let mut missing = 0;
            for byte in 0..=255u32 {
                let printable = (33..=126).contains(&byte)
                    || (161..=172).contains(&byte)
                    || (174..=255).contains(&byte);
                chars[byte as usize] = char::from_u32(if printable {
                    byte
                } else {
                    missing += 1;
                    255 + missing
                })
                .unwrap();
            }
            chars
        });
        let mut pieces = word
            .bytes()
            .map(|byte| BYTE_CHARS[byte as usize].to_string())
            .collect::<Vec<_>>();
        if let Some(last) = pieces.last_mut() {
            last.push_str("</w>");
        }
        while pieces.len() > 1 {
            let best = pieces
                .windows(2)
                .enumerate()
                .filter_map(|(index, pair)| {
                    self.ranks
                        .get(&(pair[0].clone(), pair[1].clone()))
                        .map(|rank| (index, *rank))
                })
                .min_by_key(|(_, rank)| *rank);
            let Some((index, _)) = best else {
                break;
            };
            let right = pieces.remove(index + 1);
            pieces[index].push_str(&right);
        }
        pieces
    }
}
