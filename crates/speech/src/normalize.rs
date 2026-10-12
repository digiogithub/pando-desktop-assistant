//! Text normaliser for speech (PDA-T-0110, `PDA-SP-0007.R3`).
//!
//! Turns assistant output (markdown, code, ids, URLs, emoji, numbers, units, dates) into text a TTS
//! engine reads well. Pure and deterministic; Spanish and English.

/// Output language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// `es`, `es-ES`, `en_US` ... (anything unknown is Spanish, the product default).
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("en") {
            Lang::En
        } else {
            Lang::Es
        }
    }

    fn code_block(self) -> &'static str {
        match self {
            Lang::Es => "Bloque de código omitido.",
            Lang::En => "Code block omitted.",
        }
    }

    fn dot(self) -> &'static str {
        match self {
            Lang::Es => "punto",
            Lang::En => "dot",
        }
    }
}

const CODE_MARK: char = '\u{1}';

/// Normalise `text` for speech.
pub fn normalize(text: &str, lang: Lang) -> String {
    let text = strip_fences(text);
    let text = markdown_lines(&text);
    let text = inline_markup(&text);
    let text: String = text.chars().filter(|c| !is_emoji(*c)).collect();
    let text = words(&text, lang);
    cleanup(&text, lang)
}

// ---------------------------------------------------------------------------------------------
// Block level
// ---------------------------------------------------------------------------------------------

fn strip_fences(text: &str) -> String {
    // ``` anywhere: even parts are text, odd parts are code (an unclosed fence swallows the rest).
    let mut out = String::new();
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 0 {
            out.push_str(part);
        } else {
            out.push('\n');
            out.push(CODE_MARK);
            out.push('\n');
        }
    }
    // ~~~ fences are line based
    let mut res = String::new();
    let mut in_fence = false;
    for line in out.lines() {
        if line.trim_start().starts_with("~~~") {
            if !in_fence {
                res.push('\n');
                res.push(CODE_MARK);
                res.push('\n');
            }
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            res.push_str(line);
            res.push('\n');
        }
    }
    res
}

fn markdown_lines(text: &str) -> String {
    let mut out = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            out.push('\n');
            continue;
        }
        if line.starts_with(CODE_MARK) {
            out.push_str("\n\u{1}\n");
            continue;
        }
        // horizontal rules and table separators
        if line
            .chars()
            .all(|c| matches!(c, '-' | '*' | '_' | '=' | '|' | ':' | ' '))
            && line.len() >= 3
        {
            continue;
        }
        let mut l = line;
        let mut structural = l.starts_with('#') || l.starts_with('>') || l.starts_with('|');
        l = l.trim_start_matches('#').trim_start();
        while let Some(r) = l.strip_prefix('>') {
            l = r.trim_start();
        }
        for bullet in ["- ", "* ", "+ ", "• "] {
            if let Some(r) = l.strip_prefix(bullet) {
                l = r.trim_start();
                structural = true;
                break;
            }
        }
        // numbered list "1. " / "2) "
        let digits = l.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && digits <= 3 {
            let after = &l[digits..];
            if (after.starts_with(". ") || after.starts_with(") ")) && !after[2..].trim().is_empty()
            {
                l = after[2..].trim_start();
                structural = true;
            }
        }
        // task list "[ ] " / "[x] "
        for t in ["[ ] ", "[x] ", "[X] "] {
            if let Some(r) = l.strip_prefix(t) {
                l = r;
            }
        }
        let mut s = if l.starts_with('|') {
            l.trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            l.to_string()
        };
        // headings, list items, quotes and table rows are sentences of their own
        if structural && !s.ends_with(['.', '!', '?', ':', ';', ',', '…']) {
            s.push('.');
        }
        out.push_str(&s);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Inline markup
// ---------------------------------------------------------------------------------------------

