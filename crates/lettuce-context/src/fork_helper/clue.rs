//! The Notebook's clue check: is the clue the helper claimed actually in
//! the reply?

fn words(text: &str) -> Vec<String> {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ё' => 'е',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Same word up to its ending: a shared start of five letters, or of all
/// but one letter of the shorter word (at least three).
fn same_stem(a: &str, b: &str) -> bool {
    let shared = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let shorter = a.chars().count().min(b.chars().count());
    shared >= 5 || (shared >= 3 && shared + 1 >= shorter)
}

/// `true` when the clue's wording is in the reply: the whole phrase, or at
/// least 60% of its words of four letters or more (compared without their
/// endings, so Russian inflection does not matter).
#[must_use]
pub fn clue_found(clue: &str, reply: &str) -> bool {
    let clue_words = words(clue);
    let reply_words = words(reply);
    if clue_words.is_empty() || reply_words.is_empty() {
        return false;
    }
    let phrase = clue_words.join(" ");
    if phrase.chars().count() >= 3
        && format!(" {} ", reply_words.join(" ")).contains(&format!(" {phrase} "))
    {
        return true;
    }
    let significant = clue_words
        .iter()
        .filter(|word| word.chars().count() >= 4)
        .collect::<Vec<_>>();
    if significant.is_empty() {
        return false;
    }
    let hits = significant
        .iter()
        .filter(|word| reply_words.iter().any(|reply| same_stem(word, reply)))
        .count();
    hits * 10 >= significant.len() * 6
}
