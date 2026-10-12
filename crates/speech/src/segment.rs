//! Sentence segmenter for pipelined synthesis (PDA-T-0108, `PDA-SP-0007.R2`).
//!
//! Splits on `. ! ? ; :` and line breaks, but not inside abbreviations ("Sr.", "p. ej."), initials,
//! decimals ("3.5", "3,5"), versions, file names ("main.rs"), or ellipses followed by lowercase. The first
//! chunk is cut at a comma when it has more than 12 words so audio starts early.

/// Words in the first chunk above which a comma cut is attempted.
pub const FIRST_CHUNK_MAX_WORDS: usize = 12;

const ABBREVIATIONS: &[&str] = &[
    "sr", "sra", "srta", "srs", "sras", "dr", "dra", "drs", "prof", "profa", "lic", "ing", "arq",
    "av", "avda", "pág", "págs", "pag", "pags", "núm", "num", "tel", "etc", "p. ej", "ej", "cap",
    "vol", "fig", "mr", "mrs", "ms", "st", "vs", "approx", "cf", "eg", "ie", "jr", "sr", "a.m",
    "p.m", "ud", "uds", "vd", "vds", "máx", "mín", "aprox", "dpto", "esq", "admón", "gral", "cía",
    "sta", "sto", "ee", "uu",
];

fn is_abbreviation(word: &str) -> bool {
    let w = word
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '.')
        .to_lowercase();
    let w = w.trim_end_matches('.');
    ABBREVIATIONS.contains(&w)
}

/// Split into sentences. Each returned piece is trimmed, non-empty, and keeps its closing punctuation.
pub fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' | '\r' => {
                push(&mut out, &mut cur);
                i += 1;
            }
            '.' | '!' | '?' | ';' | ':' | '…' => {
                // absorb runs of terminators and closing quotes/brackets
                let mut j = i;
                while j + 1 < chars.len() && matches!(chars[j + 1], '.' | '!' | '?' | '…') {
                    j += 1;
                }
                let run_len = j - i + 1;
                cur.extend(&chars[i..=j]);
                let mut k = j + 1;
                while k < chars.len()
                    && matches!(chars[k], '"' | '\'' | ')' | ']' | '»' | '”' | '’')
                {
                    cur.push(chars[k]);
                    k += 1;
                }
                let next = chars.get(k).copied();
                let prev = if i > 0 { Some(chars[i - 1]) } else { None };
                let boundary = match c {
                    '.' => dot_is_boundary(&chars, i, run_len, &cur, next),
                    ':' => {
                        !(prev.is_some_and(|p| p.is_ascii_digit())
                            && next.is_some_and(|n| n.is_ascii_digit()))
                            && next.is_none_or(char::is_whitespace)
                    }
                    _ => next.is_none_or(|n| n.is_whitespace()) || matches!(c, '!' | '?'),
                };
                if boundary {
                    push(&mut out, &mut cur);
                }
                i = k;
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    push(&mut out, &mut cur);
    out
}

fn push(out: &mut Vec<String>, cur: &mut String) {
    let t = cur.trim();
    // pieces with no letter or digit (only punctuation) are noise
    if t.chars().any(char::is_alphanumeric) {
        out.push(t.to_string());
    }
    cur.clear();
}

fn dot_is_boundary(
    chars: &[char],
    i: usize,
    run_len: usize,
    cur: &str,
    next: Option<char>,
) -> bool {
    // "3.5", "1.13.8", "main.rs": a dot glued to the next non-space character
    if run_len == 1 && next.is_some_and(|n| !n.is_whitespace()) {
        return false;
    }
    // ellipsis followed by lowercase continues the sentence
    if run_len >= 3 {
        let after = chars[i + run_len..].iter().find(|c| !c.is_whitespace());
        return after.is_none_or(|c| !c.is_lowercase());
    }
    // abbreviations and initials
    let word_start = cur
        .trim_end_matches('.')
        .rfind(char::is_whitespace)
        .map_or(0, |p| p + 1);
    let word = &cur[word_start..];
    if run_len == 1 {
        let bare = word.trim_end_matches('.');
        if is_abbreviation(word) {
            // "etc." / "vs." may end a sentence when a capitalised word follows; titles never do
            let next_word_capital = chars[i + 1..]
                .iter()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| c.is_uppercase());
            if !(is_sentence_final_abbr(bare) && next_word_capital) {
                return false;
            }
        }
        // single capital letter: an initial ("J. Pérez")
        if bare.chars().count() == 1 && bare.chars().all(char::is_uppercase) {
            return false;
        }
        // "p. ej.", "a. m.": a lone lowercase letter followed by a lowercase word
        if bare.chars().count() == 1
            && bare.chars().all(char::is_lowercase)
            && chars[i + 1..]
                .iter()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| c.is_lowercase())
        {
            return false;
        }
        // list numbers "1. foo" at the start of the piece
        if bare.chars().all(|c| c.is_ascii_digit()) && cur.trim() == word.trim() {
            return false;
        }
    }
    true
}