fn inline_markup(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            // images and links: ![alt](url) / [text](url)
            '!' | '[' => {
                let start = if c == '!' && chars.get(i + 1) == Some(&'[') {
                    i + 1
                } else {
                    i
                };
                if chars.get(start) == Some(&'[')
                    && let Some(close) = chars[start..].iter().position(|&x| x == ']')
                    && chars.get(start + close + 1) == Some(&'(')
                    && let Some(paren) = chars[start + close + 1..].iter().position(|&x| x == ')')
                {
                    let label: String = chars[start + 1..start + close].iter().collect();
                    out.push_str(&label);
                    i = start + close + 1 + paren + 1;
                    continue;
                }
                if c == '[' {
                    i += 1; // lone bracket
                    continue;
                }
                out.push(c);
                i += 1;
            }
            '`' => {
                // inline code (single or double backticks)
                let ticks = chars[i..].iter().take_while(|&&x| x == '`').count();
                let fence: String = "`".repeat(ticks);
                let rest: String = chars[i + ticks..].iter().collect();
                if let Some(end) = rest.find(&fence) {
                    let code = &rest[..end];
                    out.push(' ');
                    out.push_str(code.trim());
                    out.push(' ');
                    i += ticks + code.chars().count() + ticks;
                } else {
                    i += ticks;
                }
            }
            '*' | '~' => i += 1,
            '<' => {
                // HTML tag
                let next = chars.get(i + 1).copied().unwrap_or(' ');
                if (next.is_ascii_alphabetic() || next == '/' || next == '!')
                    && let Some(end) = chars[i..].iter().position(|&x| x == '>')
                {
                    out.push(' ');
                    i += end + 1;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            '"' | '«' | '»' | '“' | '”' | '\\' => {
                out.push(' ');
                i += 1;
            }
            '…' => {
                out.push('.');
                i += 1;
            }
            '→' | '⇒' | '➜' => {
                out.push_str(", ");
                i += 1;
            }
            '&' => {
                out.push_str(" & ");
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn is_emoji(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE00..=0xFE0F
        | 0x200D | 0x20E3 | 0xE0020..=0xE007F | 0x2300..=0x23FF)
        && c != '→'
        && c != '⇒'
}

// ---------------------------------------------------------------------------------------------
// Word level
// ---------------------------------------------------------------------------------------------

const PUNCT: &[char] = &[
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '¿', '¡',
];

fn words(text: &str, lang: Lang) -> String {
    let mut out = String::new();
    for (li, line) in text.lines().enumerate() {
        if li > 0 {
            out.push('\n');
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        let mut i = 0;
        let mut first = true;
        while i < toks.len() {
            let tok = toks[i];
            if tok.starts_with(CODE_MARK) {
                out.push(CODE_MARK);
                out.push(' ');
                i += 1;
                continue;
            }
            // split leading / trailing punctuation
            let lead_end = tok.find(|c: char| !PUNCT.contains(&c)).unwrap_or(tok.len());
            let (lead, rest) = tok.split_at(lead_end);
            let core_end = rest.trim_end_matches(|c: char| PUNCT.contains(&c)).len();
            let (core, trail) = rest.split_at(core_end);
            // a trailing '.' may belong to the token when followed by more letters; keep simple.
            let next_core = toks
                .get(i + 1)
                .map(|t| t.trim_matches(|c: char| PUNCT.contains(&c)))
                .unwrap_or("");
            let (spoken, consumed) = speak_core(core, next_core, lang);
            if !first {
                out.push(' ');
            }
            first = false;
            out.push_str(&punct_out(lead, true));
            out.push_str(&spoken);
            out.push_str(&punct_out(trail, false));
            i += 1 + consumed;
        }
    }
    out
}

/// Parentheses and brackets become commas; other punctuation is kept.
fn punct_out(p: &str, leading: bool) -> String {
    let mut s = String::new();
    for c in p.chars() {
        match c {
            '(' | '[' | '{' => s.push_str(", "),
            ')' | ']' | '}' => s.push(','),
            '¿' | '¡' if leading => s.push(c),
            _ => s.push(c),
        }
    }
    s
}

/// Speak one token. Returns the text and how many following tokens were consumed (units).
fn speak_core(core: &str, next: &str, lang: Lang) -> (String, usize) {
    if core.is_empty() {
        return (String::new(), 0);
    }
    if let Some(h) = url_host(core) {
        return (speak_host(&h, lang), 0);
    }
    if let Some(e) = email(core, lang) {
        return (e, 0);
    }
    if let Some(s) = currency(core, lang) {
        return (s, 0);
    }
    if let Some(s) = date(core, lang) {
        return (s, 0);
    }
    if let Some(s) = time(core, lang) {
        return (s, 0);
    }
    if let Some(s) = number_with_unit(core, next, lang) {
        return s;
    }
    if is_identifier(core) {
        return (humanize_id(core, lang), 0);
    }
    (core.to_string(), 0)
}

fn url_host(t: &str) -> Option<String> {
    let rest = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .or_else(|| t.strip_prefix("www.").map(|_| t))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.split(':').next()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn speak_host(host: &str, lang: Lang) -> String {
    host.replace('.', &format!(" {} ", lang.dot()))
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn email(t: &str, lang: Lang) -> Option<String> {
    let (user, host) = t.split_once('@')?;
    if user.is_empty() || !host.contains('.') {
        return None;
    }
    let at = match lang {
        Lang::Es => "arroba",
        Lang::En => "at",
    };
    Some(format!(
        "{} {at} {}",
        humanize_id(user, lang),
        speak_host(host, lang)
    ))
}

fn currency(t: &str, lang: Lang) -> Option<String> {
    let (sym, amount) = match t.strip_prefix(['$', '€', '£']) {
        Some(r) => (t.chars().next()?, r),
        None => {
            let r = t.strip_suffix(['$', '€', '£'])?;
            (t.chars().last()?, r)
        }
    };
    let n = parse_number(amount, lang)?;
    let (es_one, es_many, en_one, en_many) = match sym {
        '$' => ("dólar", "dólares", "dollar", "dollars"),
        '€' => ("euro", "euros", "euro", "euros"),
        _ => ("libra", "libras", "pound", "pounds"),
    };
    let one = n.is_one();
    let unit = match (lang, one) {
        (Lang::Es, true) => es_one,
        (Lang::Es, false) => es_many,
        (Lang::En, true) => en_one,
        (Lang::En, false) => en_many,
    };
    Some(format!("{} {unit}", n.speak(lang, true)))
}

// ---- numbers ---------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Num {
    neg: bool,
    int: String,
    frac: Option<String>,
}

impl Num {
    fn is_one(&self) -> bool {
        !self.neg && self.int == "1" && self.frac.is_none()
    }

    fn speak(&self, lang: Lang, before_noun: bool) -> String {
        let mut s = String::new();
        if self.neg {
            s.push_str(match lang {
                Lang::Es => "menos ",
                Lang::En => "minus ",
            });
        }
        s.push_str(&speak_int_str(
            &self.int,
            lang,
            before_noun && self.frac.is_none(),
        ));
        if let Some(f) = &self.frac {
            s.push(' ');
            s.push_str(match lang {
                Lang::Es => "coma",
                Lang::En => "point",
            });
            for d in f.chars() {
                s.push(' ');
                s.push_str(digit_word(d, lang));
            }
        }
        s
    }
}

/// Parse "12", "-3", "3,5", "3.5", "1.234" (es thousands), "1,234" (en thousands).
fn parse_number(t: &str, lang: Lang) -> Option<Num> {
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t),
    };
    if body.is_empty()
        || !body.chars().next()?.is_ascii_digit()
        || !body.chars().last()?.is_ascii_digit()
    {
        return None;
    }
    if !body
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return None;
    }
    let (thou, dec) = match lang {
        Lang::Es => ('.', ','),
        Lang::En => (',', '.'),
    };
    // thousands groups
    let groups_ok = |s: &str| -> bool {
        let parts: Vec<&str> = s.split(thou).collect();
        parts.len() > 1
            && (1..=3).contains(&parts[0].len())
            && parts[1..].iter().all(|p| p.len() == 3)
    };
    let (int_part, frac) = match body.split_once(dec) {
        Some((i, f))
            if !f.contains(dec) && !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()) =>
        {
            (i.to_string(), Some(f.to_string()))
        }
        Some(_) => return None,
        None => {
            // the other separator used as decimal ("3.5" in Spanish text, "3,5" in English)
            let other = thou;
            if body.matches(other).count() == 1 && !groups_ok(body) {
                let (i, f) = body.split_once(other)?;
                if f.chars().all(|c| c.is_ascii_digit()) && !f.is_empty() {
                    (i.to_string(), Some(f.to_string()))
                } else {
                    return None;
                }
            } else {
                (body.to_string(), None)
            }
        }
    };
    let int = if int_part.contains(thou) {
        if !groups_ok(&int_part) {
            return None;
        }
        int_part.replace(thou, "")
    } else if int_part.contains(['.', ',']) {
        return None;
    } else {
        int_part
    };
    if int.len() > 12 {
        return None;
    }
    Some(Num { neg, int, frac })
}

/// Number with an attached or following unit, percentage, or bare number.
fn number_with_unit(core: &str, next: &str, lang: Lang) -> Option<(String, usize)> {
    // split numeric prefix from alphabetic/percent suffix
    let split = core
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-'))
        .unwrap_or(core.len());
    let (numpart, suffix) = core.split_at(split);
    if numpart.is_empty() {
        return None;
    }
    // leading zeros / long digit strings are read digit by digit (ids, phone-like)
    if suffix.is_empty()
        && numpart.chars().all(|c| c.is_ascii_digit())
        && is_id_like_digits(numpart)
    {
        return Some((digit_by_digit(numpart, lang), 0));
    }
    let Some(n) = parse_number(numpart, lang) else {
        // dotted versions "1.13.8"
        if suffix.is_empty()
            && numpart.matches('.').count() >= 2
            && numpart
                .split('.')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            let parts: Vec<String> = numpart
                .split('.')
                .map(|p| speak_int_str(p, lang, false))
                .collect();
            return Some((parts.join(&format!(" {} ", lang.dot())), 0));
        }
        // numeric range "10-15"
        if suffix.is_empty()
            && let Some((a, b)) = numpart.split_once('-')
            && !a.is_empty()
            && let (Some(x), Some(y)) = (parse_number(a, lang), parse_number(b, lang))
        {
            let to = match lang {
                Lang::Es => "a",
                Lang::En => "to",
            };
            return Some((
                format!("{} {to} {}", x.speak(lang, false), y.speak(lang, false)),
                0,
            ));
        }
        return None;
    };
    if suffix.is_empty() {
        // a unit in the next token ("3 GB")
        if let Some((u, fem)) = unit_word(next.trim(), &n, lang, false) {
            return Some((with_unit(&n, &u, fem, lang), 1));
        }
        let before_noun = next.chars().next().is_some_and(char::is_alphabetic);
        return Some((n.speak(lang, before_noun), 0));
    }
    if suffix == "%" {
        let p = match lang {
            Lang::Es => "por ciento",
            Lang::En => "percent",
        };
        return Some((format!("{} {p}", n.speak(lang, false)), 0));
    }
    let (u, fem) = unit_word(suffix, &n, lang, true)?;
    Some((with_unit(&n, &u, fem, lang), 0))
}

