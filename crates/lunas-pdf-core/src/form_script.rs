//! Scripts de formulaire : fonctions standard d'Acrobat (`AFNumber_Format`, `AFDate_FormatEx`,
//! `AFRange_Validate`, `AFSimple_Calculate`…), reconnues et réimplémentées.
//!
//! Aucun code venu du PDF n'est exécuté : un script est analysé comme une suite d'appels à ces
//! fonctions, avec des arguments littéraux. Tout autre code est signalé comme personnalisé et
//! ignoré.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Mise en forme d'un champ texte (actions `/F` et `/K`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldFormat {
    /// `AFNumber_*` : `sep_style` 0 « 1,234.56 », 1 « 1234.56 », 2 « 1.234,56 », 3 « 1234,56 »,
    /// 4 « 1'234.56 » ; `neg_style` 0 « - », 1 rouge, 2 parenthèses, 3 parenthèses et rouge.
    Number {
        decimals: u8,
        #[serde(rename = "sepStyle")]
        sep_style: u8,
        #[serde(rename = "negStyle")]
        neg_style: u8,
        currency: String,
        prepend: bool,
    },
    /// `AFPercent_*` : la valeur 0,15 s'affiche « 15 % ».
    Percent {
        decimals: u8,
        #[serde(rename = "sepStyle")]
        sep_style: u8,
    },
    /// `AFDate_*` : format Acrobat (`dd/mm/yyyy`, `mmm d, yyyy`…).
    Date { format: String },
    /// `AFTime_*` : `HH:MM`, `h:MM tt`…
    Time { format: String },
    /// `AFSpecial_*` : 0 code postal, 1 code postal + 4, 2 téléphone, 3 numéro de sécurité
    /// sociale (États-Unis). La valeur ne garde que les chiffres.
    Special { kind: u8 },
    /// `AFSpecial_KeystrokeEx` : masque (9 chiffre, A lettre, O lettre ou chiffre, X tout
    /// caractère, le reste littéral). La valeur est stockée mise en forme.
    Mask { mask: String },
}

/// `AFRange_Validate` (bornes incluses).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct RangeRule {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CalcOp {
    Sum,
    Product,
    Average,
    Min,
    Max,
}

/// `AFSimple_Calculate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Calculation {
    pub op: CalcOp,
    /// Noms de champs (un nom désigne aussi ses descendants : « total » → « total.a »…).
    pub fields: Vec<String>,
}

/// Scripts d'un champ, une fois analysés.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scripts {
    pub format: Option<FieldFormat>,
    pub range: Option<RangeRule>,
    pub calc: Option<Calculation>,
    /// Code non reconnu (ignoré).
    pub custom: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Num(f64),
    Str(String),
    Bool(bool),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
struct Call {
    name: String,
    args: Vec<Arg>,
}