/// Abbreviations that usually end a sentence when the next word is capitalised.
fn is_sentence_final_abbr(w: &str) -> bool {
    matches!(w.to_lowercase().as_str(), "etc" | "vs")
}

/// Cut `sentence` at a comma when it has more than [`FIRST_CHUNK_MAX_WORDS`] words, so the first audio
/// starts early. Returns `(head, tail)`; the tail is `None` when no cut applies.
pub fn first_chunk_split(sentence: &str) -> (String, Option<String>) {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if words.len() <= FIRST_CHUNK_MAX_WORDS {
        return (sentence.trim().to_string(), None);
    }
    // the last comma that leaves 4..=12 words in the head and at least 3 words in the tail
    let mut best: Option<usize> = None;
    for (idx, w) in words.iter().enumerate() {
        let head_len = idx + 1;
        if head_len > FIRST_CHUNK_MAX_WORDS {
            break;
        }
        if head_len >= 4 && (w.ends_with(',') || w.ends_with(';')) && words.len() - head_len >= 3 {
            best = Some(idx);
        }
    }
    match best {
        Some(idx) => (words[..=idx].join(" "), Some(words[idx + 1..].join(" "))),
        None => (sentence.trim().to_string(), None),
    }
}

/// Sentences of `text` ready for synthesis, with the first chunk cut at a comma when it is long.
pub fn segments(text: &str) -> Vec<String> {
    let mut sentences = split_sentences(text);
    if sentences.is_empty() {
        return sentences;
    }
    let first = sentences.remove(0);
    let (head, tail) = first_chunk_split(&first);
    let mut out = vec![head];
    out.extend(tail);
    out.extend(sentences);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Vec<String> {
        split_sentences(t)
    }

    #[test]
    fn splits_basic_terminators() {
        let cases: &[(&str, &[&str])] = &[
            ("Hola. Adiós.", &["Hola.", "Adiós."]),
            ("¿Qué tal? Muy bien.", &["¿Qué tal?", "Muy bien."]),
            ("¡Genial! Sigamos.", &["¡Genial!", "Sigamos."]),
            ("Uno; dos; tres", &["Uno;", "dos;", "tres"]),
            ("Nota: revisa esto", &["Nota:", "revisa esto"]),
            ("Una sola frase sin punto", &["Una sola frase sin punto"]),
            ("Línea uno\nLínea dos", &["Línea uno", "Línea dos"]),
            ("A.\n\nB.", &["A.", "B."]),
            ("", &[]),
            ("   ", &[]),
            ("...", &[]),
            ("¡¡Hola!! ¿¿Qué??", &["¡¡Hola!!", "¿¿Qué??"]),
            ("Hola…  Adiós", &["Hola…", "Adiós"]),
            ("Fin.", &["Fin."]),
            (
                "Dijo \"hola.\" Luego se fue.",
                &["Dijo \"hola.\"", "Luego se fue."],
            ),
            ("(Ver arriba.) Sigue.", &["(Ver arriba.)", "Sigue."]),
        ];
        for (input, want) in cases {
            let got = s(input);
            let want: Vec<String> = want.iter().map(|x| x.to_string()).collect();
            assert_eq!(got, want, "input {input:?}");
        }
    }

    #[test]
    fn keeps_numbers_together() {
        let cases: &[(&str, usize)] = &[
            ("Vale 3.5 de media. Fin.", 2),
            ("Pi es 3,14 más o menos. Ok.", 2),
            ("Versión 1.13.8 lista. Siguiente.", 2),
            ("Abre main.rs ahora. Luego sigue.", 2),
            ("Cuesta 1.000.000 de euros. Caro.", 2),
            ("A las 14:30 hay reunión. Ven.", 2),
            ("Hay 2.5 GB libres.", 1),
            ("v2.0.1 salió ayer", 1),
        ];
        for (input, n) in cases {
            assert_eq!(s(input).len(), *n, "input {input:?}: {:?}", s(input));
        }
        assert_eq!(s("Vale 3.5 de media. Fin.")[0], "Vale 3.5 de media.");
    }

    #[test]
    fn keeps_abbreviations_together() {
        let cases: &[(&str, usize)] = &[
            ("El Sr. García llegó. Se sentó.", 2),
            ("La Dra. López habló. Todos callaron.", 2),
            ("Hay varios, p. ej. gatos y perros. Fin.", 2),
            ("Llegó J. Pérez ayer. Bien.", 2),
            ("Mr. Smith arrived. He sat.", 2),
            ("Compra pan, leche, etc. Luego vuelve.", 2),
            ("Ver pág. 12 del manual. Gracias.", 2),
            ("Tel. 555 1234 para más info. Llama.", 2),
            ("Prof. Gómez dijo que sí.", 1),
            ("Salió a las 5 p.m. y volvió tarde.", 1),
        ];
        for (input, n) in cases {
            assert_eq!(s(input).len(), *n, "input {input:?}: {:?}", s(input));
        }
    }

    #[test]
    fn more_cases() {
        let cases: &[(&str, usize)] = &[
            ("¿Dónde está? No lo sé. Pregunta luego.", 3),
            ("Primero esto; después aquello; al final lo otro.", 3),
            ("Hola.Adiós.", 1),
            ("Termina aquí!", 1),
            (
                "Dos líneas


separadas por blancos",
                2,
            ),
            ("Cuesta 3,50 euros. Barato.", 2),
            ("Usa a.b.c para eso. Vale.", 2),
            ("Dijo: \"vale\". Y se fue.", 3),
            ("Ver 1. Introducción", 2),
            ("¡Atención! ¡Peligro! ¡Corre!", 3),
        ];
        for (input, n) in cases {
            assert_eq!(s(input).len(), *n, "input {input:?}: {:?}", s(input));
        }
    }

    #[test]
    fn ellipsis_rules() {
        assert_eq!(s("Bueno... no sé"), vec!["Bueno... no sé"]);
        assert_eq!(s("Bueno... Pues vale"), vec!["Bueno...", "Pues vale"]);
    }

    #[test]
    fn spoken_normalised_text_round_trips() {
        let t = "He cambiado handlers chat punto go, tres ficheros. Bloque de código omitido.";
        assert_eq!(
            s(t),
            vec![
                "He cambiado handlers chat punto go, tres ficheros.",
                "Bloque de código omitido."
            ]
        );
    }

    #[test]
    fn first_chunk_is_cut_at_a_comma_when_long() {
        let long = "He revisado el proyecto completo con mucho cuidado, y la migración de autenticación terminó correctamente sin errores.";
        let (head, tail) = first_chunk_split(long);
        assert_eq!(head, "He revisado el proyecto completo con mucho cuidado,");
        assert!(
            tail.as_deref()
                .is_some_and(|t| t.starts_with("y la migración"))
        );
        assert_eq!(format!("{head} {}", tail.unwrap_or_default()), long);
    }

    #[test]
    fn first_chunk_not_cut_when_short_or_no_comma() {
        let short = "Una frase corta, con coma.";
        assert_eq!(first_chunk_split(short), (short.to_string(), None));
        let nocomma =
            "Esta frase es bastante larga pero no contiene ninguna coma que permita cortarla antes";
        assert_eq!(first_chunk_split(nocomma).1, None);
        // comma too early (fewer than 4 words in the head)
        let early = "Sí, esto es una frase muy larga que sigue y sigue sin ninguna otra coma hasta el final";
        assert_eq!(first_chunk_split(early).1, None);
    }

    #[test]
    fn only_the_first_sentence_is_cut() {
        let t = "Primero reviso todo el código fuente del módulo, después ejecuto todas las pruebas pendientes. Segundo paso, que también es largo y tiene muchas palabras en total aquí.";
        let seg = segments(t);
        assert_eq!(seg.len(), 3);
        assert!(seg[0].ends_with("módulo,"));
        assert!(seg[2].starts_with("Segundo paso,"));
    }

    #[test]
    fn segments_of_empty_text() {
        assert!(segments("").is_empty());
        assert_eq!(segments("Hola"), vec!["Hola"]);
    }

    #[test]
    fn pieces_never_empty_and_cover_the_letters() {
        let t = "Uno. Dos! ¿Tres? Cuatro; cinco: seis\nsiete... Ocho";
        let letters: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
        let got: String = s(t)
            .join("")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        assert_eq!(got, letters);
        assert!(s(t).iter().all(|p| !p.trim().is_empty()));
    }
}