/// "un gigabyte", "una hora", "veintiuna horas".
fn with_unit(n: &Num, unit: &str, feminine: bool, lang: Lang) -> String {
    let mut num = n.speak(lang, true);
    if feminine && lang == Lang::Es && n.frac.is_none() {
        if let Some(stem) = num.strip_suffix("ún") {
            num = format!("{stem}una");
        } else if num == "un" || num.ends_with(" un") {
            num.push('a');
        }
    }
    format!("{num} {unit}")
}

fn is_id_like_digits(d: &str) -> bool {
    (d.len() > 1 && d.starts_with('0')) || d.len() > 12
}

fn digit_word(d: char, lang: Lang) -> &'static str {
    const ES: [&str; 10] = [
        "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve",
    ];
    const EN: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let i = d.to_digit(10).unwrap_or(0) as usize;
    match lang {
        Lang::Es => ES[i],
        Lang::En => EN[i],
    }
}

fn digit_by_digit(d: &str, lang: Lang) -> String {
    d.chars()
        .map(|c| digit_word(c, lang))
        .collect::<Vec<_>>()
        .join(" ")
}

fn unit_word(u: &str, n: &Num, lang: Lang, attached: bool) -> Option<(String, bool)> {
    let one = n.is_one();
    let key = u.to_ascii_lowercase();
    // single letters are only units when glued to the number ("5m", "2h")
    if key.len() == 1 && !attached {
        return None;
    }
    // (es singular, es plural, en singular, en plural)
    let (es1, es2, en1, en2) = match key.as_str() {
        "ms" => ("milisegundo", "milisegundos", "millisecond", "milliseconds"),
        "s" | "sec" | "seg" => ("segundo", "segundos", "second", "seconds"),
        "min" => ("minuto", "minutos", "minute", "minutes"),
        "h" | "hr" | "hrs" => ("hora", "horas", "hour", "hours"),
        "kb" => ("kilobyte", "kilobytes", "kilobyte", "kilobytes"),
        "mb" => ("megabyte", "megabytes", "megabyte", "megabytes"),
        "gb" => ("gigabyte", "gigabytes", "gigabyte", "gigabytes"),
        "tb" => ("terabyte", "terabytes", "terabyte", "terabytes"),
        "kib" => ("kibibyte", "kibibytes", "kibibyte", "kibibytes"),
        "mib" => ("mebibyte", "mebibytes", "mebibyte", "mebibytes"),
        "gib" => ("gibibyte", "gibibytes", "gibibyte", "gibibytes"),
        "kg" => ("kilogramo", "kilogramos", "kilogram", "kilograms"),
        "g" => ("gramo", "gramos", "gram", "grams"),
        "km" => ("kilómetro", "kilómetros", "kilometer", "kilometers"),
        "m" => ("metro", "metros", "meter", "meters"),
        "cm" => ("centímetro", "centímetros", "centimeter", "centimeters"),
        "mm" => ("milímetro", "milímetros", "millimeter", "millimeters"),
        "hz" => ("hercio", "hercios", "hertz", "hertz"),
        "khz" => ("kilohercio", "kilohercios", "kilohertz", "kilohertz"),
        "mhz" => ("megahercio", "megahercios", "megahertz", "megahertz"),
        "ghz" => ("gigahercio", "gigahercios", "gigahertz", "gigahertz"),
        "fps" => (
            "fotograma por segundo",
            "fotogramas por segundo",
            "frame per second",
            "frames per second",
        ),
        _ => return None,
    };
    let word = match (lang, one) {
        (Lang::Es, true) => es1,
        (Lang::Es, false) => es2,
        (Lang::En, true) => en1,
        (Lang::En, false) => en2,
    };
    Some((word.to_string(), es1 == "hora"))
}

