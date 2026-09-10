//! Human-readable room codes and the topic ids derived from them.
//!
//! A room code is a memorisable phrase (`CODE_WORDS` dictionary words) that a
//! Host reads aloud. The gossip topic is not the phrase itself but its SHA-256
//! digest, so the code can be shared independently of the wire id.

use iroh_gossip::TopicId;
use sha2::{Digest, Sha256};

const WORDS: &[&str] = &[
    "alpha", "apple", "audio", "badge", "basin", "beach", "berry", "block",
    "board", "bonus", "brave", "bread", "break", "brown", "brush", "build",
    "bunch", "burst", "cabin", "cable", "candy", "carry", "catch", "cause",
    "chain", "chair", "chaos", "charm", "chart", "cheap", "check", "chest",
    "chief", "child", "china", "chunk", "civic", "civil", "claim", "clash",
    "clean", "clear", "click", "cliff", "climb", "clock", "close", "cloth",
    "coach", "coast", "coral", "couch", "count", "court", "cover", "crack",
    "craft", "crane", "crash", "cream", "creek", "crime", "crisp", "cross",
    "crowd", "crown", "cruel", "crush", "cycle", "dance", "death", "delay",
    "delta", "dense", "depth", "diary", "dirty", "donor", "doubt", "dough",
    "draft", "drain", "drone", "drove", "eagle", "earth", "eight", "elect",
    "elite", "empty", "enjoy", "entry", "equal", "error", "ether", "event",
    "every", "exact", "exile", "exist", "extra", "faint", "faith", "false",
    "fancy", "fatal", "fault", "fence", "ferry", "fiber", "final", "first",
    "fixed", "flame", "flash", "fleet", "flesh", "float", "flood", "floor",
    "flour", "fluid", "flush", "focal", "focus", "force", "forge", "forty",
    "forum", "found", "frame", "frank", "fraud", "fresh", "front", "frost",
    "fruit", "genre", "ghost", "giant", "glass", "globe", "glove", "grace",
    "grade", "grain", "grand", "grant", "grape", "graph", "grass", "grave",
    "great", "green", "greet", "grief", "grill", "grind", "gross", "group",
    "grove", "guard", "guess", "guest", "guide", "guilt", "happy", "harsh",
    "haste", "heart", "heavy", "hedge", "horse", "hotel", "house", "hover",
    "human", "humor", "ideal", "image", "imply", "index", "inner", "input",
    "issue", "ivory", "jewel", "joint", "judge", "juice", "knock", "label",
    "labor", "large", "laser", "laugh", "layer", "learn", "lease", "leave",
    "legal", "lemon", "level", "light", "limit", "linen", "liver", "local",
    "login", "logic", "loose", "lover", "lower", "loyal", "lucky", "lunch",
    "lyric", "magic", "major", "maker", "maple", "march", "match", "mayor",
    "media", "mercy", "merge", "merit", "metal", "meter", "might", "minor",
    "minus", "mixed", "model", "money", "moral", "motor", "mount", "mouse",
    "mouth", "movie", "music", "nerve", "never", "night", "noise", "north",
    "novel", "nurse", "ocean", "offer", "olive", "onset", "opera", "orbit",
    "order", "organ", "other", "outer", "paint", "panel", "panic", "paper",
    "party", "pasta", "patch", "pause", "peace", "pearl", "penny", "phase",
    "phone", "photo", "piece", "pilot", "pitch", "pixel", "pizza", "place",
    "plain", "plane", "plant", "plate", "plaza", "point", "polar", "porch",
    "pouch", "pound", "power", "press", "price", "prime", "print", "prior",
    "prism", "prize", "proof", "prose", "proud", "prove", "pulse", "punch",
    "purge", "purse", "quest", "queue", "quick", "quiet", "quota", "quote",
    "radar", "radio", "raise", "rally", "ranch", "range", "rapid", "ratio",
    "razor", "reach", "react", "ready", "realm", "rebel", "refer", "reign",
    "relax", "relay", "reply", "rider", "ridge", "rifle", "right", "rigid",
    "rival", "river", "robin", "robot", "rocky", "rogue", "rough", "round",
    "route", "royal", "ruler", "rural", "salad", "salsa", "sauce", "scale",
    "scene", "scope", "score", "scout", "screw", "sense", "serve", "setup",
    "seven", "shade", "shaft", "shake", "shame", "shape", "share", "shark",
    "sharp", "sheer", "sheet", "shelf", "shell", "shift", "shine", "shirt",
    "shock", "shore", "short", "shout", "shrub", "sight", "sigma", "since",
    "sixth", "sixty", "skill", "skull", "slack", "sleep", "slice", "slide",
    "slope", "smart", "smell", "smile", "smoke", "snake", "solid", "solve",
    "sound", "south", "space", "spare", "spark", "speak", "speed", "spell",
    "spend", "spice", "spike", "spill", "spine", "split", "spoke", "spoon",
    "sport", "spray", "squad", "stack", "staff", "stage", "stake", "stall",
    "stamp", "stand", "stare", "start", "state", "steak", "steal", "steam",
    "steel", "steep", "steer", "stern", "stick", "stiff", "still", "stock",
    "stone", "storm", "story", "stout", "stove", "strap", "straw", "strip",
    "stuck", "stuff", "stump", "style", "sugar", "suite", "sunny", "super",
    "surge", "swamp", "swear", "sweep", "sweet", "swift", "swing", "swirl",
    "sword", "table", "taste", "tiger", "tight", "timer", "title", "toast",
    "token", "total", "touch", "tough", "towel", "tower", "toxic", "trace",
    "track", "trade", "trail", "train", "trait", "trash", "treat", "trend",
    "trial", "tribe", "trick", "trout", "truck", "truly", "trunk", "trust",
    "truth", "twist", "ultra", "uncle", "under", "union", "unite", "unity",
    "until", "upper", "upset", "urban", "usage", "valid", "value", "vapor",
    "vault", "venue", "verse", "video", "vigor", "vinyl", "voice", "voter",
    "waist", "waste", "watch", "water", "weary", "weave", "wedge", "weigh",
    "weird", "whale", "wheat", "wheel", "which", "while", "white", "whole",
    "witch", "woman", "world", "worry", "worse", "worst", "worth", "wound",
    "wreck", "wrist", "write", "wrong", "yacht", "yield", "young", "youth",
    "zebra", "zone",
];

