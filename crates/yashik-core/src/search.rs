//! Поиск по почте: слово находится в любой форме, в транслите и набранное не в той раскладке.
//!
//! **Указатель.** Каждое слово письма записывается ключом — латиницей в одном написании.
//! Кириллица переводится по таблице (`щ` → `sh`, `я` → `ia`, `ё` как `е`, `сч` как `щ`), латиница
//! приводится к тому же (`shch`, `sch` → `sh`; `kh` → `h`; `y`, `j` → `i`; `x` → `ks`; `w` → `v`;
//! `c` → `k`, кроме `ch`), удвоенные буквы пишутся одной. «Отчёт», «отчет» и «otchet» дают ключ
//! `otchet`; «Щукин», «Shchukin» и «Schukin» — `shukin`; «Юлия», «Yulia» и «Julia» — `iulia`;
//! «Фёдор» и «Fyodor» — `fedor`.
//!
//! **Запрос.** Слово запроса превращается в несколько начал ключа: ключ основы слова (стеммер
//! Snowball) и ключ самого слова. Письмо подходит, если у каждого слова запроса какое-то начало
//! совпало с началом какого-то слова письма: «отчёты» → основа «отчет» → находит «отчёта»,
//! «отчётов», «otchetom». Слово латиницей читается дважды — по-английски (английская основа) и
//! по-русски (переводится в кириллицу, берётся русская основа): «otchety» находит «отчёт».
//!
//! **Раскладка.** Если по запросу ничего не нашлось, его стоит поискать в другой раскладке —
//! [`other_layout`]: «ghbdtn» — это «привет», «руддщ» — «hello».
//!
//! Указатель — таблица FTS5 в той же зашифрованной базе: в неё пишется [`index_text`] письма,
//! ищется по [`Query::fts`].

use rust_stemmers::{Algorithm, Stemmer};

/// Разобранная строка поиска: для каждого слова — начала ключей, любое из которых подходит.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    words: Vec<Vec<String>>,
}

impl Query {
    /// Разбирает строку поиска. Слова короче двух знаков не ищутся.
    pub fn parse(text: &str) -> Query {
        Query { words: words(text).iter().filter_map(|w| starts(w)).collect() }
    }

    /// Искать нечего: в запросе нет слов от двух знаков.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Начала ключей по словам запроса.
    pub fn words(&self) -> &[Vec<String>] {
        &self.words
    }

    /// Выражение для `MATCH` таблицы FTS5, куда записан [`index_text`]: слова — через AND,
    /// начала одного слова — через OR. `None` — искать нечего.
    pub fn fts(&self) -> Option<String> {
        if self.words.is_empty() {
            return None;
        }
        let word = |starts: &Vec<String>| {
            let terms: Vec<String> = starts.iter().map(|s| format!("\"{s}\"*")).collect();
            if terms.len() == 1 {
                terms[0].clone()
            } else {
                format!("({})", terms.join(" OR "))
            }
        };
        Some(self.words.iter().map(word).collect::<Vec<_>>().join(" AND "))
    }

    /// Подходит ли текст с такими ключами ([`keys`]): у каждого слова запроса есть начало,
    /// с которого начинается какой-то ключ. Для коротких списков — адресов в подсказках, проверок.
    pub fn matches(&self, keys: &[String]) -> bool {
        self.words.iter().all(|starts| starts.iter().any(|s| keys.iter().any(|k| k.starts_with(s.as_str()))))
    }
}

/// Ключи слов текста по порядку — то, по чему ищется.
pub fn keys(text: &str) -> Vec<String> {
    words(text).iter().map(|w| key(&plain(w))).filter(|k| !k.is_empty()).collect()
}

/// Текст для указателя FTS5: ключи слов через пробел.
pub fn index_text(text: &str) -> String {
    keys(text).join(" ")
}

const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>~";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюёЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮЁ";

/// Тот же запрос в другой раскладке: набранное латиницей — по-русски («ghjljk;fq» — «продолжай»),
/// кириллицей — по-английски. `None` — переводить нечего.
pub fn other_layout(text: &str) -> Option<String> {
    let lat = text.chars().filter(char::is_ascii_alphabetic).count();
    let cyr = text.chars().filter(|&c| is_cyr(c)).count();
    let (from, to) = if lat > cyr {
        (EN, RU)
    } else if cyr > 0 {
        (RU, EN)
    } else {
        return None;
    };
    let out: String = text.chars().map(|c| from.chars().position(|f| f == c).and_then(|i| to.chars().nth(i)).unwrap_or(c)).collect();
    (out != text).then_some(out)
}

