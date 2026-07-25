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

pub const CODE_WORDS: usize = 6;

pub fn generate_room_code() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let mut words = Vec::with_capacity(CODE_WORDS);
    for _ in 0..CODE_WORDS {
        let idx = rng.random_range(0..WORDS.len());
        words.push(WORDS[idx]);
    }
    words.join(" ")
}

pub fn words_to_topic_id(phrase: &str) -> TopicId {
    let normalized: String = phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let hash = Sha256::digest(normalized.as_bytes());
    let bytes: [u8; 32] = hash.into();
    TopicId::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_room_code_length() {
        let code = generate_room_code();
        let word_count = code.split_whitespace().count();
        assert_eq!(word_count, CODE_WORDS);
    }

    #[test]
    fn test_generate_room_code_uses_dictionary_words() {
        let code = generate_room_code();
        for word in code.split_whitespace() {
            assert!(WORDS.contains(&word), "word {word:?} not in dictionary");
        }
    }

    #[test]
    fn test_words_to_topic_id_is_deterministic() {
        let phrase = "alpha bravo charlie delta echo foxtrot";
        let id1 = words_to_topic_id(phrase);
        let id2 = words_to_topic_id(phrase);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_words_to_topic_id_different_phrases_differ() {
        let id1 = words_to_topic_id("alpha bravo charlie");
        let id2 = words_to_topic_id("alpha bravo delta");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_words_to_topic_id_normalizes_whitespace() {
        let id1 = words_to_topic_id("alpha  bravo\ncharlie");
        let id2 = words_to_topic_id("alpha bravo charlie");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_words_to_topic_id_normalizes_case() {
        let id1 = words_to_topic_id("Alpha Bravo Charlie");
        let id2 = words_to_topic_id("alpha bravo charlie");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generated_code_round_trips() {
        let code = generate_room_code();
        let topic_id = words_to_topic_id(&code);
        let _bytes: &[u8; 32] = topic_id.as_bytes();
        assert!(!code.is_empty());
    }
}