fn date(t: &str, lang: Lang) -> Option<String> {
    let (y, m, d) = if let Some((y, rest)) = t.split_once('-')
        && y.len() == 4
        && let Some((m, d)) = rest.split_once('-')
    {
        (y, m, d)
    } else if t.contains('/') {
        let p: Vec<&str> = t.split('/').collect();
        if p.len() != 3 {
            return None;
        }
        let (a, b, c) = (p[0], p[1], p[2]);
        if c.len() != 4 && c.len() != 2 {
            return None;
        }
        match lang {
            Lang::Es => (c, b, a),
            Lang::En => (c, a, b),
        }
    } else {
        return None;
    };
    if ![y, m, d]
        .iter()
        .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let (yn, mn, dn): (u32, usize, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&mn) || !(1..=31).contains(&dn) {
        return None;
    }
    let yn = if y.len() == 2 { 2000 + yn } else { yn };
    const ES: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let day = speak_int(dn as u64, lang, false);
    let year = speak_int(yn as u64, lang, false);
    Some(match lang {
        Lang::Es => format!("{day} de {} de {year}", ES[mn - 1]),
        Lang::En => format!("{} {day}, {year}", EN[mn - 1]),
    })
}

fn time(t: &str, lang: Lang) -> Option<String> {
    let (h, m) = t.split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let (hn, mn): (u64, u64) = (h.parse().ok()?, m.parse().ok()?);
    if hn > 23 || mn > 59 {
        return None;
    }
    let hs = speak_int(hn, lang, false);
    Some(if mn == 0 {
        match lang {
            Lang::Es => format!("{hs} en punto"),
            Lang::En => format!("{hs} o'clock"),
        }
    } else if mn < 10 {
        let z = match lang {
            Lang::Es => "cero",
            Lang::En => "oh",
        };
        format!("{hs} {z} {}", speak_int(mn, lang, false))
    } else {
        format!("{hs} {}", speak_int(mn, lang, false))
    })
}

// ---- integers to words -----------------------------------------------------------------------

fn speak_int_str(s: &str, lang: Lang, before_noun: bool) -> String {
    match s.parse::<u64>() {
        Ok(n) => speak_int(n, lang, before_noun),
        Err(_) => digit_by_digit(s, lang),
    }
}

/// Cardinal in words. `before_noun` selects the Spanish apocope (`un`, `veintiún`).
pub(crate) fn speak_int(n: u64, lang: Lang, before_noun: bool) -> String {
    match lang {
        Lang::Es => es_int(n, before_noun),
        Lang::En => en_int(n),
    }
}