/// How many dictionary words make up a room code.
pub const CODE_WORDS: usize = 6;

/// Generate a fresh, human-readable room code.
pub fn generate_room_code() -> String {
    use rand::RngExt;

    let mut rng = rand::rng();
    (0..CODE_WORDS)
        .map(|_| WORDS[rng.random_range(0..WORDS.len())])
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fold a room code (or any phrase) into the gossip topic it names.
pub fn words_to_topic_id(phrase: &str) -> TopicId {
    let digest: [u8; 32] = Sha256::digest(normalize(phrase).as_bytes()).into();
    TopicId::from_bytes(digest)
}

/// Collapse whitespace and case so `"Alpha  Bravo"` and `"alpha bravo"` are
/// the same code.
fn normalize(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "alpha bravo charlie";

    #[test]
    fn room_code_is_six_dictionary_words() {
        let code = generate_room_code();
        let words: Vec<_> = code.split_whitespace().collect();
        assert_eq!(words.len(), CODE_WORDS);
        for word in words {
            assert!(WORDS.contains(&word), "not a dictionary word: {word:?}");
        }
    }

    #[test]
    fn topic_is_sha256_of_the_normalized_phrase() {
        let expected: [u8; 32] = [
            0xbd, 0xe3, 0xc5, 0x0e, 0xca, 0x48, 0xa7, 0xcc, 0xe7, 0xc0, 0x9f, 0x3e, 0xf9, 0x68,
            0x4f, 0xba, 0x2b, 0x57, 0x3a, 0x46, 0xb8, 0x3d, 0x5c, 0xfe, 0xfe, 0x44, 0x9f, 0x68,
            0x34, 0x1c, 0xd5, 0x95,
        ];
        assert_eq!(words_to_topic_id(SAMPLE).as_bytes(), &expected);
    }

    #[test]
    fn topic_is_deterministic() {
        assert_eq!(words_to_topic_id(SAMPLE), words_to_topic_id(SAMPLE));
    }

    #[test]
    fn topic_normalizes_case_and_whitespace() {
        assert_eq!(
            words_to_topic_id("  Alpha\tBravo\nCharlie  "),
            words_to_topic_id(SAMPLE)
        );
    }

    #[test]
    fn different_phrases_get_different_topics() {
        assert_ne!(
            words_to_topic_id("alpha bravo charlie"),
            words_to_topic_id("alpha bravo delta")
        );
    }

    #[test]
    fn generated_code_maps_to_a_stable_topic() {
        let code = generate_room_code();
        assert_eq!(
            words_to_topic_id(&code),
            words_to_topic_id(&code.to_uppercase())
        );
    }
}
