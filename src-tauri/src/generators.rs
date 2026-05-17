use crate::models::{GeneratedPassword, PasswordGeneratorConfig, UsernameConfig};
use rand::Rng;

const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const NUMBERS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{}|;:',.<>?/";
const AMBIGUOUS: &str = "0OlI1";

const ADJECTIVES: &[&str] = &[
    "brave", "calm", "eager", "fancy", "gentle", "happy", "jolly", "kind", "lively", "merry",
    "nice", "proud", "quick", "silly", "witty", "zealous", "bold", "cool", "dark", "fair", "grand",
    "keen", "neat", "pure", "rare", "safe", "tall", "vast", "warm", "wise", "bright", "clever",
    "fierce", "golden", "humble", "iron", "jade", "lunar", "noble", "swift",
];

const NOUNS: &[&str] = &[
    "bear", "cloud", "dawn", "eagle", "flame", "grove", "hawk", "isle", "jewel", "knight", "lake",
    "moon", "nest", "oak", "peak", "reef", "star", "tide", "vale", "wave", "wolf", "arch", "bell",
    "cave", "dune", "fern", "glen", "helm", "ivy", "jade", "lark", "mist", "nova", "opal", "pine",
    "rose", "sage", "thorn", "urn", "vine",
];

pub fn generate_password(config: &PasswordGeneratorConfig) -> GeneratedPassword {
    let mut charset = String::new();
    if config.uppercase {
        charset.push_str(UPPERCASE);
    }
    if config.lowercase {
        charset.push_str(LOWERCASE);
    }
    if config.numbers {
        charset.push_str(NUMBERS);
    }
    if config.symbols {
        charset.push_str(SYMBOLS);
    }

    if charset.is_empty() {
        charset.push_str(LOWERCASE);
    }

    let chars: Vec<char> = if config.exclude_ambiguous {
        charset
            .chars()
            .filter(|c| !AMBIGUOUS.contains(*c))
            .collect()
    } else {
        charset.chars().collect()
    };

    let mut rng = rand::thread_rng();
    let password: String = (0..config.length)
        .map(|_| chars[rng.gen_range(0..chars.len())])
        .collect();

    let strength = evaluate_strength(&password);

    GeneratedPassword { password, strength }
}

fn evaluate_strength(password: &str) -> String {
    let mut score = 0u32;
    let len = password.chars().count() as u32;

    if len >= 8 {
        score += 1;
    }
    if len >= 12 {
        score += 1;
    }
    if len >= 16 {
        score += 1;
    }

    if password.chars().any(|c| c.is_ascii_lowercase()) {
        score += 1;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        score += 1;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        score += 1;
    }
    if password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        score += 1;
    }

    if score <= 3 {
        "weak".to_string()
    } else if score <= 5 {
        "medium".to_string()
    } else {
        "strong".to_string()
    }
}

pub fn generate_username(config: &UsernameConfig) -> String {
    let mut rng = rand::thread_rng();

    let adj = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    let num: u16 = rng.gen_range(100..9999);

    match config.format.as_str() {
        "word1234" => format!("{}{}{}", adj, noun, num),
        "word_word" => format!("{}_{}", adj, noun),
        "word.word" => format!("{}.{}", adj, noun),
        _ => format!("{}{}{}", adj, noun, num),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_strength_should_score_by_actual_characters() {
        assert_eq!(evaluate_strength("abcdefgh"), "weak");
        assert_eq!(evaluate_strength("Abc123!"), "medium");
        assert_eq!(evaluate_strength("AbcdefghijK1"), "medium");
        assert_eq!(evaluate_strength("Abcdef123!@#XYZ1"), "strong");
    }
}