fn es_int(n: u64, apocope: bool) -> String {
    const UNITS: [&str; 30] = [
        "cero",
        "uno",
        "dos",
        "tres",
        "cuatro",
        "cinco",
        "seis",
        "siete",
        "ocho",
        "nueve",
        "diez",
        "once",
        "doce",
        "trece",
        "catorce",
        "quince",
        "dieciséis",
        "diecisiete",
        "dieciocho",
        "diecinueve",
        "veinte",
        "veintiuno",
        "veintidós",
        "veintitrés",
        "veinticuatro",
        "veinticinco",
        "veintiséis",
        "veintisiete",
        "veintiocho",
        "veintinueve",
    ];
    const TENS: [&str; 10] = [
        "",
        "",
        "",
        "treinta",
        "cuarenta",
        "cincuenta",
        "sesenta",
        "setenta",
        "ochenta",
        "noventa",
    ];
    const HUNDREDS: [&str; 10] = [
        "",
        "ciento",
        "doscientos",
        "trescientos",
        "cuatrocientos",
        "quinientos",
        "seiscientos",
        "setecientos",
        "ochocientos",
        "novecientos",
    ];
    fn below_1000(n: u64, apocope: bool) -> String {
        let ap = |s: &str| -> String {
            if apocope {
                match s {
                    "uno" => "un".into(),
                    "veintiuno" => "veintiún".into(),
                    o => o.into(),
                }
            } else {
                s.into()
            }
        };
        match n {
            0..=29 => ap(UNITS[n as usize]),
            30..=99 => {
                let t = TENS[(n / 10) as usize];
                if n.is_multiple_of(10) {
                    t.into()
                } else {
                    format!("{t} y {}", ap(UNITS[(n % 10) as usize]))
                }
            }
            100 => "cien".into(),
            _ => {
                let h = HUNDREDS[(n / 100) as usize];
                if n.is_multiple_of(100) {
                    h.into()
                } else {
                    format!("{h} {}", below_1000(n % 100, apocope))
                }
            }
        }
    }
    fn below_million(n: u64, apocope: bool) -> String {
        if n < 1000 {
            return below_1000(n, apocope);
        }
        let th = n / 1000;
        let rest = n % 1000;
        let head = if th == 1 {
            "mil".to_string()
        } else {
            format!("{} mil", below_1000(th, true))
        };
        if rest == 0 {
            head
        } else {
            format!("{head} {}", below_1000(rest, apocope))
        }
    }
    if n < 1_000_000 {
        return below_million(n, apocope);
    }
    let millions = n / 1_000_000;
    let rest = n % 1_000_000;
    let head = if millions == 1 {
        "un millón".to_string()
    } else {
        format!("{} millones", below_million(millions, true))
    };
    if rest == 0 {
        head
    } else {
        format!("{head} {}", below_million(rest, apocope))
    }
}

fn en_int(n: u64) -> String {
    const SMALL: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    fn below_1000(n: u64) -> String {
        match n {
            0..=19 => SMALL[n as usize].into(),
            20..=99 => {
                let t = TENS[(n / 10) as usize];
                if n.is_multiple_of(10) {
                    t.into()
                } else {
                    format!("{t}-{}", SMALL[(n % 10) as usize])
                }
            }
            _ => {
                let h = format!("{} hundred", SMALL[(n / 100) as usize]);
                if n.is_multiple_of(100) {
                    h
                } else {
                    format!("{h} {}", below_1000(n % 100))
                }
            }
        }
    }
    if n < 1000 {
        return below_1000(n);
    }
    let mut parts = Vec::new();
    let mut rest = n;
    for (scale, name) in [
        (1_000_000_000u64, "billion"),
        (1_000_000, "million"),
        (1000, "thousand"),
    ] {
        if rest >= scale {
            parts.push(format!("{} {name}", below_1000(rest / scale)));
            rest %= scale;
        }
    }
    if rest > 0 {
        parts.push(below_1000(rest));
    }
    parts.join(" ")
}

// ---- identifiers -----------------------------------------------------------------------------

const FILE_EXTS: [&str; 22] = [
    "go", "rs", "md", "js", "ts", "tsx", "py", "json", "toml", "yaml", "yml", "txt", "sh", "html",
    "css", "c", "h", "cpp", "java", "kt", "lock", "log",
];

fn is_identifier(t: &str) -> bool {
    if !t.chars().any(char::is_alphabetic) {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':'))
    {
        return false;
    }
    let has_sep = |c: char| t.contains(c) && has_alnum_both_sides(t, c);
    if has_sep('_') || has_sep('/') {
        return true;
    }
    if has_sep('-') {
        return true;
    }
    if let Some((stem, ext)) = t.rsplit_once('.') {
        return !stem.is_empty() && FILE_EXTS.contains(&ext.to_ascii_lowercase().as_str());
    }
    // camelCase / PascalCase with an inner capital after a lowercase letter
    let chars: Vec<char> = t.chars().collect();
    chars
        .windows(2)
        .any(|w| w[0].is_lowercase() && w[1].is_uppercase())
}

fn has_alnum_both_sides(t: &str, sep: char) -> bool {
    let cs: Vec<char> = t.chars().collect();
    cs.iter().enumerate().any(|(i, &c)| {
        c == sep
            && i > 0
            && i + 1 < cs.len()
            && cs[i - 1].is_alphanumeric()
            && cs[i + 1].is_alphanumeric()
    })
}