/// Слова текста: буквы и цифры подряд. Апостроф между буквами — часть слова (Ol'ga), надстрочный
/// знак, набранный отдельно («й» как «и» + «˘»), — тоже.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c.is_alphanumeric() || (is_mark(c) && !cur.is_empty()) {
            cur.push(c);
        } else if matches!(c, '\'' | '’' | 'ʼ' | '`') && !cur.is_empty() && it.peek().is_some_and(|n| n.is_alphabetic()) {
            cur.push('\'');
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Слово в нижнем регистре, без надстрочных знаков и апострофов, «ё» — «е». В русском слове
/// похожие латинские буквы — русские, в латинском — наоборот: «пpивет» с латинской «p» — «привет».
fn plain(word: &str) -> String {
    let cyr = word.chars().filter(|&c| is_cyr(c)).count();
    let lat = word.chars().filter(|&c| is_lat(c)).count();
    let fix = |c: char| match (cyr, lat) {
        (0, _) | (_, 0) => c,
        _ if cyr >= lat => lookalike(c, LAT_LOOK, CYR_LOOK),
        _ => lookalike(c, CYR_LOOK, LAT_LOOK),
    };
    word.chars()
        .map(fix)
        .flat_map(char::to_lowercase)
        .filter(|&c| !is_mark(c) && c != '\'')
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect()
}

const LAT_LOOK: &str = "aceopxyABCEHKMOPTXY";
const CYR_LOOK: &str = "асеорхуАВСЕНКМОРТХУ";

fn lookalike(c: char, from: &str, to: &str) -> char {
    from.chars().position(|f| f == c).and_then(|i| to.chars().nth(i)).unwrap_or(c)
}

fn is_cyr(c: char) -> bool {
    ('\u{400}'..='\u{4ff}').contains(&c)
}

fn is_lat(c: char) -> bool {
    c.is_ascii_alphabetic() || deaccent(c.to_lowercase().next().unwrap_or(c)).is_some()
}

fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Латинская буква с надстрочным знаком — без него; чешские и научные «č», «š», «ž» — как ch, sh, zh.
fn deaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'ĉ' => "c",
        'č' => "ch",
        'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ğ' | 'ĝ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => "i",
        'ł' | 'ľ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'ř' => "r",
        'ś' | 'ş' => "s",
        'š' | 'ŝ' => "sh",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' => "z",
        'ž' => "zh",
        'æ' => "ae",
        'œ' => "oe",
        _ => return None,
    })
}

/// Русская (и украинская, белорусская) буква латиницей.
fn cyr_key(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' | 'ґ' => "g",
        'д' => "d",
        'е' | 'ё' | 'э' | 'є' => "e",
        'ж' => "zh",
        'з' => "z",
        'и' | 'й' | 'ы' | 'і' | 'ї' => "i",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' | 'ў' => "u",
        'ф' => "f",
        'х' => "h",
        'ц' => "ts",
        'ч' => "ch",
        'ш' | 'щ' => "sh",
        'ъ' | 'ь' => "",
        'ю' => "iu",
        'я' => "ia",
        _ => return None,
    })
}

/// Ключ слова (уже [`plain`]): латиница в одном написании.
fn key(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::new();
    let mut latin = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_lowercase() {
            latin.push(c);
        } else if let Some(s) = deaccent(c) {
            latin.push_str(s);
        } else {
            if !latin.is_empty() {
                out.push_str(&latin_key(&latin));
                latin.clear();
            }
            if c == 'с' && chars.get(i + 1) == Some(&'ч') {
                // «сч» звучит как «щ» и пишется латиницей так же: «счёт» — schet.
                out.push_str("sh");
                i += 1;
            } else {
                match cyr_key(c) {
                    Some(s) => out.push_str(s),
                    None => out.push(c),
                }
            }
        }
        i += 1;
    }
    if !latin.is_empty() {
        out.push_str(&latin_key(&latin));
    }
    collapse(&vowels(&out))
}