/// Formats prédéfinis de `AFDate_Format(n)` et `AFTime_Format(n)`.
const DATE_FORMATS: [&str; 14] = [
    "m/d",
    "m/d/yy",
    "mm/dd/yy",
    "mm/yy",
    "d-mmm",
    "d-mmm-yy",
    "dd-mmm-yy",
    "yy-mm-dd",
    "mmm-yy",
    "mmmm-yy",
    "mmm d, yyyy",
    "mmmm d, yyyy",
    "m/d/yy h:MM tt",
    "m/d/yy HH:MM",
];
const TIME_FORMATS: [&str; 4] = ["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];

/// Analyse les scripts des actions `/K`, `/F`, `/V` et `/C` d'un champ.
pub fn analyze(keystroke: Option<&str>, format: Option<&str>, validate: Option<&str>, calculate: Option<&str>) -> Scripts {
    let mut s = Scripts::default();
    let mut parse = |js: Option<&str>| -> Vec<Call> {
        let Some(js) = js else { return vec![] };
        let (calls, custom) = parse_script(js);
        s.custom |= custom;
        calls
    };
    let k = parse(keystroke);
    let f = parse(format);
    let v = parse(validate);
    let c = parse(calculate);
    // Le format d'affichage prime ; à défaut, celui de la saisie.
    s.format = f.iter().chain(&k).find_map(format_of);
    s.range = v.iter().find_map(|c| match (c.name.as_str(), c.args.as_slice()) {
        ("AFRange_Validate", [Arg::Bool(gt), Arg::Num(min), Arg::Bool(lt), Arg::Num(max), ..]) => Some(RangeRule {
            min: gt.then_some(*min),
            max: lt.then_some(*max),
        }),
        _ => None,
    });
    s.calc = c.iter().find_map(|c| match (c.name.as_str(), c.args.as_slice()) {
        ("AFSimple_Calculate", [Arg::Str(op), names, ..]) => {
            let op = match op.to_ascii_uppercase().as_str() {
                "SUM" => CalcOp::Sum,
                "PRD" => CalcOp::Product,
                "AVG" => CalcOp::Average,
                "MIN" => CalcOp::Min,
                "MAX" => CalcOp::Max,
                _ => return None,
            };
            let fields = match names {
                Arg::List(l) => l.clone(),
                Arg::Str(s) => s.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect(),
                _ => return None,
            };
            Some(Calculation { op, fields })
        }
        _ => None,
    });
    s
}

fn format_of(c: &Call) -> Option<FieldFormat> {
    let num = |i: usize| match c.args.get(i) {
        Some(Arg::Num(n)) => Some(*n),
        Some(Arg::Bool(b)) => Some(*b as u8 as f64),
        _ => None,
    };
    let text = |i: usize| match c.args.get(i) {
        Some(Arg::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let base = c
        .name
        .trim_end_matches("_Keystroke")
        .trim_end_matches("_Format")
        .trim_end_matches("_KeystrokeEx")
        .trim_end_matches("_FormatEx");
    let ex = c.name.ends_with("Ex");
    Some(match base {
        "AFNumber" => FieldFormat::Number {
            decimals: num(0)?.clamp(0.0, 10.0) as u8,
            sep_style: num(1).unwrap_or(0.0) as u8,
            neg_style: num(2).unwrap_or(0.0) as u8,
            currency: text(4).unwrap_or_default(),
            prepend: matches!(c.args.get(5), Some(Arg::Bool(true))) || num(5) == Some(1.0),
        },
        "AFPercent" => FieldFormat::Percent {
            decimals: num(0)?.clamp(0.0, 10.0) as u8,
            sep_style: num(1).unwrap_or(0.0) as u8,
        },
        "AFDate" if ex => FieldFormat::Date { format: text(0)? },
        "AFDate" => FieldFormat::Date {
            format: DATE_FORMATS.get(num(0)? as usize)?.to_string(),
        },
        "AFTime" if ex => FieldFormat::Time { format: text(0)? },
        "AFTime" => FieldFormat::Time {
            format: TIME_FORMATS.get(num(0)? as usize)?.to_string(),
        },
        "AFSpecial" if ex => FieldFormat::Mask { mask: text(0)? },
        "AFSpecial" => FieldFormat::Special {
            kind: (num(0)? as u8).min(3),
        },
        _ => return None,
    })
}

// --- Analyse du JavaScript --------------------------------------------------------------------

/// Appels `AF…(littéraux)` d'un script ; vrai s'il reste du code non reconnu.
fn parse_script(js: &str) -> (Vec<Call>, bool) {
    let b: Vec<char> = js.chars().collect();
    let mut calls = vec![];
    let mut rest = String::new();
    let mut i = 0;
    while i < b.len() {
        // Commentaires.
        if b[i] == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if b[i] == 'A' && b.get(i + 1) == Some(&'F') && (i == 0 || !is_ident(b[i - 1])) {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let name: String = b[i..j].iter().collect();
            let mut p = Parser { b: &b, i: j };
            p.ws();
            if p.eat('(')
                && let Some(args) = p.args(')')
            {
                calls.push(Call { name, args });
                i = p.i;
                continue;
            }
        }
        rest.push(b[i]);
        i += 1;
    }
    let custom = rest.chars().any(|c| !c.is_whitespace() && c != ';');
    (calls, custom)
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

struct Parser<'a> {
    b: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn word(&mut self, w: &str) -> bool {
        self.ws();
        let end = self.i + w.chars().count();
        if end <= self.b.len()
            && self.b[self.i..end].iter().copied().eq(w.chars())
            && !self.b.get(end).is_some_and(|c| is_ident(*c))
        {
            self.i = end;
            true
        } else {
            false
        }
    }
    /// Arguments jusqu'à `close` (inclus).
    fn args(&mut self, close: char) -> Option<Vec<Arg>> {
        let mut out = vec![];
        if self.eat(close) {
            return Some(out);
        }
        loop {
            out.push(self.arg()?);
            if self.eat(close) {
                return Some(out);
            }
            if !self.eat(',') {
                return None;
            }
        }
    }
    fn arg(&mut self) -> Option<Arg> {
        self.ws();
        let c = *self.b.get(self.i)?;
        if c == '"' || c == '\'' {
            return self.string().map(Arg::Str);
        }
        if self.word("true") {
            return Some(Arg::Bool(true));
        }
        if self.word("false") {
            return Some(Arg::Bool(false));
        }
        if self.eat('[') {
            return self.list(']');
        }
        if self.word("new") {
            if !self.word("Array") || !self.eat('(') {
                return None;
            }
            return self.list(')');
        }
        let start = self.i;
        while self.i < self.b.len() && (self.b[self.i].is_ascii_digit() || "+-.eE".contains(self.b[self.i])) {
            self.i += 1;
        }
        let s: String = self.b[start..self.i].iter().collect();
        s.parse().ok().map(Arg::Num)
    }
    fn list(&mut self, close: char) -> Option<Arg> {
        let items = self.args(close)?;
        items
            .into_iter()
            .map(|a| match a {
                Arg::Str(s) => Some(s),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(Arg::List)
    }
    fn string(&mut self) -> Option<String> {
        let q = self.b[self.i];
        self.i += 1;
        let mut s = String::new();
        while let Some(&c) = self.b.get(self.i) {
            self.i += 1;
            match c {
                c if c == q => return Some(s),
                '\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'u' => {
                            let hex: String = self.b.get(self.i..self.i + 4)?.iter().collect();
                            self.i += 4;
                            s.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                        }
                        other => s.push(other),
                    }
                }
                c => s.push(c),
            }
        }
        None
    }
}

// --- Mise en forme ----------------------------------------------------------------------------

/// Nombre d'une valeur de champ (`AFMakeNumber`) : espaces et apostrophes ignorés, virgule
/// décimale acceptée.
pub fn make_number(v: &str) -> Option<f64> {
    let s: String = v
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\'' && *c != '\u{a0}' && *c != '\u{202f}')
        .collect();
    if s.is_empty() {
        return None;
    }
    let s = if s.contains(',') && !s.contains('.') {
        s.replace(',', ".")
    } else {
        s.replace(',', "")
    };
    s.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Nombre en texte, à la manière de JavaScript (sans zéros inutiles).
pub fn number_text(n: f64) -> String {
    let r = (n * 1e10).round() / 1e10;
    let s = format!("{r:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn group(int: &str, sep: &str) -> String {
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

fn digits(n: f64, decimals: u8, sep_style: u8) -> String {
    let s = format!("{:.*}", decimals as usize, n.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let (g, d) = match sep_style {
        1 => ("", "."),
        2 => (".", ","),
        3 => ("", ","),
        4 => ("'", "."),
        _ => (",", "."),
    };
    let mut out = group(int, g);
    if !frac.is_empty() {
        out.push_str(d);
        out.push_str(frac);
    }
    out
}

/// Texte affiché d'une valeur et indicateur « en rouge » (nombres négatifs).
pub fn display(format: &FieldFormat, value: &str) -> (String, bool) {
    if value.is_empty() {
        return (String::new(), false);
    }
    match format {
        FieldFormat::Number {
            decimals,
            sep_style,
            neg_style,
            currency,
            prepend,
        } => {
            let Some(n) = make_number(value) else {
                return (value.into(), false);
            };
            let body = digits(n, *decimals, *sep_style);
            let body = if *prepend {
                format!("{currency}{body}")
            } else {
                format!("{body}{currency}")
            };
            let neg = n < 0.0 && digits(n, *decimals, 1).chars().any(|c| c.is_ascii_digit() && c != '0');
            match (neg, neg_style) {
                (false, _) => (body, false),
                (true, 1) => (body, true),
                (true, 2) => (format!("({body})"), false),
                (true, 3) => (format!("({body})"), true),
                (true, _) => (format!("-{body}"), false),
            }
        }
        FieldFormat::Percent { decimals, sep_style } => match make_number(value) {
            Some(n) => {
                let sign = if n < 0.0 { "-" } else { "" };
                (format!("{sign}{}%", digits(n * 100.0, *decimals, *sep_style)), false)
            }
            None => (value.into(), false),
        },
        FieldFormat::Special { kind } => {
            let d: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
            let masked = match (kind, d.len()) {
                (0, 5) => Some(d.clone()),
                (1, 9) => Some(format!("{}-{}", &d[..5], &d[5..])),
                (2, 10) => Some(format!("({}) {}-{}", &d[..3], &d[3..6], &d[6..])),
                (2, 7) => Some(format!("{}-{}", &d[..3], &d[3..])),
                (3, 9) => Some(format!("{}-{}-{}", &d[..3], &d[3..5], &d[5..])),
                _ => None,
            };
            (masked.unwrap_or_else(|| value.into()), false)
        }
        // Dates, heures et masques : la valeur est déjà stockée sous sa forme affichée.
        _ => (value.into(), false),
    }
}

/// Résultat d'un calcul sur les valeurs des champs désignés.
pub fn calculate(op: CalcOp, values: &[&str]) -> String {
    let nums: Vec<f64> = values.iter().map(|v| make_number(v).unwrap_or(0.0)).collect();
    let r = match op {
        CalcOp::Sum => nums.iter().sum(),
        CalcOp::Product => nums.iter().product(),
        CalcOp::Average if nums.is_empty() => 0.0,
        CalcOp::Average => nums.iter().sum::<f64>() / nums.len() as f64,
        CalcOp::Min => nums.iter().copied().reduce(f64::min).unwrap_or(0.0),
        CalcOp::Max => nums.iter().copied().reduce(f64::max).unwrap_or(0.0),
    };
    number_text(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_standard_functions() {
        let s = analyze(
            Some("AFNumber_Keystroke(2, 2, 0, 0, \" €\", false);"),
            Some("AFNumber_Format(2, 2, 0, 0, \" €\", false);"),
            Some("AFRange_Validate(true, 0, true, 1000);"),
            None,
        );
        assert_eq!(
            s.format,
            Some(FieldFormat::Number {
                decimals: 2,
                sep_style: 2,
                neg_style: 0,
                currency: " €".into(),
                prepend: false
            })
        );
        assert_eq!(
            s.range,
            Some(RangeRule {
                min: Some(0.0),
                max: Some(1000.0)
            })
        );
        assert!(!s.custom);

        let s = analyze(
            None,
            Some("AFDate_FormatEx(\"dd/mm/yyyy\")"),
            None,
            Some("AFSimple_Calculate(\"SUM\", new Array (\"a\", \"b.c\"));"),
        );
        assert_eq!(
            s.format,
            Some(FieldFormat::Date {
                format: "dd/mm/yyyy".into()
            })
        );
        assert_eq!(
            s.calc,
            Some(Calculation {
                op: CalcOp::Sum,
                fields: vec!["a".into(), "b.c".into()]
            })
        );

        assert_eq!(
            analyze(None, Some("AFDate_Format(10)"), None, None).format,
            Some(FieldFormat::Date {
                format: "mmm d, yyyy".into()
            })
        );
        assert_eq!(
            analyze(None, Some("AFSpecial_Format(2);"), None, None).format,
            Some(FieldFormat::Special { kind: 2 })
        );
        assert_eq!(
            analyze(None, None, None, Some("AFSimple_Calculate('PRD', 'prix, quantite')"))
                .calc
                .unwrap()
                .fields,
            ["prix", "quantite"]
        );
    }

    #[test]
    fn flags_custom_code() {
        assert!(analyze(None, None, None, Some("event.value = this.getField('a').value * 2;")).custom);
        assert!(
            analyze(
                Some("if (event.willCommit) AFNumber_Keystroke(2,0,0,0,'',true);"),
                None,
                None,
                None
            )
            .custom
        );
        // Commentaires et points-virgules seuls : rien de personnalisé.
        assert!(!analyze(None, Some("// généré\nAFPercent_Format(1, 0); /* fin */ ;"), None, None).custom);
    }

    #[test]
    fn formats_values() {
        let eur = FieldFormat::Number {
            decimals: 2,
            sep_style: 2,
            neg_style: 0,
            currency: " €".into(),
            prepend: false,
        };
        assert_eq!(display(&eur, "1234.5").0, "1.234,50 €");
        assert_eq!(display(&eur, "-3").0, "-3,00 €");
        assert_eq!(display(&eur, "").0, "");
        assert_eq!(display(&eur, "abc").0, "abc");
        let usd = FieldFormat::Number {
            decimals: 0,
            sep_style: 0,
            neg_style: 3,
            currency: "$".into(),
            prepend: true,
        };
        assert_eq!(display(&usd, "-1234567"), ("($1,234,567)".into(), true));
        assert_eq!(
            display(
                &FieldFormat::Percent {
                    decimals: 1,
                    sep_style: 3
                },
                "0.155"
            )
            .0,
            "15,5%"
        );
        assert_eq!(display(&FieldFormat::Special { kind: 2 }, "0612345678").0, "(061) 234-5678");
        assert_eq!(display(&FieldFormat::Special { kind: 1 }, "123456789").0, "12345-6789");
    }

    #[test]
    fn numbers_and_calculations() {
        assert_eq!(make_number("1 234,5"), Some(1234.5));
        assert_eq!(make_number("1,234.5"), Some(1234.5));
        assert_eq!(make_number(""), None);
        assert_eq!(number_text(0.1 + 0.2), "0.3");
        assert_eq!(number_text(-0.0), "0");
        assert_eq!(calculate(CalcOp::Product, &["12.5", "3"]), "37.5");
        assert_eq!(calculate(CalcOp::Sum, &["1", "", "Off", "2,5"]), "3.5");
        assert_eq!(calculate(CalcOp::Average, &["1", "2"]), "1.5");
        assert_eq!(calculate(CalcOp::Max, &["1", "-2"]), "1");
    }
}