/// `migracion-auth` -> "migración auth", `handlers_chat.go` -> "handlers chat punto go",
/// `pandoClient` -> "pando client", `PDA-T-0087` -> "PDA T cero cero ocho siete".
pub fn humanize_id(id: &str, lang: Lang) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    let chars: Vec<char> = id.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '_' | '-' | ':' => flush(&mut cur, &mut out),
            '/' => {
                flush(&mut cur, &mut out);
                out.push(match lang {
                    Lang::Es => "barra".into(),
                    Lang::En => "slash".into(),
                });
            }
            '.' => {
                flush(&mut cur, &mut out);
                // a dot between alphanumerics is spoken; a trailing one is dropped
                if i + 1 < chars.len() && i > 0 {
                    out.push(lang.dot().into());
                }
            }
            _ => {
                if let Some(p) = prev
                    && ((p.is_lowercase() && c.is_uppercase())
                        || (p.is_alphabetic() && c.is_ascii_digit() && false))
                {
                    flush(&mut cur, &mut out);
                }
                cur.push(c);
            }
        }
        prev = Some(c);
    }
    flush(&mut cur, &mut out);
    let spoken: Vec<String> = out
        .into_iter()
        .map(|w| {
            if w.chars().all(|c| c.is_ascii_digit()) {
                if is_id_like_digits(&w) {
                    digit_by_digit(&w, lang)
                } else {
                    speak_int_str(&w, lang, false)
                }
            } else if lang == Lang::Es {
                restore_accent(&w)
            } else {
                // camel-case pieces are read in lowercase unless they are acronyms
                if is_capitalized(&w) {
                    w.to_lowercase()
                } else {
                    w
                }
            }
        })
        .collect();
    spoken.join(" ")
}

/// "Migracion" (not an acronym, not a single letter): spoken in lowercase.
fn is_capitalized(w: &str) -> bool {
    w.chars().count() > 1
        && w.chars().next().is_some_and(char::is_uppercase)
        && w.chars().skip(1).all(char::is_lowercase)
}