/// Латиница в одно написание: разные транслиты одной русской буквы дают одно и то же.
fn latin_key(run: &str) -> String {
    let s = run
        .replace("shch", "sh")
        .replace("sch", "sh")
        .replace("kh", "h")
        .replace("ph", "f")
        .replace("ck", "k")
        .replace("tz", "ts")
        .replace('x', "ks")
        .replace('w', "v")
        .replace('q', "k");
    let b: Vec<char> = s.chars().collect();
    b.iter()
        .enumerate()
        .map(|(i, &c)| match c {
            'c' if b.get(i + 1) != Some(&'h') => 'k',
            'y' | 'j' => 'i',
            _ => c,
        })
        .collect()
}

/// «io» после согласной и «ie» в начале или после гласной — «e»: так «ё» и «е» пишут латиницей
/// (Fyodor = Фёдор, Yelena = Елена, Sergeyev = Сергеев). Для обеих письменностей одинаково.
fn vowels(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == 'i' && i + 1 < c.len() {
            let prev = i.checked_sub(1).map(|p| c[p]);
            let after_consonant = prev.is_some_and(|p| p.is_ascii_lowercase() && !is_vowel(p));
            if (c[i + 1] == 'o' && after_consonant) || (c[i + 1] == 'e' && prev.is_none_or(is_vowel)) {
                out.push('e');
                i += 2;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// Удвоенные буквы — одной (цифры не трогаются): «Анна» = «Ana», «Филипп» = «Filip».
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = None;
    for c in s.chars() {
        if prev == Some(c) && c.is_alphabetic() {
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

/// Латиница по-русски — для русской основы слова, набранного транслитом.
fn to_cyrillic(lat: &str) -> String {
    const MULTI: &[(&str, &str)] = &[
        ("shch", "щ"),
        ("sch", "щ"),
        ("zh", "ж"),
        ("kh", "х"),
        ("ts", "ц"),
        ("tz", "ц"),
        ("ch", "ч"),
        ("sh", "ш"),
        ("yo", "ё"),
        ("jo", "ё"),
        ("yu", "ю"),
        ("ju", "ю"),
        ("iu", "ю"),
        ("ya", "я"),
        ("ja", "я"),
        ("ia", "я"),
        ("ye", "е"),
        ("je", "е"),
        ("ph", "ф"),
        ("ck", "к"),
        ("iy", "ий"),
        ("ij", "ий"),
        ("yy", "ый"),
        ("x", "кс"),
    ];
    let mut out = String::new();
    let mut i = 0;
    'next: while i < lat.len() {
        for (from, to) in MULTI {
            if lat[i..].starts_with(from) {
                out.push_str(to);
                i += from.len();
                continue 'next;
            }
        }
        let c = lat[i..].chars().next().unwrap_or(' ');
        let after_vowel = out.chars().last().is_some_and(|p| "аеёиоуыэюя".contains(p));
        match c {
            'a' => out.push('а'),
            'b' => out.push('б'),
            'v' | 'w' => out.push('в'),
            'g' => out.push('г'),
            'd' => out.push('д'),
            'e' => out.push('е'),
            'z' => out.push('з'),
            'i' => out.push('и'),
            'j' => out.push('й'),
            'k' | 'q' => out.push('к'),
            'l' => out.push('л'),
            'm' => out.push('м'),
            'n' => out.push('н'),
            'o' => out.push('о'),
            'p' => out.push('п'),
            'r' => out.push('р'),
            's' => out.push('с'),
            't' => out.push('т'),
            'u' => out.push('у'),
            'f' => out.push('ф'),
            'h' => out.push('х'),
            'c' => out.push('ц'),
            'y' if after_vowel => out.push('й'),
            'y' => out.push('ы'),
            _ => out.push(c),
        }
        i += c.len_utf8();
    }
    out
}

/// Начала ключей для слова запроса: ключ слова и ключ его основы; у латиницы — ещё ключи русского
/// прочтения. Из начал, где одно продолжает другое, остаётся короткое. `None` — слово короче двух знаков.
fn starts(word: &str) -> Option<Vec<String>> {
    let w = plain(word);
    let whole = key(&w);
    if whole.chars().count() < 2 {
        return None;
    }
    let cyr = w.chars().any(is_cyr);
    let lat = w.chars().any(is_lat);
    let mut out = vec![whole];
    if cyr && !lat {
        out.push(stem_key(&ru_stem(&w), &w));
    } else if lat && !cyr {
        let ascii: String = w.chars().map(|c| deaccent(c).map_or_else(|| c.to_string(), str::to_string)).collect();
        out.push(stem_key(&en_stem(&ascii), &ascii));
        let ru = to_cyrillic(&ascii);
        out.push(key(&ru));
        out.push(stem_key(&ru_stem(&ru), &ru));
    }
    Some(minimal(out))
}

/// Ключ основы; основа короче трёх букв ищет слишком много — тогда ключ всего слова.
fn stem_key(stem: &str, word: &str) -> String {
    let k = key(stem);
    if k.chars().count() >= 3 {
        k
    } else {
        key(word)
    }
}

fn ru_stem(word: &str) -> String {
    Stemmer::create(Algorithm::Russian).stem(word).into_owned()
}

fn en_stem(word: &str) -> String {
    Stemmer::create(Algorithm::English).stem(word).into_owned()
}

/// Без повторов и без начал, которые продолжают более короткое: «otchet*» уже находит всё, что «otcheti*».
fn minimal(mut v: Vec<String>) -> Vec<String> {
    v.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    v.dedup();
    let mut out: Vec<String> = Vec::new();
    for s in v {
        if !out.iter().any(|p| s.starts_with(p.as_str())) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(word: &str) -> String {
        let k = keys(word);
        assert_eq!(k.len(), 1, "{word}: {k:?}");
        k[0].clone()
    }

    fn same(words: &[&str], expected: &str) {
        for w in words {
            assert_eq!(one(w), expected, "ключ слова «{w}»");
        }
    }

    fn finds(query: &str, text: &str) -> bool {
        Query::parse(query).matches(&keys(text))
    }

    #[test]
    fn transliterations_give_one_key() {
        same(&["Отчёт", "отчет", "otchet", "OTCHET"], "otchet");
        same(&["Щукин", "Shchukin", "Schukin", "Ščukin"], "shukin");
        same(&["Юлия", "Yulia", "Yuliya", "Julia", "Iuliia"], "iulia");
        same(&["Ящик", "Yashik", "Jaschik"], "iashik");
        same(&["Алексей", "Alexey", "Aleksei", "Aleksey", "Alexei"], "aleksei");
        same(&["Достоевский", "Dostoevsky", "Dostoyevsky", "Dostoevskiy", "Dostoevskii"], "dostoevski");
        same(&["Елена", "Yelena", "Elena"], "elena");
        same(&["Сергеев", "Sergeyev", "Sergeev"], "sergev");
        same(&["Фёдор", "Федор", "Fyodor", "Fedor"], "fedor");
        same(&["Семён", "Semyon", "Semen"], "semen");
        same(&["Горбачёв", "Gorbachyov", "Gorbachev"], "gorbachev");
        same(&["Хабаровск", "Khabarovsk", "Habarovsk"], "habarovsk");
        same(&["Цветков", "Tsvetkov", "Tzvetkov"], "tsvetkov");
        same(&["Ольга", "Ol'ga", "Olga", "Ol’ga"], "olga");
        same(&["Филипп", "Philipp", "Filip"], "filip");
        same(&["Виктор", "Victor", "Viktor"], "viktor");
        same(&["счёт", "schet", "щёт"], "shet");
        same(&["миллион", "million"], "milen");
        same(&["радио", "radio"], "rade");
    }

    #[test]
    fn lookalike_letters_and_marks() {
        // Латинская «p» в русском слове, русская «о» в английском.
        assert_eq!(one("пpивет"), one("привет"));
        assert_eq!(one("hellо"), one("hello"));
        // «й» и «ё», набранные двумя знаками.
        assert_eq!(one("и\u{306}од"), one("йод"));
        assert_eq!(one("е\u{308}лка"), one("ёлка"));
        assert_eq!(keys("Привет, Ol'ga! Счёт №2025-114."), ["privet", "olga", "shet", "2025", "114"]);
    }

    #[test]
    fn word_forms() {
        for text in ["Отчёт за ноябрь", "два отчёта", "с отчётом", "отчёты готовы", "otchety v prilozhenii"] {
            assert!(finds("отчёт", text), "«отчёт» → «{text}»");
            assert!(finds("отчёты", text), "«отчёты» → «{text}»");
        }
        assert!(finds("Иванов", "Письмо Ивановой"));
        assert!(finds("договоры", "Договор поставки"));
        assert!(finds("встречи", "Встреча в пятницу"));
        assert!(!finds("отчёт", "Счёт на оплату"));
        assert!(!finds("встреча отчёт", "Встреча в пятницу"));
    }

    #[test]
    fn translit_both_ways() {
        assert!(finds("otchet", "Отчёт за ноябрь"));
        assert!(finds("otchety", "два отчёта"));
        assert!(finds("отчёт", "Otchet za noiabr"));
        assert!(finds("Ivanov", "Иванову И. И."));
        assert!(finds("Иванов", "Ivanova Maria"));
        assert!(finds("Fyodor", "Федору Михайловичу"));
        assert!(finds("Щукину", "Shchukin A."));
        assert!(finds("Yulia", "Юлия Петровна"));
        assert!(finds("konec", "конец месяца"));
        assert!(finds("schet", "Счёт на оплату"));
        assert!(finds("Yashik", "Ящик"));
        // Английское слово — английская основа.
        assert!(finds("reports", "Monthly report"));
        assert!(finds("meeting", "Meetings next week"));
    }

    #[test]
    fn short_words_are_skipped() {
        assert!(Query::parse("в и a").is_empty());
        assert_eq!(Query::parse("в отчёт").words().len(), 1);
        assert_eq!(Query::parse("").fts(), None);
    }

    #[test]
    fn fts_expression() {
        assert_eq!(Query::parse("отчёты").fts().as_deref(), Some("\"otchet\"*"));
        // Латиница: английская основа и русское прочтение, если не продолжают одно другое.
        let q = Query::parse("Ivanov отчёт");
        assert_eq!(q.words(), [vec!["ivan".to_string()], vec!["otchet".to_string()]]);
        assert_eq!(q.fts().as_deref(), Some("\"ivan\"* AND \"otchet\"*"));
        // «c» латиницей — и «k» (Victor), и «ц» (konec = конец): ищутся оба прочтения.
        assert_eq!(Query::parse("konec").fts().as_deref(), Some("(\"konek\"* OR \"konets\"*)"));
        assert!(finds("радио", "radio") && finds("radio", "Радио"));
    }

    #[test]
    fn other_layout_fixes_the_keyboard() {
        assert_eq!(other_layout("ghbdtn").as_deref(), Some("привет"));
        assert_eq!(other_layout("ghjljk;fq").as_deref(), Some("продолжай"));
        assert_eq!(other_layout("Jnx`n pf yjz,hm").as_deref(), Some("Отчёт за ноябрь"));
        assert_eq!(other_layout("руддщ").as_deref(), Some("hello"));
        assert_eq!(other_layout("2025"), None);
        assert!(finds(&other_layout("ghbdtn").unwrap(), "Привет! Как дела?"));
    }

    /// FTS5 есть в SQLCipher, которым kit шифрует базу, и понимает выражения [`Query::fts`].
    #[test]
    fn fts5_in_the_encrypted_base() {
        let dir = std::env::temp_dir().join(format!("yashik-search-{}", crate::new_id().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::open(&dir.join("yashik.db"), &[7u8; 32], "aaaa").unwrap();
        let db = store.conn();
        db.execute_batch("CREATE VIRTUAL TABLE mail_fts USING fts5(terms, tokenize = 'ascii')").unwrap();
        let letters = [
            (1, "Отчёт за ноябрь"),
            (2, "Otchety za dekabr. Shchukin"),
            (3, "Привет! Встреча в пятницу. Юлия"),
            (4, "Счёт на оплату №2025-114"),
        ];
        for (id, text) in letters {
            db.execute("INSERT INTO mail_fts(rowid, terms) VALUES (?1, ?2)", (id, index_text(text))).unwrap();
        }
        let find = |query: &str| -> Vec<i64> {
            let Some(expr) = Query::parse(query).fts() else { return Vec::new() };
            let mut st = db.prepare("SELECT rowid FROM mail_fts WHERE mail_fts MATCH ?1 ORDER BY rowid").unwrap();
            st.query_map([expr], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(find("отчёты"), [1, 2]);
        assert_eq!(find("otchet"), [1, 2]);
        assert_eq!(find("Щукину"), [2]);
        assert_eq!(find("Yulia встречу"), [3]);
        assert_eq!(find("schet 2025"), [4]);
        assert_eq!(find("ghbdtn"), Vec::<i64>::new());
        assert_eq!(find(&other_layout("ghbdtn").unwrap()), [3]);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