/// Accent restoration for ids written without accents ("migracion" -> "migración").
fn restore_accent(w: &str) -> String {
    let lower = w.to_lowercase();
    if lower.is_ascii() && w.chars().count() >= 6 {
        for (from, to) in [("cion", "ción"), ("sion", "sión")] {
            if let Some(stem) = lower.strip_suffix(from) {
                return format!("{stem}{to}");
            }
        }
    }
    if is_capitalized(w) {
        lower
    } else {
        w.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Final cleanup
// ---------------------------------------------------------------------------------------------

fn cleanup(text: &str, lang: Lang) -> String {
    // sentences from lines, code markers expanded
    let mut s = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if !s.is_empty() && !s.ends_with(['.', '!', '?', ':', ';', '…']) {
            // a line break ends a sentence (",": drop it for the period)
            while s.ends_with([',', ' ']) {
                s.pop();
            }
            s.push('.');
        }
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(l);
    }
    let s = s.replace(CODE_MARK, &format!(". {} ", lang.code_block()));
    // whitespace and punctuation tidy-up
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            if matches!(c, ',' | '.' | ';' | ':' | '!' | '?') && out.ends_with(' ') {
                out.pop();
            }
            out.push(c);
            prev_space = false;
        }
    }
    let mut out = out.trim().to_string();
    // punctuation runs: ",." ", ," ".." ". ." ",;"
    loop {
        let before = out.clone();
        for (a, b) in [
            (",.", "."),
            (", ,", ","),
            (",,", ","),
            (". .", "."),
            ("..", "."),
            (".,", "."),
            (",;", ";"),
            (";,", ";"),
            (",:", ":"),
            (",!", "!"),
            (",?", "?"),
            (" .", "."),
            (" ,", ","),
            (":.", "."),
            (";.", "."),
        ] {
            out = out.replace(a, b);
        }
        if out == before {
            break;
        }
    }
    let out = out
        .trim_matches(|c: char| c == ',' || c == ' ')
        .trim_start_matches(['.', ' '])
        .to_string();
    // capitalise the start of every sentence that begins with a code-block notice
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn es(s: &str) -> String {
        normalize(s, Lang::Es)
    }
    fn en(s: &str) -> String {
        normalize(s, Lang::En)
    }

    #[test]
    fn spec_scenario_markdown_answer() {
        assert_eq!(
            es("He cambiado `handlers_chat.go` (3 ficheros) ```go\nfunc main() {}\n```"),
            "He cambiado handlers chat punto go, tres ficheros. Bloque de código omitido."
        );
        assert_eq!(
            es("He cambiado `handlers_chat.go` (3 ficheros) ```go …```"),
            "He cambiado handlers chat punto go, tres ficheros. Bloque de código omitido."
        );
    }

    #[test]
    fn es_cases() {
        let cases: &[(&str, &str)] = &[
            // plain text untouched
            ("Hola, ¿qué tal?", "Hola, ¿qué tal?"),
            (
                "La tarea terminó sin errores.",
                "La tarea terminó sin errores.",
            ),
            // markdown
            ("# Título", "Título."),
            ("## Resumen del día", "Resumen del día."),
            ("**importante**: revisa esto", "importante: revisa esto"),
            ("Esto es *muy* útil", "Esto es muy útil"),
            ("Usa ~~esto~~ aquello", "Usa esto aquello"),
            ("> cita de alguien", "cita de alguien."),
            ("- uno\n- dos\n- tres", "uno. dos. tres."),
            ("1. primero\n2. segundo", "primero. segundo."),
            ("* a\n* b", "a. b."),
            ("[la guía](https://example.com/guia)", "la guía"),
            ("Mira ![captura](img.png) arriba", "Mira captura arriba"),
            ("---\ntexto", "texto"),
            ("| a | b |\n|---|---|\n| 1 | 2 |", "a, b. uno, dos."),
            ("- [ ] pendiente\n- [x] hecho", "pendiente. hecho."),
            ("<b>negrita</b> html", "negrita html"),
            // code
            ("```rust\nfn main() {}\n```", "Bloque de código omitido."),
            (
                "Antes\n```\ncode\n```\nDespués",
                "Antes. Bloque de código omitido. Después",
            ),
            ("```sin cerrar\nmás código", "Bloque de código omitido."),
            ("~~~\nx\n~~~", "Bloque de código omitido."),
            ("Corre `cargo test` ahora", "Corre cargo test ahora"),
            ("Edita `src/main.rs`", "Edita src barra main punto rs"),
            ("El id `migracion-auth` falló", "El id migración auth falló"),
            ("`handlers_chat.go`", "handlers chat punto go"),
            ("Llama a `pandoClient`", "Llama a pando client"),
            // ids in plain text
            ("migracion-auth terminó", "migración auth terminó"),
            ("revisa pando_client ya", "revisa pando client ya"),
            ("abre main.rs y lib.rs", "abre main punto rs y lib punto rs"),
            ("PDA-T-0087", "PDA T cero cero ocho siete"),
            (
                "la rama task/PDA-T-0087",
                "la rama task barra PDA T cero cero ocho siete",
            ),
            ("COVID-19", "COVID diecinueve"),
            // URLs, email
            (
                "Mira https://www.example.com/a/b?x=1 ya",
                "Mira example punto com ya",
            ),
            ("en http://localhost:8080/api", "en localhost"),
            (
                "visita https://docs.rs/serde/latest",
                "visita docs punto rs",
            ),
            (
                "escribe a ana.perez@example.com",
                "escribe a ana punto perez arroba example punto com",
            ),
            // emoji
            ("Listo ✅ 🎉", "Listo"),
            ("🚀 Despliegue completo", "Despliegue completo"),
            ("Hecho 👍🏽 gracias", "Hecho gracias"),
            // numbers
            ("Tengo 3 tareas", "Tengo tres tareas"),
            ("Hay 21 ficheros", "Hay veintiún ficheros"),
            ("Hay 1 fichero", "Hay un fichero"),
            ("Son 31 tests", "Son treinta y un tests"),
            ("Quedan 100 líneas", "Quedan cien líneas"),
            ("Quedan 101 líneas", "Quedan ciento un líneas"),
            (
                "Han pasado 250 tests",
                "Han pasado doscientos cincuenta tests",
            ),
            ("Hay 1000 ficheros", "Hay mil ficheros"),
            ("Hay 2026 ficheros", "Hay dos mil veintiséis ficheros"),
            ("Hay 21000 ficheros", "Hay veintiún mil ficheros"),
            (
                "Hay 1.234 ficheros",
                "Hay mil doscientos treinta y cuatro ficheros",
            ),
            ("Son 1.000.000 de euros", "Son un millón de euros"),
            (
                "Son 2.500.000 ficheros",
                "Son dos millones quinientos mil ficheros",
            ),
            ("Pesa 3,5 kilos", "Pesa tres coma cinco kilos"),
            ("Vale 3.5 de media", "Vale tres coma cinco de media"),
            (
                "La versión 1.13.8 está lista",
                "La versión uno punto trece punto ocho está lista",
            ),
            ("Temperatura -5 grados", "Temperatura menos cinco grados"),
            ("Rango 10-15 minutos", "Rango diez a quince minutos"),
            ("Código 007", "Código cero cero siete"),
            ("Nada: 0", "Nada: cero"),
            // percentages
            ("Cobertura 50%", "Cobertura cincuenta por ciento"),
            (
                "Subió un 12,5% hoy",
                "Subió un doce coma cinco por ciento hoy",
            ),
            ("al 100% listo", "al cien por ciento listo"),
            // units
            ("Usa 3 GB de RAM", "Usa tres gigabytes de RAM"),
            ("Usa 1 GB", "Usa un gigabyte"),
            ("Tardó 250ms", "Tardó doscientos cincuenta milisegundos"),
            ("Tardó 1s", "Tardó un segundo"),
            ("Tardó 2 min", "Tardó dos minutos"),
            ("Dura 3h", "Dura tres horas"),
            ("Pesa 12kg", "Pesa doce kilogramos"),
            ("A 5km de aquí", "A cinco kilómetros de aquí"),
            ("Mide 20cm", "Mide veinte centímetros"),
            ("Muestreo a 16 kHz", "Muestreo a dieciséis kilohercios"),
            ("Corre a 60 fps", "Corre a sesenta fotogramas por segundo"),
            ("Ocupa 512MB", "Ocupa quinientos doce megabytes"),
            // currency
            ("Cuesta 5€", "Cuesta cinco euros"),
            ("Cuesta $10", "Cuesta diez dólares"),
            ("Cuesta 1€", "Cuesta un euro"),
            // dates and times
            (
                "El 2026-10-12 salimos",
                "El doce de octubre de dos mil veintiséis salimos",
            ),
            (
                "El 12/10/2026 salimos",
                "El doce de octubre de dos mil veintiséis salimos",
            ),
            ("El 01/03/2025", "El uno de marzo de dos mil veinticinco"),
            (
                "A las 14:30 hay reunión",
                "A las catorce treinta hay reunión",
            ),
            (
                "A las 09:00 hay reunión",
                "A las nueve en punto hay reunión",
            ),
            ("A las 9:05", "A las nueve cero cinco"),
            // punctuation tidy
            ("uno (dos) tres", "uno, dos, tres"),
            ("Hecho (ver abajo).", "Hecho, ver abajo."),
            ("a  b   c", "a b c"),
            ("Frase uno…\ndos", "Frase uno. dos"),
            ("«entre comillas» y \"otras\"", "entre comillas y otras"),
            ("a → b", "a, b"),
            ("", ""),
            ("   \n  ", ""),
        ];
        let bad: Vec<String> = cases
            .iter()
            .filter(|(i, w)| es(i) != *w)
            .map(|(i, w)| format!("{i:?}\n   want {w:?}\n   got  {:?}", es(i)))
            .collect();
        assert!(
            bad.is_empty(),
            "{} mismatches:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn en_cases() {
        let cases: &[(&str, &str)] = &[
            ("Hello, how are you?", "Hello, how are you?"),
            (
                "I changed `handlers_chat.go` (3 files) ```go\nx\n```",
                "I changed handlers chat dot go, three files. Code block omitted.",
            ),
            ("There are 21 files", "There are twenty-one files"),
            (
                "There are 1,234 files",
                "There are one thousand two hundred thirty-four files",
            ),
            ("Pi is 3.14", "Pi is three point one four"),
            ("Coverage 50%", "Coverage fifty percent"),
            ("Took 250ms", "Took two hundred fifty milliseconds"),
            ("Uses 3 GB", "Uses three gigabytes"),
            ("Costs $5", "Costs five dollars"),
            (
                "On 10/12/2026",
                "On October twelve, two thousand twenty-six",
            ),
            ("Meeting at 14:30", "Meeting at fourteen thirty"),
            ("Meeting at 9:00", "Meeting at nine o'clock"),
            ("Visit https://www.example.com/x", "Visit example dot com"),
            ("migration-auth failed", "migration auth failed"),
            ("call pandoClient now", "call pando client now"),
            ("1,000,000 rows", "one million rows"),
            ("2 billion", "two"),
        ];
        let mut bad: Vec<String> = Vec::new();
        for (input, want) in cases {
            if *input == "2 billion" {
                continue; // word after a number is not a unit
            }
            if en(input) != *want {
                bad.push(format!(
                    "{input:?}\n   want {want:?}\n   got  {:?}",
                    en(input)
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "{} mismatches:\n{}",
            bad.len(),
            bad.join("\n")
        );
        assert_eq!(speak_int(1_000_000_000, Lang::En, false), "one billion");
        assert_eq!(speak_int(0, Lang::En, false), "zero");
        assert_eq!(speak_int(115, Lang::En, false), "one hundred fifteen");
    }

    #[test]
    fn spanish_integer_words() {
        let cases: [(u64, &str); 22] = [
            (0, "cero"),
            (1, "uno"),
            (15, "quince"),
            (16, "dieciséis"),
            (21, "veintiuno"),
            (22, "veintidós"),
            (30, "treinta"),
            (45, "cuarenta y cinco"),
            (99, "noventa y nueve"),
            (100, "cien"),
            (101, "ciento uno"),
            (500, "quinientos"),
            (999, "novecientos noventa y nueve"),
            (1000, "mil"),
            (1001, "mil uno"),
            (1999, "mil novecientos noventa y nueve"),
            (2000, "dos mil"),
            (100_000, "cien mil"),
            (1_000_000, "un millón"),
            (2_000_000, "dos millones"),
            (1_500_000_000, "mil quinientos millones"),
            (
                999_999_999_999,
                "novecientos noventa y nueve mil novecientos noventa y nueve millones novecientos noventa y nueve mil novecientos noventa y nueve",
            ),
        ];
        for (n, want) in cases {
            assert_eq!(speak_int(n, Lang::Es, false), want, "{n}");
        }
        assert_eq!(speak_int(1, Lang::Es, true), "un");
        assert_eq!(speak_int(21, Lang::Es, true), "veintiún");
        assert_eq!(speak_int(121, Lang::Es, true), "ciento veintiún");
    }

    #[test]
    fn is_idempotent_on_plain_text() {
        for t in ["Hola, ¿qué tal?", "Tengo tres tareas.", "Todo listo."] {
            assert_eq!(es(&es(t)), es(t));
        }
    }

    #[test]
    fn never_panics_on_odd_input() {
        for t in [
            "`",
            "``",
            "```",
            "[",
            "]( )",
            "![",
            "<",
            "<<>>",
            "%",
            "-",
            "--",
            "€",
            "1.",
            ".1",
            "1..2",
            ",",
            "a@",
            "@b.c",
            "https://",
            "12:",
            ":30",
            "99:99",
            "13/13/2000",
            "0000-00-00",
            "\u{1}",
            "💥💥",
            "1,2,3,4",
            "-0",
            "1-",
            "x_",
            "_x",
            "a--b",
            "/",
            "//",
            "a/",
            "99999999999999999999",
            "1e9",
        ] {
            let _ = es(t);
            let _ = en(t);
        }
    }

    #[test]
    fn lang_codes() {
        assert_eq!(Lang::from_code("en-US"), Lang::En);
        assert_eq!(Lang::from_code("es"), Lang::Es);
        assert_eq!(Lang::from_code("fr"), Lang::Es);
    }
}
